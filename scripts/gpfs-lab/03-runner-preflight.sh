#!/usr/bin/env bash
# Read-only proof that the runner sees an independent, real CES mount and test data.
set -euo pipefail
RO="$1"; RW="$2"; ACCESS="$3"
case "$ACCESS" in ces-nfs|ces-smb) ;; *) echo 'FAIL ces-nfs or ces-smb required' >&2; exit 2;; esac
test -n "$RO" && test -n "$RW" && test "$RO" != "$RW" && test "$RO" != /
for path in "$RO" "$RW"; do
  test -d "$path" && test ! -L "$path"
  test "$(realpath "$path")" = "$path"
  test "$(findmnt -rn -T "$path" -o TARGET)" = "$path"
done
FSTYPE=$(findmnt -rn -T "$RO" -o FSTYPE)
case "$ACCESS:$FSTYPE" in ces-nfs:nfs|ces-nfs:nfs4|ces-smb:cifs|ces-smb:smb3) ;; *)
  echo "FAIL actual mount type incompatible: $FSTYPE" >&2; exit 1;; esac
test "$(findmnt -rn -T "$RO" -o OPTIONS | tr ',' '\n' | grep -xc ro)" -eq 1
test "$(cat "$RO/MB_EXPLICIT_TEST_FIXTURE_R042")" = MB_EXPLICIT_TEST_FIXTURE_R042
test "$(cat "$RW/MB_EXPLICIT_RECOVERY_TARGET_R042")" = MB_EXPLICIT_RECOVERY_TARGET_R042
test -f "$RO/test.bin" && test -f "$RO/test.sha256"
EXPECTED=$(cut -d' ' -f1 "$RO/test.sha256")
test "$(printf '%s' "$EXPECTED" | wc -c)" -eq 64
test "$EXPECTED" = "$(sha256sum "$RO/test.bin" | awk '{print $1}')"
ATTEST=/etc/multi-backup-ci/gpfs-ces-export-evidence.txt
test -f "$ATTEST" && test ! -L "$ATTEST"
test "$(stat -c %u "$ATTEST")" = 0
grep -Fx 'IBM_STORAGE_SCALE_CES_LAB' "$ATTEST" >/dev/null
echo "PASS real $ACCESS kernel mount RO, file hash and root-owned IBM CES attestation."
echo "This test DOES NOT itself prove that a remote NFS/SMB server runs GPFS: independently inspect actual IBM server cluster, exports, snapshot and license."
