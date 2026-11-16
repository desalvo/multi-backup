#!/usr/bin/env python3
"""Ceph evidence must prove a genuine 3-MON, 3-OSD, 3-host cluster."""
import json
import pathlib
import sys


def attest(status: dict, tree: dict) -> dict:
    monmap = status.get("monmap") or {}
    mons = monmap.get("mons") or []
    quorum = status.get("quorum") or []
    quorum_names = status.get("quorum_names") or []
    if len(mons) < 3 or len(quorum) < 3 or len(set(quorum_names)) < 3:
        raise ValueError("Ceph MON membership/quorum smaller than three")
    osd = (status.get("osdmap") or {}).get("osdmap") or status.get("osdmap") or {}
    if any(int(osd.get(key, 0)) < 3
           for key in ("num_osds", "num_up_osds", "num_in_osds")):
        raise ValueError("Ceph does not report three OSDs UP and IN")
    nodes = tree.get("nodes") or []
    osds = {int(v["id"]) for v in nodes
            if v.get("type") == "osd" and v.get("status") == "up"}
    hosts = {}
    for node in nodes:
        if node.get("type") != "host":
            continue
        name = str(node.get("name", ""))
        hosts[name] = set(node.get("children") or []) & osds
    active_hosts = sorted(name for name, children in hosts.items() if children)
    if len(active_hosts) < 3 or len(osds) < 3:
        raise ValueError("Ceph CRUSH tree lacks three hosts with UP OSDs")
    if len(set(active_hosts)) != len(active_hosts):
        raise ValueError("Ceph CRUSH hosts not unique")
    # Do not qualify transient recovery after CRUSH relocation: every PG
    # must have converged, with no remapped or degraded status.
    pgmap = status.get("pgmap") or {}
    num_pgs = int(pgmap.get("num_pgs", 0))
    remapped = int(pgmap.get("num_remapped_pgs", -1))
    states = pgmap.get("pgs_by_state") or []
    if num_pgs < 1 or remapped != 0 or not states:
        raise ValueError("Ceph PGs are not ready or remain remapped")
    if sum(int(row.get("count", 0)) for row in states) != num_pgs or any(
        not str(row.get("state_name", "")).startswith("active+clean")
        or "remapped" in str(row.get("state_name", ""))
        or "degraded" in str(row.get("state_name", ""))
        for row in states
    ):
        raise ValueError("Ceph PGs are not all active+clean")
    return {"mons": len(mons), "quorum": len(quorum),
            "osds": len(osds), "hosts": active_hosts, "pgs": num_pgs}


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("Usage: ceph-assert-cluster.py status.json osd-tree.json")
    result = attest(json.loads(pathlib.Path(sys.argv[1]).read_text()),
                    json.loads(pathlib.Path(sys.argv[2]).read_text()))
    print("CEPH REAL CLUSTER: PASS " + json.dumps(result, sort_keys=True))
