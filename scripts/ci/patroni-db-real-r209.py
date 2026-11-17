#!/usr/bin/env python3
"""True 3-node Patroni/etcd TLS + PostgreSQL16 native encrypted B-In and
create-only restore against healthy leader; refuse replica and wrong age key.
This is a hosted isolated process HA lab, not independent bare-metal hosts.
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ORIGINAL=Path(__file__).with_name("patroni-etcd-real-ha.py")
CLI=Path("rust/target/debug/multi-backup-native").resolve()
CERT=Path("/usr/local/share/ca-certificates/mb-patroni-r209.crt")

def cmd(*argv,okay=True,env=None,timeout=45):
    p=subprocess.run(list(map(str,argv)),env=env,capture_output=True,
                     text=True,timeout=timeout)
    if okay and p.returncode:
        # No passwords or potentially sensitive DB contents in user-visible errors.
        raise RuntimeError("Native Patroni database acceptance command failed: "+str(argv[:3]))
    return p

def private(path,obj):
    path.write_text(json.dumps(obj))
    path.chmod(0o600)

def run():
    if os.environ.get("GITHUB_ACTIONS")!="true" or os.environ.get("RUNNER_ENVIRONMENT")!="github-hosted":
        raise RuntimeError("Only isolated GitHub hosted real Patroni lab accepted")
    if os.geteuid()==0 or not CLI.is_file():
        raise RuntimeError("Unprivileged native Rust executable mandatory")
    spec=importlib.util.spec_from_file_location("patroni_r209",ORIGINAL)
    ha=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ha)
    executed=[]
    def guard(root,primary,processes):
        # Install this runner's disposable CA so BOTH Rust HTTPS and libpq TLS
        # validate actual certs rather than disabling verification.
        cmd("sudo","-n","install","-m","0644",root/"ca.crt",CERT)
        cmd("sudo","-n","update-ca-certificates",timeout=45)
        def all_healthy():
            status,body=ha.patroni_https(root,primary,"cluster")
            if status!=200:
                return False
            cluster=json.loads(body)
            members=cluster.get("members",[])
            return len(members)==3 and all(
                m.get("state") in ("running","streaming") for m in members)
        ha.wait_for("all 3 genuine Patroni members running/streaming before native DB snapshot",
                    all_healthy,processes,90)
        with tempfile.TemporaryDirectory(prefix="mb-pg-patroni-db-") as w:
            w=Path(w)
            age=w/"age-key"
            cmd("age-keygen","-o",age)
            recipient=cmd("age-keygen","-y",age).stdout.strip()
            store=w/"store"
            cmd(CLI,"storage","init","--root",store,
                "--recipient",recipient)
            ca=w/"ca.crt"
            shutil.copyfile(root/"ca.crt",ca)
            ca.chmod(0o600)
            host=ha.ADDRESSES[primary]
            port=ha.PGPORTS[primary]
            rest=ha.RESTPORTS[primary]
            pgpass=w/"pgpass"
            pgpass.write_text(f"{host}:{port}:*:postgres:{ha.DB_PASSWORD}\n")
            pgpass.chmod(0o600)
            base={"engine":"postgresql","host":host,"port":port,
                  "user":"postgres","database":"postgres",
                  "credentials_file":str(pgpass),"tls_ca_file":str(ca),
                  "patroni_api":f"https://{host}:{rest}/",
                  "patroni_expected_members":3}
            src=w/"source.json";private(src,base)
            source=cmd(CLI,"plugins","database-backup",
                       "--profile-json",src,"--root",store,
                       "--identity",age,"--label","real-patroni-3-nodes",
                       "--json",timeout=110)
            item=json.loads(source.stdout)
            backup_id=item["id"]
            if not isinstance(backup_id,str) or len(backup_id)!=64:
                raise RuntimeError("Native PostgreSQL encrypted backup ID invalid")
            cmd(CLI,"storage","verify-chain","--root",store,
                "--identity",age,"--id",backup_id,timeout=100)

            # A 200 response from a replica's generic /health endpoint
            # must NOT be mistaken for Patroni /primary leader-lock proof.
            replica=next(i for i in range(3) if i!=primary)
            bad=dict(base,host=ha.ADDRESSES[replica],
                     port=ha.PGPORTS[replica],
                     patroni_api=f"https://{ha.ADDRESSES[replica]}:{ha.RESTPORTS[replica]}/")
            deny=w/"replica.json";private(deny,bad)
            result=cmd(CLI,"plugins","database-backup",
                       "--profile-json",deny,"--root",store,
                       "--identity",age,"--label","unsafe-replica",
                       "--json",okay=False,timeout=35)
            if result.returncode==0:
                raise RuntimeError("UNSAFE: replica Patroni /primary allowed backup")

            restored_db="mb_patroni_r209_restored"
            target=dict(base,database=restored_db)
            target_path=w/"target.json";private(target_path,target)
            confirm=f"postgresql:{host}:{port}:{restored_db}"
            result=cmd(CLI,"plugins","database-restore-new",
                       "--profile-json",target_path,"--root",store,
                       "--identity",age,"--id",backup_id,
                       "--confirm-target",confirm,"--json",timeout=100)
            restored=json.loads(result.stdout)
            if restored.get("overwrote_existing") is not False:
                raise RuntimeError("Restore did not attest create-only")
            env=os.environ.copy()
            env.update({"PGPASSWORD":ha.DB_PASSWORD,"PGSSLMODE":"verify-full",
                        "PGSSLROOTCERT":str(ca)})
            query=cmd("psql","-X","-A","-t","-v","ON_ERROR_STOP=1",
                      "-h",host,"-p",str(port),"-U","postgres",
                      "-d",restored_db,"-c",
                      "SELECT payload FROM ha_witness WHERE id=1",
                      env=env,timeout=15)
            if query.stdout.strip()!="acknowledged-before-failover":
                raise RuntimeError("Restored Patroni data missing acknowledged synchronous write")
            bad_restore=cmd(CLI,"plugins","database-restore-new",
                       "--profile-json",target_path,"--root",store,
                       "--identity",age,"--id",backup_id,
                       "--confirm-target",confirm,okay=False,timeout=35)
            if bad_restore.returncode==0:
                raise RuntimeError("UNSAFE: duplicate Patroni recovery overwrote database")
            executed.append(True)
            print("PASS actual 3x Patroni/etcd TLS leader-locked PostgreSQL B-In, verified age chunks, refused replica, create-only DB restore and SQL evidence",flush=True)

    ha.db_consistency_guard_hook=guard
    try:
        ha.main()
    finally:
        cmd("sudo","-n","rm","-f","--",CERT,okay=False)
        cmd("sudo","-n","update-ca-certificates",okay=False,timeout=45)
    if not executed:
        raise RuntimeError("Real Rust Patroni backup integration callback never executed")
    print("PASS actual Patroni automated failover plus encrypted database restore acceptance",flush=True)

if __name__=="__main__":
    run()
