#!/usr/bin/env python3
"""One-shot kind-only CRUSH correction from independent live OSD pod proof.

Not a Ceph mock, and not an attestation of three physical failure domains.
"""
import json
import pathlib
import re
import sys

WORKER=re.compile(r"mb-ceph-rgw-worker(?:[2-9][0-9]*)?")
LOOP=re.compile(r"loop[0-9]+")

def reconcile(status,tree,pods,bindings):
    if len(bindings)!=3 or len(set(bindings))!=3:
        raise ValueError("exactly three distinct worker/device bindings required")
    expected={}
    for node,device in bindings:
        if not WORKER.fullmatch(node) or not LOOP.fullmatch(device):
            raise ValueError("unsafe node/device name")
        if node in expected or device in expected.values():
            raise ValueError("device used by two workers")
        expected[node]=device
    osdmap=(status.get("osdmap") or {}).get("osdmap") or status.get("osdmap") or {}
    if len(set(status.get("quorum_names") or []))<3 or any(
        int(osdmap.get(k,0))<3 for k in ("num_osds","num_up_osds","num_in_osds")):
        raise ValueError("genuine three-MON and UP+IN OSD preflight failed")
    osds={int(n["id"]) for n in tree.get("nodes",[])
          if n.get("type")=="osd" and n.get("status")=="up"}
    if len(osds)!=3: raise ValueError("CRUSH tree lacks exactly 3 UP OSDs")
    active=[]
    for pod in pods.get("items",[]):
        node=pod.get("spec",{}).get("nodeName","")
        if node not in expected or pod.get("status",{}).get("phase")!="Running":
            raise ValueError("OSD not Running on allowed worker")
        # Phase=Running alone includes replacement pods whose OSD is not yet
        # ready. Moving CRUSH too early lets a late OSD startup undo the move.
        statuses=[c for c in pod.get("status",{}).get("containerStatuses",[])
                  if c.get("name")=="osd" and c.get("ready") is True
                  and isinstance(c.get("state",{}).get("running"),dict)]
        if len(statuses)!=1:
            raise ValueError("OSD container is not ready and running")
        entries=[]
        for container in pod.get("spec",{}).get("containers",[]):
            env={x.get("name"):x.get("value") for x in container.get("env",[])
                 if "value" in x}
            if "ROOK_OSD_ID" in env:entries.append(env)
        if len(entries)!=1: raise ValueError("OSD identity missing/ambiguous")
        env=entries[0]
        if not str(env["ROOK_OSD_ID"]).isascii() or not str(env["ROOK_OSD_ID"]).isdigit():
            raise ValueError("invalid OSD id")
        osd=int(env["ROOK_OSD_ID"])
        if osd not in osds or env.get("ROOK_BLOCK_PATH")!="/dev/"+expected[node]:
            raise ValueError("OSD ID or disposable block device mismatch")
        if env.get("ROOK_CRUSHMAP_HOSTNAME")!=node:
            raise ValueError("OSD daemon CRUSH hostname does not match verified worker")
        active.append((osd,node))
    if len(active)!=3 or len(set(active))!=3 or len({a[0] for a in active})!=3 or (
        len({a[1] for a in active})!=3 or {a[0] for a in active}!=osds):
        raise ValueError("OSDs not independently scheduled one per worker")
    current={}
    for n in tree.get("nodes",[]):
        if n.get("type")=="host":
            for child in n.get("children") or []:
                if child in current:raise ValueError("duplicate host in CRUSH tree")
                current[child]=n.get("name")
    return [(id,node) for id,node in sorted(active) if current.get(id)!=node]

if __name__=="__main__":
    try:
        if len(sys.argv)!=7:
            raise ValueError("usage: guard.py STATUS TREE PODS WORKER:LOOP x3")
        status,tree,pods=[json.loads(pathlib.Path(p).read_text()) for p in sys.argv[1:4]]
        bindings=[]
        for arg in sys.argv[4:]:
            if arg.count(":")!=1:raise ValueError("invalid binding")
            bindings.append(tuple(arg.split(":")))
        for osd,node in reconcile(status,tree,pods,bindings):
            print(f"{osd} {node}")
    except (ValueError,KeyError,OSError) as exc:
        print(f"REFUSED unsafe CRUSH move: {exc}",file=sys.stderr)
        raise SystemExit(3) from None
