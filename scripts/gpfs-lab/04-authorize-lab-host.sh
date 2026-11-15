#!/usr/bin/env bash
# Explicit administrative opt-in, no IBM software or credentials copied.
set -euo pipefail
test "$(id -u)" -eq 0
test "$MB_GPFS_LAB_APPROVAL" = APPROVED-DISPOSABLE-R042-ONLY
test "$(uname -s)" = Linux
ATTEST=/etc/multi-backup-ci/gpfs-ces-export-evidence.txt
test -f "$ATTEST" && test ! -L "$ATTEST"
test "$(stat -c %u "$ATTEST")" = 0
grep -Fx 'IBM_STORAGE_SCALE_CES_LAB' "$ATTEST" >/dev/null
DEST=/etc/multi-backup-ci/allow-disposable-filesystem-field
test ! -e "$DEST" && test ! -L "$DEST"
printf 'APPROVED-DISPOSABLE-R042-ONLY\n' > "$DEST"
chown root:root "$DEST"
chmod 0644 "$DEST"
echo 'PASS opt-in guard installed for dedicated self-hosted field runner. Revoke by removing marker.'
