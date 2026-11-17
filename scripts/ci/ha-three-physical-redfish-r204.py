#!/usr/bin/env python3
"""Real 3-bare-metal Patroni disaster drill with TLS-pinned Redfish ForceOff.
Requires a protected self-hosted runner, dedicated lab host allowlist, explicit
manual authorization and 3 independently validated BMCs. Never production CI.
"""
import argparse
import base64
import json
import os
import re
import ssl
import stat
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

AUTHORIZED = "YES_POWER_CYCLE_ONE_ISOLATED_TEST_HOST"
CONFIG = Path("/etc/multi-backup/physical-ha-lab.json")
HOST = re.compile(r"^[A-Za-z0-9][A-Za-z0-9.-]{0,252}$")
def canonical(v):
    return re.sub("[^0-9a-f]","",str(v).lower())
def ssh(host,script,timeout=50):
    if not HOST.fullmatch(host) or ".." in host:
        raise RuntimeError("Unsafe physical host")
    p=subprocess.run(["ssh","-o","BatchMode=yes","-o","StrictHostKeyChecking=yes",
        "-o","ForwardAgent=no","-o","ConnectTimeout=7",host,"sh","-s"],
        input=script,text=True,capture_output=True,timeout=timeout)
    if p.returncode or len(p.stdout)>100000:
        raise RuntimeError("Physical-host SSH command or trusted host key failed")
    return p.stdout.strip()
HARDWARE = """set -eu
printf 'name=%s\\n' "$(hostname -s)"
printf 'machine=%s\\n' "$(cat /etc/machine-id)"
printf 'boot=%s\\n' "$(cat /proc/sys/kernel/random/boot_id)"
printf 'uuid=%s\\n' "$(cat /sys/class/dmi/id/product_uuid)"
printf 'chassis=%s\\n' "$(cat /sys/class/dmi/id/chassis_serial)"
if systemd-detect-virt -q;then echo 'physical=NO';else echo 'physical=YES';fi
"""
def hardware(host):
    lines=[line.split("=",1) for line in ssh(host,HARDWARE).splitlines()]
    if len(lines)!=6 or any(len(line)!=2 for line in lines):
        raise RuntimeError("Duplicate or malformed bare-metal attestation")
    data=dict(lines)
    if set(data)!={"name","machine","boot","uuid","chassis","physical"}:
        raise RuntimeError("Incomplete bare-metal DMI/boot proof")
    if data["physical"]!="YES" or len(canonical(data["uuid"]))!=32 or \
       len(data["chassis"])<5:
        raise RuntimeError("VM or unidentified physical hardware refused")
    return data
class BlockRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*a,**kw):
        raise RuntimeError("Unexpected Redfish TLS redirect")
class Redfish:
    def __init__(self,rec,ca,user,password):
        self.url=rec["url"]
        self.path=rec["system"]
        u=urllib.parse.urlparse(self.url)
        if u.scheme!="https" or not u.hostname or u.username or u.password or \
           u.query or u.fragment or u.path not in ("","/"):
            raise RuntimeError("Redfish endpoint must be a bare HTTPS origin")
        if not self.path.startswith("/redfish/v1/Systems/") or ".." in self.path or \
           "?" in self.path or "#" in self.path:
            raise RuntimeError("Invalid pinned Redfish system URI")
        if not user or not password or len(password)<12:
            raise RuntimeError("Missing BMC credentials in protected runner environment")
        self.header="Basic "+base64.b64encode((user+":"+password).encode()).decode()
        context=ssl.create_default_context(cafile=ca)
        self.client=urllib.request.build_opener(urllib.request.HTTPSHandler(context=context),
                                               BlockRedirect())
    def query(self,method,path,data=None):
        raw=json.dumps(data).encode() if data is not None else None
        request=urllib.request.Request(self.url+path,method=method,data=raw,
            headers={"Authorization":self.header,"Content-Type":"application/json",
                     "Accept":"application/json"})
        with self.client.open(request,timeout=25) as resp:
            if resp.status not in (200,201,202,204):
                raise RuntimeError("Redfish BMC refused authorized power operation")
            body=resp.read(65537)
            if len(body)>65536:raise RuntimeError("Redfish response too large")
            return json.loads(body) if body else {}
    def status(self):return self.query("GET",self.path)
    def power(self,mode):
        if mode not in ("ForceOff","On"):raise RuntimeError("Invalid Redfish power action")
        self.query("POST",self.path+"/Actions/ComputerSystem.Reset",
                   {"ResetType":mode})
def members(host):
    data=json.loads(ssh(host,
        "set -eu\npatronictl -c /etc/patroni/patroni.yml list -f json\n"))
    if not isinstance(data,list) or len(data)!=3:
        raise RuntimeError("Patroni DCS did not report three physical members")
    return data
def leader(host):
    rows=members(host)
    names=[x.get("Member",x.get("member")) for x in rows
        if str(x.get("Role",x.get("role",""))).lower() in ("leader","primary")]
    if len(names)!=1 or not names[0]:
        raise RuntimeError("Split brain or no Patroni primary")
    return names[0]
def sql(host,query):
    if not re.fullmatch(r"[A-Za-z0-9_ ,()*='.-]+",query):
        raise RuntimeError("Unsafe witness SQL")
    return ssh(host,"set -eu\nsudo -n -u postgres psql -XAt -v ON_ERROR_STOP=1"
                    " -d postgres -c '"+query+"'\n",timeout=40)
