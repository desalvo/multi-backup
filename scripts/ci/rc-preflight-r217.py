#!/usr/bin/env python3
"""r217: 16 real scoped providers and native OS installer field acceptance, nonpublishing."""
from __future__ import annotations
import argparse
import json
import re
from pathlib import Path

SHA = re.compile(r"^[0-9a-f]{40}$")
TOTAL = 33
FIELDS = ("backup_input", "backup_output", "restore_input", "restore_output")
STAGES = frozenset(("unsupported", "planned", "preview", "experimental", "ready"))
# Exactly the source code's dedicated *implemented* paths; not broad provider readiness.
READY_ROLES = {
    "posix": (("ready", "ready", "ready", "ready"), "content_only_quiesced_regular_files"),
    "s3": (("ready", "preview", "preview", "planned"), "minio_version_pinned_single_object_sha256_age_import"),
    "cephfs": (("ready", "preview", "preview", "ready"), "cephfs_snapshot_attested_mounted_regular_file_content_create_only"),
    "rbd": (("ready", "unsupported", "unsupported", "ready"), "ceph_rbd_pinned_snapshot_full_image_new_image_readback"),
    "iscsi": (("ready", "planned", "planned", "ready"), "linux_tgt_loopback_readonly_pinned_snapshot_new_lun_readback"),
    "mysql": (("ready", "unsupported", "unsupported", "ready"), "mariadb11_mysql84_single_database_full_innodb_create_only"),
    "postgresql": (("ready", "unsupported", "unsupported", "ready"), "postgresql16_single_database_full_logical_create_only"),
    "oracle": (("ready", "unsupported", "unsupported", "ready"), "oracle_rman_media_pieces_new_directory_age_verified"),
    "anti-ransomware-vault": (("unsupported", "ready", "ready", "unsupported"), "minio_s3_object_lock_compliance_ciphertext_versioned_recovery"),
    "webdav": (("ready", "ready", "ready", "planned"), "webdav_authenticated_ro_davfs2_bytes_create_only_https_ciphertext_put_get"),
    "ssh": (("ready", "planned", "planned", "planned"), "openssh_known_hosts_one_pinned_remote_file_sha256_age_import"),
    "sshfs": (("ready", "ready", "ready", "ready"), "sshfs_fuse_attested_regular_file_bytes_new_rw_restore_ciphertext_bout"),
    "lustre": (("ready", "ready", "ready", "ready"), "lustre_qemu_mdt_ost_quiesced_regular_file_bytes_new_restore_ciphertext_archive"),
    "nfs": (("ready", "ready", "ready", "ready"), "nfsv3_kernel_export_attested_frozen_regular_file_full_inc_new_restore_ciphertext_bout"),
    "smb": (("ready", "ready", "ready", "ready"), "samba_smb3_authenticated_cifs_attested_regular_file_full_inc_new_restore_ciphertext_bout"),
    "glusterfs": (("ready", "ready", "ready", "ready"), "glusterfs_single_node_real_fuse_mount_attested_regular_file_full_inc_restore_ciphertext_bout"),
}
# These are genuine operational field tests, not unit tests or static declarations.
# The workflow requires all five to complete SUCCESS *on the current source SHA*.
MANDATORY_LABS = ("mysql84", "ceph_rbd_cephfs", "iscsi_tgt", "oracle_free", "minio_worm_iam", "minio_s3_object",
                  "openssh_sshfs", "webdav_https", "lustre_qemu", "nfs_smb_gluster")
MANDATORY_INSTALLERS = ("linux_deb", "macos_arm64_dmg", "macos_x86_64_dmg", "windows_x64_inno")

class PromotionError(ValueError):
    pass

