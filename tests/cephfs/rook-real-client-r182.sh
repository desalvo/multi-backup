#!/usr/bin/env bash
# Only in throwaway Rook-created Ubuntu client pod, real CephFS mount required.
set -Eeuo pipefail
umask 077
for bin in ceph-fuse ceph age-keygen sha256sum findmnt mountpoint jq;do command -v "$bin";done
test -c /dev/fuse || { echo 'FAIL no /dev/fuse in genuine client' >&2;exit 1; }
ceph fs status mb-ci-fs
RW=/mnt/mb-cephfs-rw
RO=/mnt/mb-cephfs-ro
LOCAL=/tmp/mb-cephfs-local
mkdir -p "$RW" "$RO" "$LOCAL"
cleanup(){
  set +e
  fusermount -u "$RO" 2>/dev/null || umount "$RO" 2>/dev/null || :
  fusermount -u "$RW" 2>/dev/null || umount "$RW" 2>/dev/null || :
}
trap cleanup EXIT
ceph-fuse --client_fs mb-ci-fs "$RW"
findmnt -rn -T "$RW" -o FSTYPE | grep -Eq '^(fuse.ceph-fuse|ceph)$'
mkdir -p "$RW/source/nested"
printf 'real cephfs source before update\n' >"$RW/source/nested/original.txt"
dd if=/dev/urandom of="$RW/source/payload.bin" bs=1M count=2 status=none
HASH="$(sha256sum "$RW/source/payload.bin" | awk '{print $1}')"
# This is a quiesced fixture, NOT an attested CephFS native snapshot.
ceph-fuse --client_fs mb-ci-fs -o ro "$RO"
findmnt -rn -T "$RO" -o OPTIONS | grep -Eq '(^|,)ro(,|$)'
test "$(cat "$RO/source/nested/original.txt")" = 'real cephfs source before update'
age-keygen -o "$LOCAL/identity" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$LOCAL/identity")"
CLI=/usr/local/bin/multi-backup-native
chmod 0755 "$CLI"
"$CLI" storage init --root "$LOCAL/store" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins probe --plugin cephfs --mount "$RO" --json |
  jq -e '.rw_mount_option == false and (.fs_type=="fuse.ceph-fuse" or .fs_type=="ceph")'
if "$CLI" plugins backup --input cephfs --output posix --source "$RO" \
  --root "$LOCAL/store" --identity "$LOCAL/identity" \
  --label no-attestation >/dev/null 2>&1;then
  echo 'FAIL: CephFS accepted source without consistency attestation' >&2;exit 1
fi
"$CLI" plugins backup --input cephfs --output posix --source "$RO" \
  --root "$LOCAL/store" --identity "$LOCAL/identity" \
  --source-snapshot-attested --namespace cephfs --label real-cephfs-full \
  --json >"$LOCAL/full.json"
FULL="$(jq -er .id "$LOCAL/full.json")"
"$CLI" storage verify-chain --root "$LOCAL/store" --identity "$LOCAL/identity" --id "$FULL"
"$CLI" plugins restore --input posix --output cephfs \
  --root "$LOCAL/store" --identity "$LOCAL/identity" --id "$FULL" \
  --destination "$RW/recovered-full"
cmp "$RW/source/payload.bin" "$RW/recovered-full/cephfs/source/payload.bin"
test "$HASH" = "$(sha256sum "$RW/recovered-full/cephfs/source/payload.bin" | awk '{print $1}')"
if "$CLI" plugins restore --input posix --output cephfs \
  --root "$LOCAL/store" --identity "$LOCAL/identity" --id "$FULL" \
  --destination "$RW/recovered-full" >/dev/null 2>&1;then
  echo 'FAIL: CephFS restore overwrote existing target' >&2;exit 1
fi
mkdir -p "$LOCAL/local-input/nested"
printf 'real CephFS B-Out encrypted archive\n' >"$LOCAL/local-input/nested/value.txt"
"$CLI" plugins mounted-backup --output cephfs --mount "$RW" \
  --source "$LOCAL/local-input" --filename cephfs-output.tar.gz.age \
  --recipient "$RECIPIENT" --identity "$LOCAL/identity" --json >"$LOCAL/out.json"
jq -e '.create_only and .encrypted and (.verified_members==.source_members)' "$LOCAL/out.json"
test -f "$RW/cephfs-output.tar.gz.age.sha256"
"$CLI" plugins mounted-restore --input cephfs --mount "$RW" \
  --filename cephfs-output.tar.gz.age --identity "$LOCAL/identity" \
  --destination "$LOCAL/recovered-output"
cmp "$LOCAL/local-input/nested/value.txt" \
  "$LOCAL/recovered-output/multi-backup/mounted-output/nested/value.txt"
if "$CLI" plugins mounted-backup --output cephfs --mount "$RW" \
  --source "$LOCAL/local-input" --filename cephfs-output.tar.gz.age \
  --recipient "$RECIPIENT" --identity "$LOCAL/identity" >/dev/null 2>&1;then
  echo 'FAIL CephFS B-Out overwrote an existing encrypted archive' >&2;exit 1
fi
echo 'PASS real CephFS MDS/OSD FUSE mounts, Rust age backup, create-only remote restore and encrypted archive B-Out'
