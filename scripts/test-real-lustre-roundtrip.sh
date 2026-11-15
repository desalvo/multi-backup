#!/usr/bin/env bash
# Manual field acceptance: genuine pre-mounted Lustre RO snapshots -> encrypted Rust store -> NEW Lustre RW targets.
# This DOES NOT provision a Lustre cluster or certify snapshot consistency by itself.
set -euo pipefail
umask 077
: "$LUSTRE_FULL_MOUNT" "$LUSTRE_INCREMENTAL_MOUNT" "$LUSTRE_RESTORE_MOUNT" "$LUSTRE_SNAPSHOT_ATTESTED"
[[ "$LUSTRE_SNAPSHOT_ATTESTED" == "COORDINATED_MDT_OST_LAB_SNAPSHOTS" ]] || { echo "FAIL: explicit MDT+OST snapshot attestation missing" >&2; exit 1; }
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
for tool in age-keygen jq cmp sha256sum getfattr getfacl stat lfs; do command -v "$tool" >/dev/null || { echo "FAIL: $tool is missing" >&2; exit 1; }; done
for src in "$LUSTRE_FULL_MOUNT" "$LUSTRE_INCREMENTAL_MOUNT"; do
  test -d "$src"
  test -f "$src/.multi-backup-ci-fixture"
  [[ "$(cat "$src/.multi-backup-ci-fixture")" == "DEDICATED_LUSTRE_TEST_DATA_V1" ]] || { echo "FAIL: non-lab Lustre source refused" >&2; exit 1; }
  "$CLI" plugins probe --plugin lustre --mount "$src" --json | jq -e '.fs_type == "lustre" and .rw_mount_option == false' >/dev/null
done
test -d "$LUSTRE_RESTORE_MOUNT"
"$CLI" plugins probe --plugin lustre --mount "$LUSTRE_RESTORE_MOUNT" --json | jq -e '.fs_type == "lustre" and .rw_mount_option == true' >/dev/null
test -f "$LUSTRE_FULL_MOUNT/README.txt"
test -f "$LUSTRE_FULL_MOUNT/payload/removed.bin"
test -f "$LUSTRE_FULL_MOUNT/payload/stripe.bin"
test -f "$LUSTRE_INCREMENTAL_MOUNT/README.txt"
test -f "$LUSTRE_INCREMENTAL_MOUNT/payload/new.bin"
test -f "$LUSTRE_INCREMENTAL_MOUNT/payload/stripe.bin"
test ! -e "$LUSTRE_INCREMENTAL_MOUNT/payload/removed.bin"
if cmp -s "$LUSTRE_FULL_MOUNT/README.txt" "$LUSTRE_INCREMENTAL_MOUNT/README.txt"; then echo "FAIL: incremental fixture is unchanged" >&2; exit 1; fi
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
test -n "$RECIPIENT"
"$CLI" storage init --root "$WORK/encrypted-store" --recipient "$RECIPIENT"
"$CLI" plugins backup --input lustre --output posix --source "$LUSTRE_FULL_MOUNT" --root "$WORK/encrypted-store" --identity "$WORK/identity" --label lustre-field-full --namespace lustre --top-namespace field --source-snapshot-attested --json > "$WORK/full.json"
FULL=$(jq -er .id "$WORK/full.json")
"$CLI" storage verify --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$FULL"
"$CLI" plugins backup --input lustre --output posix --source "$LUSTRE_INCREMENTAL_MOUNT" --root "$WORK/encrypted-store" --identity "$WORK/identity" --label lustre-field-incremental --namespace lustre --top-namespace field --mode incremental --parent "$FULL" --source-snapshot-attested --json > "$WORK/incremental.json"
INCREMENTAL=$(jq -er .id "$WORK/incremental.json")
jq -e --arg parent "$FULL" '.kind == "incremental" and .parent_id == $parent' "$WORK/incremental.json" >/dev/null
"$CLI" storage verify-chain --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$INCREMENTAL" --json >/dev/null
TAG="mb-lustre-ci-$(date +%s)-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
FULL_DEST="$LUSTRE_RESTORE_MOUNT/$TAG-full"
INCREMENTAL_DEST="$LUSTRE_RESTORE_MOUNT/$TAG-incremental"
test ! -e "$FULL_DEST"
test ! -e "$INCREMENTAL_DEST"
"$CLI" plugins restore --input posix --output lustre --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$FULL" --destination "$FULL_DEST" --json >/dev/null
"$CLI" plugins restore --input posix --output lustre --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$INCREMENTAL" --destination "$INCREMENTAL_DEST" --json >/dev/null
for file in README.txt payload/removed.bin payload/stripe.bin; do
  cmp "$LUSTRE_FULL_MOUNT/$file" "$FULL_DEST/field/lustre/$file"