def wait_for(label,check,timeout):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        try:
            answer=check()
            if answer:return answer
        except Exception:
            pass
        time.sleep(4)
    raise RuntimeError(label+" timed out; physically isolate and inspect lab")
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--hosts",nargs=3,required=True)
    p.add_argument("--authorize",required=True)
    args=p.parse_args()
    hosts=args.hosts
    if args.authorize!=AUTHORIZED or os.getenv("MB_EXPLICIT_PHYSICAL_LAB")!=AUTHORIZED or \
       os.getenv("RUNNER_ENVIRONMENT")!="self-hosted" or \
       os.getenv("GITHUB_ACTIONS")!="true" or os.geteuid()==0:
        raise RuntimeError("Denied: dedicated protected manual physical lab required")
    if len(set(hosts))!=3 or any(not HOST.fullmatch(h) for h in hosts):
        raise RuntimeError("Expected three distinct physical host addresses")
    meta=CONFIG.lstat()
    if not stat.S_ISREG(meta.st_mode) or meta.st_uid!=0 or meta.st_mode&0o037:
        raise RuntimeError("Lab allowlist must be root-owned 0640, group-readable by dedicated CI runner")
    config=json.loads(CONFIG.read_text())
    if sorted(config.get("hosts",{}))!=sorted(hosts):
        raise RuntimeError("Selected hosts are not exactly those approved in lab allowlist")
    ca=config.get("ca_bundle")
    if not ca or not Path(ca).is_file():raise RuntimeError("Pinned Redfish CA missing")
    hw=[];bmc=[]
    for n,host in enumerate(hosts,1):
        info=hardware(host);rec=config["hosts"][host]
        if canonical(rec["product_uuid"])!=canonical(info["uuid"]):
            raise RuntimeError("Host DMI does not match dedicated approved hardware")
        remote=Redfish(rec,ca,os.getenv("MB_BMC"+str(n)+"_USER"),
                     os.getenv("MB_BMC"+str(n)+"_PASSWORD"))
        status=remote.status()
        if canonical(status.get("UUID"))!=canonical(info["uuid"]) or \
           status.get("PowerState")!="On":
            raise RuntimeError("BMC/host physical identity or power state mismatch")
        hw.append(info);bmc.append(remote)
    for field in ("machine","boot","uuid","chassis"):
        if len({record[field] for record in hw})!=3:
            raise RuntimeError("Three independent physical host identity proofs failed")
    # Reuse existing live three-host Patroni quorum preflight.
    pre=subprocess.run([sys.executable,"scripts/ci/ha-three-host-field-r166.py",
                       "--hosts",*hosts],capture_output=True,text=True,timeout=180)
    if pre.returncode:raise RuntimeError("Read-only three-host Patroni preflight failed")
    lead=leader(hosts[0])
    if any(leader(host)!=lead for host in hosts):
        raise RuntimeError("Patroni primary view inconsistent")
    index=next((i for i,x in enumerate(hw) if x["name"]==lead),None)
    if index is None:raise RuntimeError("Primary has no uniquely mapped physical BMC")
    if not sql(hosts[index],"SHOW synchronous_standby_names").strip():
        raise RuntimeError("Physical outage test requires synchronous standby acknowledgement")
    nonce=uuid.uuid4().hex
    sql(hosts[index],"CREATE TABLE IF NOT EXISTS public.mb_physical_witness"
        "(id text primary key,payload text not null)")
    sql(hosts[index],"INSERT INTO public.mb_physical_witness VALUES "
        "('"+nonce+"','acknowledged-before-power-off')")
    survivors=[h for i,h in enumerate(hosts) if i!=index]
    query="SELECT count(*) FROM public.mb_physical_witness WHERE id='"+nonce+"'"
    if not any(sql(x,query).strip()=="1" for x in survivors):
        raise RuntimeError("No synchronous witness on the standby before fault")
    powered_off=False
    try:
        print("ATTESTED: three physical host DMI identities and pinned HTTPS Redfish BMC",flush=True)
        # If the HTTPS response is lost AFTER BMC accepted ForceOff, the
        # PowerOn recovery must still be attempted in finally.
        powered_off=True
        bmc[index].power("ForceOff")
        wait_for("Real primary hardware PowerState=Off",
                 lambda:bmc[index].status().get("PowerState")=="Off",100)
        def new_primary():
            first,second=leader(survivors[0]),leader(survivors[1])
            return first if first==second and first!=lead else None
        promotion=wait_for("Single surviving Patroni leader",new_primary,180)
        witness_host=next((h for h,x in zip(hosts,hw) if x["name"]==promotion),None)
        if witness_host not in survivors or sql(witness_host,query).strip()!="1":
            raise RuntimeError("Synchronous PostgreSQL ACK lost after physical primary outage")
        print("PASS primary physical ForceOff, new leader elected, acknowledged data preserved",flush=True)
    finally:
        if powered_off:
            bmc[index].power("On")
            print("RECOVERY: Redfish PowerOn issued to exactly one test host",flush=True)
    wait_for("BMC PowerState On",lambda:bmc[index].status().get("PowerState")=="On",180)
    def healthy_rejoin():
        leads=[leader(h) for h in hosts]
        if len(set(leads))!=1 or leads[0]==lead:return False
        rows=members(hosts[index])
        return all(str(m.get("State",m.get("state",""))).lower() in
                   ("running","streaming") for m in rows) and any(
                   m.get("Member",m.get("member"))==lead and
                   str(m.get("Role",m.get("role",""))).lower() not in
                   ("leader","primary") for m in rows)
    wait_for("Old physical node rejoined as healthy replica of surviving leader",
             healthy_rejoin,300)
    if hardware(hosts[index])["boot"]==hw[index]["boot"]:
        raise RuntimeError("No physical kernel boot-id change despite power fault")
    print("PASS 3 bare-metal nodes, real physical power fault, Patroni fencing and recovery",flush=True)
if __name__=="__main__":
    try:main()
    except Exception as e:
        print("FAIL three physical host lab: "+str(e),file=sys.stderr)
        sys.exit(1)
