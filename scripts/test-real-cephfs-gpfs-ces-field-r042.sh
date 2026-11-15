#!/usr/bin/env bash
# Strictly guarded independent field lab: CephFS or IBM GPFS via REAL CES.
# Never masquerade a generic NFS/CIFS share as a licensed GPFS server.
set -euo pipefail
umask 077
MODE="$1"
case "$MODE" in cephfs|gpfs-ces-nfs|gpfs-ces-smb) ;; *) echo 'Usage: cephfs|gpfs-ces-nfs|gpfs-ces-smb' >&2; exit 2;; esac
test "$RUNNER_ENVIRONMENT" = self-hosted
# A self-hosted runner must belong to the separate PRIVATE CI coordinator.
test -n "${MB_PRIVATE_LAB_REPOSITORY:-}"
test "${GITHUB_REPOSITORY:-}" = "$MB_PRIVATE_LAB_REPOSITORY"
test "$MB_PRIVATE_LAB_REPOSITORY" != desalvo/multi-backup
test "$MB_FIELD_APPROVAL" = APPROVED-DISPOSABLE-FILESYSTEM-LAB
MARKER=/etc/multi-backup-ci/allow-disposable-filesystem-field
test -f "$MARKER" && test ! -L "$MARKER"
test "$(stat -c %u "$MARKER")" = 0
test "$(< "$MARKER")" = APPROVED-DISPOSABLE-R042-ONLY
RO="$MB_FIELD_RO_MOUNT"
RW="$MB_FIELD_RW_MOUNT"
test "$RO" != "$RW" && test "$RO" != / && test "$RW" != /
for path in "$RO" "$RW"; do
  test -d "$path" && test ! -L "$path"
  test "$(realpath "$path")" = "$path"
  test "$(findmnt -rn -T "$path" -o TARGET)" = "$path"
done
test "$(< "$RO/MB_EXPLICIT_TEST_FIXTURE_R042")" = MB_EXPLICIT_TEST_FIXTURE_R042
test "$(< "$RW/MB_EXPLICIT_RECOVERY_TARGET_R042")" = MB_EXPLICIT_RECOVERY_TARGET_R042
test -f "$RO/test.bin" && test -f "$RO/test.sha256"
EXPECTED=$(cut -d' ' -f1 "$RO/test.sha256")
test "$(printf %s "$EXPECTED" | wc -c)" -eq 64
test "$EXPECTED" = "$(sha256sum "$RO/test.bin" | awk '{print $1}')"
test -n "$MB_FIELD_SNAPSHOT_ID" && test "$MB_FIELD_SNAPSHOT_ID" != LIVE_RW_VOLUME
test "$MB_FIELD_SNAPSHOT_ATTESTATION" = COHERENT_READONLY_SNAPSHOT_R042
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
ACCESS=""
case "$MODE" in
cephfs)
  PLUGIN=cephfs
  FS=$(findmnt -rn -T "$RO" -o FSTYPE)
  case "$FS" in ceph|fuse.ceph-fuse) ;; *) echo "FAIL: not a real CephFS client" >&2; exit 1;; esac
  "$CLI" plugins probe --plugin cephfs --mount "$RO" --json >/dev/null
  ;;
gpfs-ces-nfs|gpfs-ces-smb)
  PLUGIN=gpfs
  test "$MB_GPFS_CES_BACKEND_ATTESTED" = IBM_STORAGE_SCALE_CES_LAB
  ATTEST=/etc/multi-backup-ci/gpfs-ces-export-evidence.txt
  test -f "$ATTEST" && test ! -L "$ATTEST"
  test "$(stat -c %u "$ATTEST")" = 0
  if [ "$MODE" = gpfs-ces-nfs ]; then ACCESS=ces-nfs; else ACCESS=ces-smb; fi
  "$CLI" plugins probe --plugin gpfs --mount "$RO" --access "$ACCESS" --json >/dev/null
  ;;
esac
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT" >/dev/null
set --
if [ "$PLUGIN" = gpfs ]; then set -- --input-gpfs-access "$ACCESS"; fi
"$CLI" plugins backup --input "$PLUGIN" --output posix --source "$RO" \
  --root "$WORK/store" --identity "$WORK/identity" --label "$MODE field" \
  --namespace data --source-snapshot-attested "$@" --json > "$WORK/backup.json"
ID=$(jq -er '.id | select(length==64)' "$WORK/backup.json")
"$CLI" storage verify-chain --root "$WORK/store" --identity "$WORK/identity" --id "$ID" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
  --identity "$WORK/identity" --id "$ID" --destination "$WORK/recovered" >/dev/null
cmp "$RO/test.bin" "$WORK/recovered/data/test.bin"
if [ "$MODE" = cephfs ]; then
  "$CLI" plugins probe --plugin cephfs --mount "$RW" --json > "$WORK/rw.json"
  test "$(jq -r .rw_mount_option "$WORK/rw.json")" = true
  NAME="r042-recovery-$(date +%s)-$$"
  "$CLI" plugins restore --input posix --output cephfs --root "$WORK/store" \
    --identity "$WORK/identity" --id "$ID" --destination "$RW/$NAME" >/dev/null
  cmp "$RO/test.bin" "$RW/$NAME/data/test.bin"
  if "$CLI" plugins restore --input posix --output cephfs --root "$WORK/store" \
    --identity "$WORK/identity" --id "$ID" --destination "$RW/$NAME" >/dev/null 2>&1; then
    echo "FAIL: a live CephFS restore overwrote existing data" >&2; exit 1
  fi
  echo "PASS real CephFS fixture to Rust encrypted store and create-only CephFS recovery"
else
  echo "PASS operator-attested GPFS CES snapshot to Rust encrypted storage and local recovery"
  echo "NOT CERTIFIED: native GPFS driver, GPFS direct restore or ILM/ACL preservation"
fi
echo "NOT CERTIFIED: production snapshots, metadata fidelity, crash atomicity and physical write fencing"