def validate(catalog: list[dict], backup: dict, restore: dict,
             sha: str, run: str, gate_results: dict, installer_results: dict) -> dict:
    if not SHA.fullmatch(sha) or not re.fullmatch(r"[1-9][0-9]{4,}", run):
        raise PromotionError("Full 40-digit source SHA and real Actions run ID required")
    if len(catalog) != TOTAL or len({p.get("id") for p in catalog}) != TOTAL:
        raise PromotionError("Catalog count or unique provider inventory changed")
    present = {}
    for p in catalog:
        if not all(p.get(f) in STAGES for f in FIELDS):
            raise PromotionError("Missing or unknown provider role stage")
        role = tuple(p[f] for f in FIELDS)
        scope = p.get("ready_scope")
        ready = "ready" in role
        if bool(scope) != ready:
            raise PromotionError("READY without scope or Preview with a fabricated READY scope")
        if ready:
            exp = READY_ROLES.get(p["id"])
            if exp is None or role != exp[0] or exp[1] not in scope:
                raise PromotionError("Unqualified broad provider READY or incorrect role capability")
            if ";" not in scope:
                raise PromotionError("READY scope must list explicitly excluded operations")
            present[p["id"]] = scope
        elif scope is not None:
            raise PromotionError("False ready_scope on unqualified provider")
    if set(present) != set(READY_ROLES):
        raise PromotionError("A qualified provider missing or a new unqualified READY inserted")
    for direction, item in (("backup",backup),("restore",restore)):
        if not (item.get("mode")==direction and
                item.get("source_plugin")=="posix" and
                item.get("destination_plugin")=="posix" and
                item.get("source_stage")=="ready" and
                item.get("destination_stage")=="ready" and
                item.get("executable") is True and
                item.get("preview_executable") is False and
                item.get("confirmation_required_before_writes") is True and
                "content-only" in item.get("reason","")):
            raise PromotionError("Only POSIX/posix is a qualified executable generic pair")
    if set(gate_results) != {"source_sha", "actions_run_id", *MANDATORY_LABS}:
        raise PromotionError("Missing/additional actual provider field gate result")
    if gate_results["source_sha"] != sha or str(gate_results["actions_run_id"]) != run:
        raise PromotionError("Provider lab evidence does not belong to this exact commit/run")
    if any(gate_results[k] != "success" for k in MANDATORY_LABS):
        raise PromotionError("A genuine provider field run was failed/skipped/cancelled")
    if set(installer_results) != {"source_sha", "actions_run_id", *MANDATORY_INSTALLERS}:
        raise PromotionError("Missing or additional native installer proof")
    if installer_results["source_sha"] != sha or str(installer_results["actions_run_id"]) != run:
        raise PromotionError("Native installer proof must match the exact source commit and run")
    if any(installer_results[k] != "success" for k in MANDATORY_INSTALLERS):
        raise PromotionError("One or more real native installation/test suites failed, skipped, or cancelled")
    return {
        "schema":3, "source_sha":sha,"actions_run_id":int(run),
        "plugin_count":TOTAL,
        "scoped_ready_count":len(present),
        "scoped_ready_ids":list(present),
        "scopes":present,
        "current_sha_provider_field_gates":{g:gate_results[g] for g in MANDATORY_LABS},
        "current_sha_native_installer_gates":{g:installer_results[g] for g in MANDATORY_INSTALLERS},
        "all_providers_ready":False,
        "release_channel":"installable-rc-preview-ci-artifacts-only",
        "rc_publication_authorized":False,
        "macos_signed_notarized_certified":False,
        "windows_signed_installer_certified":False,
        "vmware_vadp_cbt_certified":False,
        "ovirt_full_vm_boot_certified":False,
        "pve_pbs_full_vm_ct_boot_certified":False,
        "three_physical_host_fencing_certified":False,
        "limitations": (
            "Exactly sixteen independently scoped implemented provider paths only. "
            "POSIX/CephFS metadata ACL/xattr not captured by generic data-content path; "
            "MySQL GTID/PITR and global users; Oracle DB automatic restore; "
            "arbitrary SAN/multipath; RBD VM guest consistency; non-MinIO S3 and "
            "provider/host root ransomware immunity are excluded. "
            "All other 17 catalog IDs remain Preview/Planned/Unsupported. "
            "Native installers are unsigned/adhoc RC-preview only, no Apple notarization or Windows Authenticode. "
            "No official stable or vendor-wide field certification."
        ),
    }

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for a in ("catalog", "backup-plan", "restore-plan", "field-gates", "installer-gates", "output"):
        parser.add_argument("--"+a,type=Path,required=True)
    parser.add_argument("--source-sha",required=True)
    parser.add_argument("--run-id",required=True)
    args=parser.parse_args()
    result=validate(
        json.loads(args.catalog.read_text()),
        json.loads(args.backup_plan.read_text()),
        json.loads(args.restore_plan.read_text()),
        args.source_sha,args.run_id,
        json.loads(args.field_gates.read_text()),
        json.loads(args.installer_gates.read_text()))
    args.output.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
    print(f"PASS source-matched RC-preview: {len(READY_ROLES)} bounded READY providers, all ten required real field groups (12+ real protocol sublabs) and four platform installers PASS")
    print("NO stable/public release authorized; unsigned/notarization and real hypervisor/physical HA gaps remain open")

if __name__=="__main__":
    main()
