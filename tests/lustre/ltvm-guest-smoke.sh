#!/usr/bin/env bash
# This executes INSIDE a disposable QEMU guest with genuine Lustre kernel modules.
set -euo pipefail
umask 077
test -r /proc/fs/lustre/version || { echo "FAIL: Lustre kernel subsystem absent" >&2; exit 1; }
command -v lfs >/dev/null
CLIENT=$(findmnt -rn -t lustre -o SOURCE,TARGET | awk '$1 ~ /:\// { print $2; exit }')
test -n "$CLIENT" && test -d "$CLIENT" || { echo "FAIL: real Lustre client mount missing" >&2; exit 1; }
findmnt -rn -T "$CLIENT" -o FSTYPE | grep -qx lustre
lfs df "$CLIENT"
TESTDIR="$CLIENT/mb-real-ci-$(date +%s)"
mkdir "$TESTDIR"
trap 'rm -rf -- "$TESTDIR"' EXIT
lfs setstripe -c 2 -S 1M "$TESTDIR/striped.bin"
dd if=/dev/urandom of="$TESTDIR/striped.bin" bs=1M count=3 status=none
test "$(lfs getstripe -c "$TESTDIR/striped.bin")" -eq 2
sha256sum "$TESTDIR/striped.bin" > "$TESTDIR/checksums"
(cd "$TESTDIR" && sha256sum -c checksums)
mkdir -p "$TESTDIR/nested"
printf 'Lustre real kernel-backed file\n' > "$TESTDIR/nested/data"
cmp <(printf 'Lustre real kernel-backed file\n') "$TESTDIR/nested/data"
# Try genuine read-only remount in a SECOND client mount. This does not
# produce an MDT/OST coordinated snapshot; it is only a read-only-client smoke.
SOURCE=$(findmnt -rn -T "$CLIENT" -o SOURCE)
test -n "$SOURCE"
mkdir -p /mnt/mb-lustre-ci-ro
if ! mount -t lustre -o ro "$SOURCE" /mnt/mb-lustre-ci-ro; then
  echo "FAIL: genuine second read-only Lustre client mount unavailable" >&2
  exit 1
fi
trap 'umount /mnt/mb-lustre-ci-ro || true; rm -rf -- "$TESTDIR"' EXIT
findmnt -rn -T /mnt/mb-lustre-ci-ro -o FSTYPE,OPTIONS | grep -Eq '^lustre .*\bro\b'
test -f "/mnt/mb-lustre-ci-ro/$(basename "$TESTDIR")/striped.bin"
cmp "$TESTDIR/striped.bin" "/mnt/mb-lustre-ci-ro/$(basename "$TESTDIR")/striped.bin"
echo 'PASS genuine Lustre QEMU kernel/client, two OST striping, byte integrity and separate RO mount'
echo 'PENDING separate field grade: coordinated MDT+OST snapshots and Rust encrypted backup+metadata-preserving restore.'
