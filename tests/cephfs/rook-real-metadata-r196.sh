#!/usr/bin/env bash
# Real CephFS immutable .snap subtree while concurrent writes continue
# against the live working tree. bsdtar PAX captures ACL + xattrs.
set -Eeuo pipefail
umask 077
for b in ceph-fuse ceph bsdtar setfacl getfacl setfattr getfattr sha256sum jq; do
  command -v "$b" >/dev/null || { echo "FAIL: CephFS metadata lab lacks genuine $b" >&2; exit 1; }
done
CLI=/usr/local/bin/multi-backup-native
RW=/mnt/mb-cephfs-rw-r196
SNAP=/mnt/mb-cephfs-snapshot-r196
D=$(mktemp -d /tmp/mb-fsmeta-r196.XXXXXXXX)
WRITER=""
cleanup(){
  set +e
  if [ -n "$WRITER" ];then kill "$WRITER" >/dev/null 2>&1 || :;wait "$WRITER" 2>/dev/null || :;fi
  mountpoint -q "$SNAP" && (fusermount -u "$SNAP" || umount "$SNAP")
  mountpoint -q "$RW" && (fusermount -u "$RW" || umount "$RW")
  rm -rf -- "$D"
}
trap cleanup EXIT
# The preceding functional CephFS test correctly unmounted its own RW
# client at exit. This advanced test must create a SEPARATE genuine mount,
# never silently treat an ordinary local directory as CephFS.
mkdir -p "$RW" "$SNAP"
if ! ceph-fuse --client_fs mb-ci-fs "$RW" >"$D/ceph-fuse-rw.log" 2>&1;then
  echo 'FAIL: independent real CephFS RW mount for ACL/xattr test failed' >&2
  exit 1
fi
FSTYPE="$(findmnt -rn -T "$RW" -o FSTYPE)"
case "$FSTYPE" in fuse.ceph-fuse|ceph) ;; *)
 echo "FAIL: metadata test source is not genuine mounted CephFS ($FSTYPE)" >&2;exit 1;; esac
ceph fs set mb-ci-fs allow_new_snaps true >/dev/null
mkdir -p "$RW/r196-source/nested"
printf 'stable snapshot version\n' > "$RW/r196-source/nested/data.txt"
dd if=/dev/urandom of="$RW/r196-source/nested/payload.bin" bs=1M count=2 status=none
SOURCE_SHA=$(sha256sum "$RW/r196-source/nested/payload.bin" | awk '{print $1}')
chmod 0750 "$RW/r196-source/nested"
setfattr -n user.mb_ci_r196 -v 'genuine-cephfs-metadata' "$RW/r196-source/nested/data.txt"
setfacl -m u:1001:r-- "$RW/r196-source/nested/data.txt"
setfacl -m d:u:1001:r-x "$RW/r196-source/nested"
sync
mkdir "$RW/r196-source/.snap/frozen-r196"
test -r "$RW/r196-source/.snap/frozen-r196/nested/data.txt"
if ! ceph-fuse --client_fs mb-ci-fs -r /r196-source/.snap/frozen-r196 \
  -o ro "$SNAP" >"$D/mount.log" 2>&1;then
  echo "FAIL: cannot mount genuine immutable CephFS .snap subtree as an exact read-only mount" >&2
  exit 1
fi
findmnt -rn -T "$SNAP" -o OPTIONS | grep -Eq '(^|,)ro(,|$)'
# Mutations on the live source do not affect the distinct immutable snapshot.
(
  for i in $(seq 1 80);do
    printf 'concurrent-write-%s\n' "$i" > "$RW/r196-source/nested/data.txt"
    sleep 0.03
  done
) &
WRITER=$!
age-keygen -o "$D/identity" >/dev/null 2>&1
RECIPIENT=$(age-keygen -y "$D/identity")
"$CLI" storage init --root "$D/store" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins metadata-snapshot-backup --input cephfs --source "$SNAP" \
  --root "$D/store" --identity "$D/identity" --label cephfs-real-snapshot-acl-xattr \
  --source-snapshot-attested --json > "$D/snapshot.json"
ID=$(jq -er .backup_id "$D/snapshot.json")
jq -e '.provider=="cephfs" and .encrypted==true and .restores_acl_xattr==true' "$D/snapshot.json" >/dev/null
"$CLI" storage verify-chain --root "$D/store" --identity "$D/identity" --id "$ID" >/dev/null
"$CLI" plugins metadata-snapshot-restore --output cephfs --root "$D/store" \
  --identity "$D/identity" --id "$ID" \
  --destination "$RW/r196-restored-new" --json > "$D/recovery.json"
jq -e '.provider=="cephfs" and .restores_acl_xattr==true' "$D/recovery.json" >/dev/null
wait "$WRITER"
WRITER=""
test "$(cat "$RW/r196-restored-new/nested/data.txt")" = 'stable snapshot version'
test "$SOURCE_SHA" = "$(sha256sum "$RW/r196-restored-new/nested/payload.bin" | awk '{print $1}')"
test "$(getfattr --only-values -n user.mb_ci_r196 "$RW/r196-restored-new/nested/data.txt" 2>/dev/null)" = "genuine-cephfs-metadata"
getfacl -cp "$RW/r196-restored-new/nested/data.txt" | grep -Eq '^user:1001:r--$'
getfacl -cp "$RW/r196-restored-new/nested" | grep -Eq '^default:user:1001:r-x$'
if "$CLI" plugins metadata-snapshot-restore --output cephfs --root "$D/store" \
  --identity "$D/identity" --id "$ID" --destination "$RW/r196-restored-new" >/dev/null 2>&1;then
  echo 'FAIL: ACL/xattr restore overwrote existing target' >&2;exit 1
fi
echo 'PASS genuine concurrently mutated CephFS .snap, immutable PAX ACL+xattr encrypted Rust backup, create-only restore and independent metadata readback'