done
for file in README.txt payload/new.bin payload/stripe.bin; do
  cmp "$LUSTRE_INCREMENTAL_MOUNT/$file" "$INCREMENTAL_DEST/field/lustre/$file"
done
test ! -e "$INCREMENTAL_DEST/field/lustre/payload/removed.bin"
if "$CLI" plugins restore --input posix --output lustre --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$FULL" --destination "$FULL_DEST" >/dev/null 2>&1; then
  echo "FAIL: existing Lustre destination overwritten" >&2; exit 1
fi
# Recovery fidelity is independently mandatory; current generic Rust file-byte
# preview is not assumed to preserve these attributes.
for pair in "$LUSTRE_FULL_MOUNT|$FULL_DEST" "$LUSTRE_INCREMENTAL_MOUNT|$INCREMENTAL_DEST"; do
  SRC=${pair%%|*}; DST=${pair#*|}
  A="$SRC/payload/stripe.bin"; B="$DST/field/lustre/payload/stripe.bin"
  [[ "$(stat -c '%a:%u:%g:%Y' "$A")" == "$(stat -c '%a:%u:%g:%Y' "$B")" ]] || { echo "FAIL: Lustre mode/owner/mtime not preserved" >&2; exit 1; }
  SRC_ACL=$(getfacl -cp "$A") || { echo "FAIL: cannot read source Lustre ACL" >&2; exit 1; }
  DST_ACL=$(getfacl -cp "$B") || { echo "FAIL: cannot read recovered Lustre ACL" >&2; exit 1; }
  [[ "$SRC_ACL" == "$DST_ACL" ]] || { echo "FAIL: Lustre POSIX ACL not preserved" >&2; exit 1; }
  SRC_XATTR=$(getfattr --only-values -n user.multi_backup_ci "$A") || { echo "FAIL: source Lustre xattr fixture missing" >&2; exit 1; }
  DST_XATTR=$(getfattr --only-values -n user.multi_backup_ci "$B") || { echo "FAIL: recovered Lustre xattr missing" >&2; exit 1; }
  [[ "$SRC_XATTR" == "$DST_XATTR" ]] || { echo "FAIL: Lustre xattr not preserved" >&2; exit 1; }
  SRC_STRIPE=$(lfs getstripe -c "$A") || { echo "FAIL: cannot inspect source Lustre stripe" >&2; exit 1; }
  DST_STRIPE=$(lfs getstripe -c "$B") || { echo "FAIL: cannot inspect restored Lustre stripe" >&2; exit 1; }
  [[ "$SRC_STRIPE" == "$DST_STRIPE" ]] || { echo "FAIL: Lustre stripe-count not preserved" >&2; exit 1; }
done
echo "PASS genuine Lustre field acceptance: full/incremental encrypted store, new-target restore, bytes, ACL/xattr/mode/mtime/owner and striping"
echo "Lustre restore evidence retained at: $FULL_DEST and $INCREMENTAL_DEST"
echo "Full ID: $FULL"
echo "Incremental ID: $INCREMENTAL"
