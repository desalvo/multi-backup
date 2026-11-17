#!/usr/bin/env python3
"""Fail-closed RC preflight: attest EXACT SHA and narrow Ready scope, not a release."""
from __future__ import annotations
import argparse
import json
import re
from pathlib import Path

SHA = re.compile(r"^[0-9a-f]{40}$")
READY_SCOPES = {
    "posix": ("content_only_quiesced_regular_files",
              ("ready","ready","ready","ready")),
    "postgresql": ("postgresql16_single_database_full_logical_create_only",
                   ("ready","unsupported","unsupported","ready")),
}
FIELDS = ("backup_input", "backup_output", "restore_input", "restore_output")
TOTAL = 33

class PromotionError(ValueError):
    pass

def validate(catalog: list[dict], backup: dict, restore: dict, sha: str, run: str) -> dict:
    if not SHA.fullmatch(sha):
        raise PromotionError("Source must be an actual full Git commit SHA")
    if not re.fullmatch(r"[1-9][0-9]{4,}", run):
        raise PromotionError("RC evidence needs a real GitHub Actions run ID")
    if len(catalog) != TOTAL or len({p.get("id") for p in catalog}) != TOTAL:
        raise PromotionError("Plugin inventory changed unexpectedly")
    valid_stage_values = {"unsupported", "planned", "preview", "experimental", "ready"}
    for p in catalog:
        if any(p.get(f) not in valid_stage_values for f in FIELDS):
            raise PromotionError("Unknown or missing capability state")
        states = tuple(p[f] for f in FIELDS)
        scoped = "ready" in states
        if scoped != bool(p.get("ready_scope")):
            raise PromotionError("Missing or fabricated Ready capability scope")
        if scoped:
            expected = READY_SCOPES.get(p.get("id"))
            if expected is None or expected[0] not in p["ready_scope"]:
                raise PromotionError("Uncertified provider cannot claim a Ready scope")
            if states != expected[1]:
                raise PromotionError("READY roles exceed explicitly tested provider capability")
        elif p.get("ready_scope") is not None:
            raise PromotionError("Preview provider has a Ready claim")
    promoted = [p["id"] for p in catalog if p["backup_input"] == "ready"]
    if promoted != ["posix","postgresql"]:
        raise PromotionError("Exactly the qualified POSIX and PostgreSQL scopes are permitted")
    for direction, evidence in (("backup", backup), ("restore", restore)):
        if (evidence.get("mode") != direction or
            evidence.get("source_plugin") != "posix" or
            evidence.get("destination_plugin") != "posix" or
            evidence.get("source_stage") != "ready" or
            evidence.get("destination_stage") != "ready" or
            evidence.get("executable") is not True or
            evidence.get("preview_executable") is not False or
            evidence.get("confirmation_required_before_writes") is not True or
            "content-only" not in evidence.get("reason", "")):
            raise PromotionError("Compiled POSIX operation is not the narrow Ready contract")
    return {
        "schema": 1, "source_sha": sha, "actions_run_id": int(run),
        "plugin_count": TOTAL, "ready_scoped_plugins": promoted,
        "ready_scopes": {p["id"]:p["ready_scope"] for p in catalog if p["backup_input"]=="ready"},
        "all_plugins_ready": False,
        "release_channel": "rc-preflight-not-published",
        "rc_publication_authorized": False,
        "native_mac_windows_installer_certified": False,
        "licensed_vmware_vadp_cbt_certified": False,
        "real_ovirt_vm_boot_certified": False,
        "baremetal_three_host_ha_certified": False,
        "notes": (
            "POSIX Ready means only quiescent regular-file byte content: "
            "no ownership, mode, mtime, ACL, xattr, hardlinks, symlinks, empty dirs "
            "or application-consistent live snapshots. PostgreSQL 16 Ready " 
            "only covers a single full logical database dump and create-only restore, "
            "never roles/ACL/globals/PITR under Patroni. Remaining 31 providers "
            "cannot be promoted without specific field acceptance. "
            "No release is created by preflight."
        )
    }

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--catalog", type=Path, required=True)
    p.add_argument("--backup-plan", type=Path, required=True)
    p.add_argument("--restore-plan", type=Path, required=True)
    p.add_argument("--source-sha", required=True)
    p.add_argument("--run-id", required=True)
    p.add_argument("--output", type=Path, required=True)
    args=p.parse_args()
    report=validate(
        json.loads(args.catalog.read_text(encoding="utf-8")),
        json.loads(args.backup_plan.read_text(encoding="utf-8")),
        json.loads(args.restore_plan.read_text(encoding="utf-8")),
        args.source_sha,args.run_id)
    args.output.write_text(json.dumps(report,sort_keys=True,indent=2)+"\n",encoding="utf-8")
    print("PASS single-SHA RC preflight; EXACTLY TWO scoped Ready plugins (POSIX file bytes, PostgreSQL 16 full logical DB)")
    print("NOT A RELEASE: real-provider field gates and cross-platform packages remain OPEN")

if __name__=="__main__":
    main()
