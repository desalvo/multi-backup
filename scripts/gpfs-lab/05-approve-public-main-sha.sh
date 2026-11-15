#!/usr/bin/env bash
# Run manually AS ROOT after reviewing exact public main commit, never inside CI.
set -euo pipefail
umask 077
test "$(id -u)" -eq 0 || { echo "Root required" >&2; exit 1; }
test "${MB_GPFS_SHA_APPROVAL:-}" = I-REVIEWED-THIS-EXACT-PUBLIC-MAIN-COMMIT || {
  echo "Independent operator approval absent" >&2; exit 1;
}
SHA="${1:?Usage: 05-approve-public-main-sha.sh EXACT_MAIN_SHA}"
[[ "$SHA" =~ ^[0-9a-f]{40}$ ]] || { echo "Invalid 40-digit SHA" >&2; exit 1; }
CURRENT=$(git ls-remote https://github.com/desalvo/multi-backup.git refs/heads/main | awk '{print $1}')
test "$SHA" = "$CURRENT" || { echo "SHA is not current public main HEAD" >&2; exit 1; }
DIR=/etc/multi-backup-ci
FILE="$DIR/approved-public-main-sha"
test ! -L "$DIR" && test ! -L "$FILE"
install -d -m 0755 -o root -g root "$DIR"
TEMP=$(mktemp "$DIR/.approved-main.XXXXXXXX")
trap 'rm -f -- "$TEMP"' EXIT
printf "%s\n" "$SHA" > "$TEMP"
chown root:root "$TEMP"
chmod 0644 "$TEMP"
mv -f -- "$TEMP" "$FILE"
echo "Approved public main SHA $SHA for private GPFS/CephFS runner. Remove marker to revoke."
