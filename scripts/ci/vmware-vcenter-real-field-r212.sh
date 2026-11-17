#!/usr/bin/env bash
# DESCTRUCTIVE CREATE-ONLY OFFLINE VMware VM import. Isolated physical field
# lab only, with explicit approval and approved, named throwaway VM.
set -Eeuo pipefail
umask 077
if test "$RUNNER_ENVIRONMENT" != self-hosted ||
   test "$MB_VMWARE_FIELD" != YES_ISOLATED_VMWARE_NEW_VM ||
   test "$(id -u)" -eq 0; then
 echo 'DENIED: real VMware field lab requires non-root, protected environment and confirmation' >&2
 exit 3
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NATIVE="$ROOT/rust/target/release/multi-backup-native"
test -x "$NATIVE"
LAB=/etc/multi-backup-ci/vmware-field
SENTINEL="$LAB/allow-disposable-vmware"
test -f "$SENTINEL"
test "$(cat "$SENTINEL")" = DEDICATED_VMWARE_TEST_VM_ONLY
EXPORT="$LAB/export.json"
IMPORT="$LAB/import.json"
test -f "$EXPORT" && test -f "$IMPORT"
test "$(stat -c %a "$EXPORT")" = 600 && test "$(stat -c %a "$IMPORT")" = 600
test "$(stat -c %a "$SENTINEL")" = 600
AGE_ID="$LAB/age-private.key"
test -f "$AGE_ID" && test "$(stat -c %a "$AGE_ID")" = 600
STAGE="$LAB/encrypted-staging"
STORE="$LAB/store"
test -d "$STAGE" && test -d "$STORE"
test "$(stat -c %a "$STAGE")" = 700
export EXPORT IMPORT
python3 - <<'PY'
import json,os
source=json.load(open(os.environ["EXPORT"]))
target=json.load(open(os.environ["IMPORT"]))
assert source["role"]=="export" and target["role"]=="import"
assert source["username"] != target["username"],"distinct export/import principals mandatory"
assert source["endpoint"]==target["endpoint"],"source/restore vCenter mismatch"
assert source["vm_path"].split("/")[-1].startswith("mb-lab-")
assert target["destination_folder"].startswith("/"+source["datacenter"]+"/vm/")
assert target["datastore"].startswith("mb-lab-"),"restore storage must be isolated"
PY
NAME="mb-lab-restore-$GITHUB_RUN_ID"
if ! [[ "$NAME" =~ ^mb-lab-restore-[0-9]{4,}$ ]];then
 echo 'DENIED: invalid dedicated new VM identity' >&2;exit 3
fi
DS="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["datastore"])' "$IMPORT")"
"$NATIVE" plugins vmware-ovf-backup --profile-json "$EXPORT" \
 --root "$STORE" --identity "$AGE_ID" \
 --stage-root "$STAGE" --label "Physical VMware offline isolated lab" \
 --json > "$LAB/field-backup-$GITHUB_RUN_ID.json"
ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$LAB/field-backup-$GITHUB_RUN_ID.json")"
"$NATIVE" plugins vmware-ovf-restore-new --profile-json "$IMPORT" \
 --root "$STORE" --identity "$AGE_ID" --id "$ID" \
 --stage-root "$STAGE" --new-vm "$NAME" \
 --confirm-new-vm "vmware:$NAME:$DS" --json > "$LAB/field-restore-$GITHUB_RUN_ID.json"
python3 - "$LAB/field-restore-$GITHUB_RUN_ID.json" <<'PY'
import json,sys
result=json.load(open(sys.argv[1]))
assert result["imported_new_vm"] is True
assert result["offline_only"] is True
assert result["boot_verified"] is False
assert result["vcenter_field_certified"] is False
PY
echo "PASS: physical VMware API offline import into NEW stopped lab VM $NAME. NO deletion, reboot or guest networking performed."
echo "LIMIT: guest boot/data, VADP CBT/live backups, cross-host disaster recovery not certified by this script"
