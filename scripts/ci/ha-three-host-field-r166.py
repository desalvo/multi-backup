#!/usr/bin/env python3
"""Read-only three-host Patroni/etcd field preflight.

This is a real SSH-to-three-independent-kernels check, NOT a substitute
for live failover, power cuts, network partitions or physical host attestation.
It never stops a service, changes firewall rules or writes to a database.
"""
import argparse
import json
import re
import subprocess
import sys

HOST = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9.-]{0,252}$")
MARKER = "MB-PATRONI-LIST-BEGIN"
REMOTE = r"""set -eu
printf 'MACHINE_ID=%s\n' "$(cat /etc/machine-id)"
printf 'BOOT_ID=%s\n' "$(cat /proc/sys/kernel/random/boot_id)"
printf 'NODE_NAME=%s\n' "$(hostname)"
printf 'PRODUCT_UUID=%s\n' "$(cat /sys/class/dmi/id/product_uuid 2>/dev/null || echo unavailable)"
printf 'PATRONI_RUNNING=%s\n' "$(systemctl is-active patroni || true)"
printf 'ETCD_RUNNING=%s\n' "$(systemctl is-active etcd || true)"
printf 'MB-PATRONI-LIST-BEGIN\n'
patronictl -c /etc/patroni/patroni.yml list -f json
"""


def parse_host_result(raw):
    if MARKER not in raw:
        raise ValueError("Patroni JSON attestation marker missing")
    metadata, patroni = raw.split(MARKER, 1)
    values = {}
    for line in metadata.strip().splitlines():
        if "=" not in line:
            raise ValueError("Unexpected remote host metadata")
        key, value = line.split("=", 1)
        if key in values or key not in {
            "MACHINE_ID", "BOOT_ID", "NODE_NAME", "PRODUCT_UUID",
            "PATRONI_RUNNING", "ETCD_RUNNING"
        }:
            raise ValueError("Unexpected or repeated host metadata")
        values[key] = value.strip()
    required = ("MACHINE_ID", "BOOT_ID", "NODE_NAME",
                "PATRONI_RUNNING", "ETCD_RUNNING")
    if any(not values.get(field) for field in required):
        raise ValueError("Missing host fingerprint or service evidence")
    if values["PATRONI_RUNNING"] != "active" or values["ETCD_RUNNING"] != "active":
        raise ValueError("Patroni and etcd must both be running")
    members = json.loads(patroni.strip())
    if not isinstance(members, list) or len(members) != 3:
        raise ValueError("Patroni must report exactly three members")
    names = []
    leaders = []
    for member in members:
        if not isinstance(member, dict):
            raise ValueError("Invalid member record")
        name = member.get("Member", member.get("member"))
        role = member.get("Role", member.get("role"))
        state = member.get("State", member.get("state"))
        if not isinstance(name, str) or not name or name in names:
            raise ValueError("Invalid/duplicate member identity")
        if not isinstance(role, str) or not isinstance(state, str):
            raise ValueError("Role/state missing")
        if state.lower() not in ("running", "streaming"):
            raise ValueError("A member is not running/streaming")
        names.append(name)
        if role.lower() in ("leader", "primary"):
            leaders.append(name)
    if len(leaders) != 1:
        raise ValueError("Exactly one Patroni primary required")
    values["members"] = sorted(names)
    values["leader"] = leaders[0]
    return values


def verify_three_hosts(hosts, records):
    if len(hosts) != 3 or len(set(hosts)) != 3:
        raise ValueError("Exactly three distinct host addresses are required")
    if len(records) != 3:
        raise ValueError("Three independent host observations required")
    for identity in ("MACHINE_ID", "BOOT_ID", "NODE_NAME"):
        if len({x[identity] for x in records}) != 3:
            raise ValueError(f"Duplicate host fingerprint: {identity}")
    if len({tuple(x["members"]) for x in records}) != 1:
        raise ValueError("Inconsistent Patroni membership across the hosts")
    if len({x["leader"] for x in records}) != 1:
        raise ValueError("Patroni views disagree on primary")
    return {
        "attested_hosts": len(hosts),
        "distinct_kernel_boot_ids": 3,
        "single_primary": records[0]["leader"],
        "members": records[0]["members"],
        "services_running": True,
        "physical_fault_domains_certified": False,
        "live_failover_certified": False,
        "network_partition_certified": False,
    }


def ssh_read_only(host):
    if not HOST.fullmatch(host) or ".." in host:
        raise ValueError("Unsafe remote lab hostname")
    result = subprocess.run(
        ["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
         "-o", "ForwardAgent=no", "-o", "ConnectTimeout=8",
         host, "sh", "-s"],
        input=REMOTE, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, timeout=45, check=False,
    )
    if result.returncode:
        raise ValueError(f"Host {host} failed read-only attestation"
                         " (no remote error text printed)")
    return parse_host_result(result.stdout)


def selftest():
    members = [
        {"Member": "p1", "Role": "Leader", "State": "running"},
        {"Member": "p2", "Role": "Sync Standby", "State": "running"},
        {"Member": "p3", "Role": "Replica", "State": "streaming"}
    ]
    records = []
    for i in range(3):
        output = "\n".join((
            f"MACHINE_ID=machine{i}",
            f"BOOT_ID=boot{i}",
            f"NODE_NAME=host{i}",
            f"PRODUCT_UUID=product{i}",
            "PATRONI_RUNNING=active",
            "ETCD_RUNNING=active",
            MARKER,
            json.dumps(members)
        ))
        records.append(parse_host_result(output))
    hosts = ["lab1.example.test", "lab2.example.test", "lab3.example.test"]
    assert verify_three_hosts(hosts, records)["single_primary"] == "p1"
    try:
        verify_three_hosts(hosts[:2] + hosts[:1], records)
    except ValueError:
        pass
    else:
        raise AssertionError("Duplicate hosts must fail")
    failed = [dict(x) for x in records]
    failed[1]["BOOT_ID"] = failed[0]["BOOT_ID"]
    try:
        verify_three_hosts(hosts, failed)
    except ValueError:
        pass
    else:
        raise AssertionError("Identical kernel boot IDs must fail")
    split = [dict(x) for x in records]
    split[2]["leader"] = "p2"
    try:
        verify_three_hosts(hosts, split)
    except ValueError:
        pass
    else:
        raise AssertionError("Conflicting primary must fail")
    for value in ("bad host", "-invalid", "evil..invalid"):
        try:
            ssh_read_only(value)
        except ValueError:
            pass
        else:
            raise AssertionError("Unsafe SSH hostname accepted")
    print("PASS: three-host strict identity, quorum views and negative fixtures")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--hosts", nargs=3, metavar=("NODE1", "NODE2", "NODE3"))
    args = parser.parse_args()
    if args.self_test:
        selftest()
        return
    if not args.hosts:
        parser.error("Three independently operated lab hostnames required")
    try:
        if len(set(args.hosts)) != 3 or any(not HOST.fullmatch(h) or ".." in h for h in args.hosts):
            raise ValueError("Unsafe or duplicate lab host address")
        records = [ssh_read_only(host) for host in args.hosts]
        summary = verify_three_hosts(args.hosts, records)
    except (ValueError, subprocess.TimeoutExpired) as error:
        print(f"FAIL: field preflight: {error}", file=sys.stderr)
        sys.exit(1)
    print(json.dumps(summary, indent=2, sort_keys=True))
    print("FIELD PREFLIGHT ONLY: real 3-host crash, write fencing, quorum-loss"
          " and rejoin tests still require an isolated operator-controlled lab")


if __name__ == "__main__":
    main()
