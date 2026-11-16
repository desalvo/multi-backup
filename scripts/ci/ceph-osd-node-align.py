#!/usr/bin/env python3
"""Preflight actual Ceph OSD Kubernetes node selection on disposable kind ONLY.

The caller may pin a Rook OSD Deployment to its node only when Ceph reports
three genuinely running/up/in OSDs and the pod's block path identifies the
exact throwaway loop device that the caller created on one dedicated worker.
The final independent Ceph attestation is NEVER bypassed.
"""
import json
import pathlib
import re
import sys

WORKER = re.compile(r"mb-ceph-rgw-worker(?:[2-9][0-9]*)?")
DEVICE = re.compile(r"loop[0-9]+")

def desired_nodes(status,tree,pods,bindings):
    if len(bindings)!=3 or len({v[0] for v in bindings})!=3 or (
        len({v[1] for v in bindings})!=3):
        raise ValueError("Expected 3 distinct known worker/device bindings")
    wanted={}
    for worker,dev in bindings:
        if not WORKER.fullmatch(worker) or not DEVICE.fullmatch(dev):
            raise ValueError("Unsafe node or non-loop device binding")
        wanted[dev]=worker
    osdmap=(status.get("osdmap") or {}).get("osdmap") or status.get("osdmap") or {}
    if len(set(status.get("quorum_names") or []))<3 or any(
      int(osdmap.get(k,0))<3 for k in ("num_osds","num_up_osds","num_in_osds")):
        raise ValueError("Ceph does not have 3 genuine MON/OSD daemons")
    osds={int(n["id"]) for n in tree.get("nodes",[])
          if n.get("type")=="osd" and n.get("status")=="up"}
    if len(osds)!=3:raise ValueError("Ceph tree lacks 3 active OSD daemons")
    observed={}
    for pod in pods.get("items",[]):
        if pod.get("status",{}).get("phase")!="Running":
            raise ValueError("Non-running OSD cannot be pinned")
        node=pod.get("spec",{}).get("nodeName","")
        if node not in wanted.values():raise ValueError("Unexpected kind OSD node")
        entries=[]
        for container in pod.get("spec",{}).get("containers",[]):
            env={x.get("name"):x.get("value") for x in container.get("env",[])
                 if "value" in x}
            if "ROOK_OSD_ID" in env:entries.append(env)
        if len(entries)!=1:raise ValueError("No unique OSD identity in real pod")
        data=entries[0]
        identifier=str(data["ROOK_OSD_ID"])
        if not identifier.isascii() or not identifier.isdigit():
            raise ValueError("Invalid OSD identifier")
        id=int(identifier)
        block=data.get("ROOK_BLOCK_PATH","")
        if not block.startswith("/dev/") or block[5:] not in wanted:
            raise ValueError("Unrecognized or unsafe OSD block device")
        if id not in osds or id in observed:
            raise ValueError("Missing/duplicated OSD daemon")
        if not pod.get("metadata",{}).get("name","").startswith(f"rook-ceph-osd-{id}-"):
            raise ValueError("OSD pod name does not match daemon ID")
        crush_hostname=data.get("ROOK_CRUSHMAP_HOSTNAME","")
        if not WORKER.fullmatch(crush_hostname):
            raise ValueError("Missing or unsafe OSD CRUSH hostname in live daemon")
        observed[id]=(node,wanted[block[5:]],block,crush_hostname)
    if len(observed)!=3 or len({v[2] for v in observed.values()})!=3:
        raise ValueError("Three distinct live OSDs and distinct dedicated loop devices required")
    # Kubernetes scheduling alone cannot fix stale ROOK_CRUSHMAP_HOSTNAME.
    # The daemon otherwise reasserts its former logical CRUSH host.
    return [(id,target) for id,(_,target,_,_) in sorted(observed.items())
            if observed[id][0]!=target or observed[id][3]!=target]

if __name__=="__main__":
    try:
        if len(sys.argv)!=7:raise ValueError("Expected status, tree, pods and three worker:loop bindings")
        docs=[json.loads(pathlib.Path(p).read_text()) for p in sys.argv[1:4]]
        bindings=[]
        for entry in sys.argv[4:]:
            if entry.count(":")!=1:raise ValueError("Malformed worker:loop binding")
            bindings.append(tuple(entry.split(":")))
        for id,worker in desired_nodes(*docs,bindings):
            print(id,worker)
    except (ValueError,KeyError,OSError) as ex:
        print(f"REFUSED OSD scheduling change without independent evidence: {ex}",file=sys.stderr)
        raise SystemExit(3) from None
