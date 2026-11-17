#!/usr/bin/env bash
# Real Lustre QEMU guest only, with actual kernel, MDS/OST and client mounts.
# CI provides only public static executables over throwaway host bridge.
set -Eeuo pipefail
umask 077
: "$MB_LUSTRE_HTTP_PORT"
test -n "$MB_LUSTRE_HTTP_PORT"
command -v lfs && command -v findmnt && command -v curl
lsmod | grep -q '^lustre '
CLIENT="$(findmnt -rn -t lustre -o TARGET | head -1)"
test -n "$CLIENT"
findmnt -rn -T "$CLIENT" -o FSTYPE | grep -qx lustre
GATEWAY="$(ip -4 route show default | awk '{print $3;exit}')"
test -n "$GATEWAY" || { echo 'FAIL: guest cannot reach disposable runner gateway';exit 1; }
WORK="$(mktemp -d)"
RO=/mnt/mb-rust-lustre-ro
mkdir -p "$RO"
cleanup(){
  set +e
  mountpoint -q "$RO" && umount "$RO"
  rm -rf -- "$WORK"
}
trap cleanup EXIT
test "${MB_LUSTRE_BINARY_SHA:-}" != "" || exit 1
test "${MB_LUSTRE_AGE_SHA:-}" != "" || exit 1
test "${MB_LUSTRE_KEYGEN_SHA:-}" != "" || exit 1
for bin in multi-backup-native age age-keygen;do
  curl --fail --silent --show-error --retry 4 \
    "http://$GATEWAY:$MB_LUSTRE_HTTP_PORT/$bin" -o "$WORK/$bin"
  chmod 0755 "$WORK/$bin"
done
printf '%s  %s\n' "$MB_LUSTRE_BINARY_SHA" "$WORK/multi-backup-native" \
  "$MB_LUSTRE_AGE_SHA" "$WORK/age" \
  "$MB_LUSTRE_KEYGEN_SHA" "$WORK/age-keygen" | sha256sum -c -
export PATH="$WORK:$PATH"
CLI="$WORK/multi-backup-native"
"$CLI" --version
source="$(findmnt -rn -T "$CLIENT" -o SOURCE)"
test -n "$source"
TESTDIR="$CLIENT/mb-rust-ci-$RANDOM-$RANDOM"
mkdir -p "$TESTDIR/nested"
printf 'genuine Lustre Rust full backup\n' > "$TESTDIR/nested/source.txt"
lfs setstripe -c 2 -S 1M "$TESTDIR/stripe.bin"
dd if=/dev/urandom of="$TESTDIR/stripe.bin" bs=1M count=2 status=none
test "$(lfs getstripe -c "$TESTDIR/stripe.bin")" -eq 2
HASH="$(sha256sum "$TESTDIR/stripe.bin" | awk '{print $1}')"
mount -t lustre -o ro "$source" "$RO"
findmnt -rn -T "$RO" -o FSTYPE,OPTIONS | grep -Eq '^lustre .*ro'
test -f "$RO/$(basename "$TESTDIR")/nested/source.txt"
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$WORK/identity")"
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
if "$CLI" plugins backup --input lustre --output posix \
  --source "$RO" --root "$WORK/store" --identity "$WORK/identity" \
  --label missing-attestation >/dev/null 2>&1;then
  echo 'FAIL: no snapshot/quiescence attestation rejected' >&2;exit 1
fi
"$CLI" plugins backup --input lustre --output posix \
  --source "$RO" --root "$WORK/store" --identity "$WORK/identity" \
  --label genuine-lustre-full --namespace lustre --source-snapshot-attested \
  --json > "$WORK/full.json"
FULL="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$WORK/full.json")"
"$CLI" storage verify-chain --root "$WORK/store" --identity "$WORK/identity" --id "$FULL"
RESTORED="$CLIENT/mb-recovered-$RANDOM-$RANDOM"
"$CLI" plugins restore --input posix --output lustre \
  --root "$WORK/store" --identity "$WORK/identity" --id "$FULL" \
  --destination "$RESTORED"
cmp "$TESTDIR/nested/source.txt" "$RESTORED/lustre/$(basename "$TESTDIR")/nested/source.txt"
test "$HASH" = "$(sha256sum "$RESTORED/lustre/$(basename "$TESTDIR")/stripe.bin" | awk '{print $1}')"
if "$CLI" plugins restore --input posix --output lustre \
  --root "$WORK/store" --identity "$WORK/identity" --id "$FULL" \
  --destination "$RESTORED" >/dev/null 2>&1;then
  echo 'FAIL: Lustre restore overwrote data' >&2;exit 1
fi
mkdir -p "$WORK/new-source/nested"
printf 'Lustre encrypted remote output\n' > "$WORK/new-source/nested/value.txt"
"$CLI" plugins mounted-backup --output lustre --mount "$CLIENT" \
  --source "$WORK/new-source" --filename "mb-ci-$RANDOM.tar.gz.age" \
  --recipient "$RECIPIENT" --identity "$WORK/identity" \
  --json >"$WORK/output.json"
python3 - "$WORK/output.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1]))
assert x["encrypted"] is True and x["create_only"] is True
assert x["verified_members"]==x["source_members"]
PY
NAME="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["archive"].rsplit("/",1)[-1])' "$WORK/output.json")"
"$CLI" plugins mounted-restore --input lustre --mount "$CLIENT" \
  --filename "$NAME" --identity "$WORK/identity" \
  --destination "$WORK/remote-age-recovered"
cmp "$WORK/new-source/nested/value.txt" \
  "$WORK/remote-age-recovered/multi-backup/mounted-output/nested/value.txt"
if "$CLI" plugins mounted-backup --output lustre --mount "$CLIENT" \
  --source "$WORK/new-source" --filename "$NAME" \
  --recipient "$RECIPIENT" --identity "$WORK/identity" >/dev/null 2>&1;then
  echo 'FAIL: Lustre backup overwrote encrypted archive' >&2;exit 1
fi
echo 'PASS real Lustre QEMU MDS/OST client, encrypted Rust backup, create-only Lustre restore and mounted ciphertext B-Out'
echo 'NOT CERTIFIED Lustre MDT/OST snapshot consistency or stripe/xattrs/mode preservation on restored target'
