#!/usr/bin/env python3
"""Real PVE+PBS disposable-lab provider acceptance. Never run in hosted CI.
Creates NEW VMIDs only. Leaves recovered guests STOPPED for inspection:
no false guest-boot/content certification on a mere restore API receipt.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time

CONFIG=Path("/etc/multi-backup/pve-pbs-field-lab.json")
CLI=Path("rust/target/release/multi-backup-native").resolve()
MARK="mb-field-fixture"
def require(ok,message):
    if not ok:raise RuntimeError(message)
def run(args,root=False,bytes_output=False,limit=4_194_304):
    argv=(["sudo","-n"] if root else [])+[str(x) for x in args]
    result=subprocess.run(argv,capture_output=True,timeout=3600,check=False)
    if result.returncode or len(result.stdout)>limit:
        raise RuntimeError("Native field command failed: "+Path(str(args[0])).name)
    return result.stdout if bytes_output else result.stdout.decode().strip()
def cfg():
    info=CONFIG.stat()
    require(info.st_uid==0 and stat.S_ISREG(info.st_mode) and info.st_mode&0o037==0,
            "PVE/PBS fixture allowlist must be root-owned group-readable 0640")
    return json.loads(CONFIG.read_text())
def preflight(c):
    require(os.getenv("GITHUB_ACTIONS")=="true" and
            os.getenv("RUNNER_ENVIRONMENT")=="self-hosted" and
            os.getenv("MB_PVE_PBS_FIELD")=="YES_ISOLATED_PVE_PBS_TEST_GUESTS" and
            os.geteuid()!=0,
            "Real PVE/PBS test is only permitted on protected dedicated self-hosted lab")
    require(CLI.is_file() and os.access(CLI,os.X_OK),"Exact SHA native Rust CLI absent")
    node=run(["hostname","-s"])
    require(node==c["node"],"Field runner is not on approved PVE node")
    for binary in ("qm","pct","qmrestore","pvesh","pvesm","age-keygen",
                   "proxmox-backup-client"):
        require(run(["which",binary]),"Missing Proxmox provider binary")
    for group in ("vm","ct"):
        source=int(c[f"source_{group}_id"]);target=int(c[f"new_{group}_id"])
        require(100<=source<=999_999_999 and 100<=target<=999_999_999 and
                source!=target,"Invalid PVE VM/CT fixture IDs")
        program="qm" if group=="vm" else "pct"
        desc=run([program,"config",source],root=True)
        require(re.search(r"(?m)^tags:.*\b"+re.escape(MARK)+r"\b",desc)!=None,
                "Source is not explicitly labelled as disposable mb-field-fixture")
        result=subprocess.run(["sudo","-n",program,"config",str(target)],
                              capture_output=True,timeout=20)
        require(result.returncode!=0,"PVE destination guest ID already allocated")
    for name in ("backup_storage","restore_storage","pbs_storage"):
        require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}",c[name])!=None,
                "Invalid fixture storage ID")
    for name in ("pve_api_config","pbs_source_config"):
        require(Path(c[name]).is_absolute() and Path(c[name]).is_file(),
                "Missing administrator-provisioned protected PVE/PBS file")
    for kind in ("vm","ct"):
        snap=c[f"pbs_{kind}"]
        require(100<=int(snap["source_vmid"])<=999_999_999 and
                100<=int(snap["new_vmid"])<=999_999_999,
                "PBS fixture guest IDs invalid")
        require(re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ",snap["time"])!=None,
                "PBS snapshot timestamp invalid")
        program="qm" if kind=="vm" else "pct"
        exists=subprocess.run(["sudo","-n",program,"config",
            str(snap["new_vmid"])],capture_output=True,timeout=20)
        require(exists.returncode!=0,"PBS target NEW VMID was already allocated")
    return node
def parse_ve_json(text):
    data=json.loads(text)
    return data.get("data",data) if isinstance(data,dict) else data
def task_status(node,upid):
    status=parse_ve_json(run(["pvesh","get",f"/nodes/{node}/tasks/{upid}/status",
                           "--output-format","json"],root=True))
    return status
def backup_task(node,vmid,storage):
    before=time.time()
    task=run(["pvesh","create",f"/nodes/{node}/vzdump",
              "--vmid",vmid,"--mode","snapshot","--compress","zstd",
              "--storage",storage],root=True).strip().strip('"')
    require(task.startswith(f"UPID:{node}:"),"PVE task is not exact node UPID")
    for _ in range(100):
        status=task_status(node,task)
        if status.get("status")=="stopped":
            require(status.get("exitstatus")=="OK",
                    "Real PVE snapshot backup completed with non-OK task status")
            return task,before
        time.sleep(3)
    raise RuntimeError("PVE vzdump snapshot never completed")
def backup_volume(storage,kind,vmid,after):
    data=parse_ve_json(run(["pvesm","list",storage,"--content",
                             "backup","--output-format","json"],root=True))
    prefix=f"vzdump-{'qemu' if kind=='vm' else 'lxc'}-{vmid}-"
    suffix=".vma.zst" if kind=="vm" else ".tar.zst"
    rows=[r for r in data if isinstance(r,dict) and
          r.get("volid","").split("/")[-1].startswith(prefix) and
          r.get("volid","").endswith(suffix) and int(r.get("ctime",0))>=after-30]
    require(len(rows)==1,"Exact new PVE snapshot has ambiguous/absent media inventory")
    volid=rows[0]["volid"]
    path=Path(run(["pvesm","path",volid],root=True))
    require(path.is_absolute() and path.is_file() and not path.is_symlink(),
            "PVE backup media path invalid")
    digest=hashlib.sha256()
    with path.open("rb") as f:
        while True:
            b=f.read(1024*1024)
            if not b:break
            digest.update(b)
    return volid,path.stat().st_size,digest.hexdigest()
def native(args):
    return parse_ve_json(run([CLI,*args],root=True))
def main():
    require(os.getenv("GITHUB_ACTIONS")=="true" and
            os.getenv("RUNNER_ENVIRONMENT")=="self-hosted" and
            os.getenv("MB_PVE_PBS_FIELD")=="YES_ISOLATED_PVE_PBS_TEST_GUESTS",
            "Refusing unapproved field run even before fixture loading")
    c=cfg()
    node=preflight(c)
    with tempfile.TemporaryDirectory(prefix="mb-pve-pbs-field-") as scratch:
        base=Path(scratch)
        run(["age-keygen","-o",base/"identity"])
        pub=run(["age-keygen","-y",base/"identity"])
        # Store and temp area are local ephemeral and only used by THIS fixture.
        native(["storage","init","--root",base/"store","--recipient",pub])
        results=[]
        for kind in ("vm","ct"):
            source=int(c[f"source_{kind}_id"])
            dest=int(c[f"new_{kind}_id"])
            upid,started=backup_task(node,source,c["backup_storage"])
            volid,bytes_count,sha=backup_volume(c["backup_storage"],kind,source,started)
            receipt=native(["plugins","pve-media-backup",
                "--config-json",c["pve_api_config"],"--node",node,
                "--storage",c["backup_storage"],"--guest-kind",
                "qemu" if kind=="vm" else "lxc","--guest-id",str(source),
                "--upid",upid,"--volid",volid,"--sha256",sha,
                "--bytes",str(bytes_count),"--root",base/"store",
                "--identity",base/"identity","--label",f"field-pve-{kind}","--json"])
            require(receipt.get("encrypted") and receipt.get("bytes")==bytes_count,
                    "PVE media not verified or encrypted")
            recovered=native(["plugins","pve-restore-new-guest",
                "--root",base/"store","--identity",base/"identity",
                "--id",receipt["id"],"--new-guest-id",str(dest),
                "--storage",c["restore_storage"],"--stage-root",base,
                "--confirm-new-guest",
                f"{'qemu' if kind=='vm' else 'lxc'}:{dest}:{c['restore_storage']}",
                "--json"])
            require(recovered.get("created_new_guest") and not recovered.get("guest_started"),
                    "Native PVE new guest was not created with autostart OFF")
            output=run(["qm" if kind=="vm" else "pct","config",dest],root=True)
            require(output,"PVE newly restored guest has no native configuration")
            results.append({"family":f"PVE-{kind}","source":source,"restored":dest,
                 "encrypted":True,"new_guest":True,"boot_certified":False})
        pbs=c["pbs_source_config"]
        receipt=native(["plugins","pbs-disk-backup",
                        "--snapshot-json",pbs,
                        "--root",base/"store","--identity",base/"identity",
                        "--label","field-pbs-real-vm-disk","--json"])
        require(receipt.get("provider")=="pbs" and receipt.get("native_pbs_read_verified"),
                "Official PBS client did not verify native VM image")
        recovered=native(["plugins","pbs-disk-restore-offline",
                "--root",base/"store","--identity",base/"identity",
                "--id",receipt["id"],"--destination",base/"pbs-recovered","--json"])
        require(recovered.get("bytes")==receipt.get("bytes"),
                "PBS image bytes changed during age recovery")
        rows=list((base/"pbs-recovered").rglob("*.img.fidx"))
        require(len(rows)==1,"PBS recovered exactly one raw VM image archive")
        expected=json.loads(run(["cat",pbs],root=True))["sha256"]
        with rows[0].open("rb") as stream:
            h=hashlib.file_digest(stream,"sha256").hexdigest()
        require(h==expected,"PBS encrypted VM disk SHA256 changed")
        results.append({"family":"PBS-native-client-VM-disk",
                        "encrypted":True,"disk_verified":True,
                        "guest_boot_certified":False})
        for kind in ("vm","ct"):
            b=c[f"pbs_{kind}"]
            source=int(b["source_vmid"]);target=int(b["new_vmid"])
            restored=native(["plugins","pbs-restore-new-pve-guest",
                "--source-storage",c["pbs_storage"],"--kind",kind,
                "--source-vmid",str(source),"--snapshot-time",b["time"],
                "--new-vmid",str(target),"--destination-storage",
                c["restore_storage"],"--confirm-new-guest",
                f"{kind}:{target}:{c['restore_storage']}","--json"])
            require(restored.get("native_pve_guest_created") and
                    not restored.get("guest_boot_verified"),
                    "PVE did not accept native PBS snapshot as NEW guest")
            config=run(["qm" if kind=="vm" else "pct","config",target],root=True)
            require(config,"PBS-backed restored guest has no native PVE config")
            results.append({"family":f"PBS-PVE-{kind}","new_guest":target,
                            "boot_certified":False,"age_roundtrip":False})
        print(json.dumps({"result":"PASS provider-mediated VM/CT CREATE-ONLY",
                          "node":node,"results":results,
                          "requires_manual_guest_boot_and_data_verification":True,
                          "new_guests_left_stopped_for_operator_inspection":True},
                         indent=2))
if __name__=="__main__":
    try:main()
    except Exception as e:
        print("FAIL real PVE/PBS field: "+str(e),file=sys.stderr)
        sys.exit(1)
