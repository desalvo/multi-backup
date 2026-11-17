#!/usr/bin/env bash
# Genuine hosted TLS+authenticated WebDAV server and davfs2 kernel/FUSE client.
# Synthetic quiescent fixture only; never claims actual provider snapshot fidelity.
set -euo pipefail
umask 077
test "${RUNNER_ENVIRONMENT:-}" = github-hosted || { echo "Only dedicated hosted CI" >&2; exit 1; }
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
WORK=$(mktemp -d "$HOME/.mb-webdav-r046.XXXXXXXX")
RO="$WORK/ro"; ROOT="$WORK/export"; STORE="$WORK/age-store"; IDENTITY="$WORK/age-identity"
mkdir -p "$RO" "$ROOT/nested"
SERVER_PID=""
WRITE_PID=""
cleanup() {
  set +e
  if test -n "${RW:-}" && mountpoint -q "$RW"; then sudo umount "$RW"; fi
  if mountpoint -q "$RO"; then sudo umount "$RO"; fi
  if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null || :; wait "$SERVER_PID" 2>/dev/null || :; fi
  if [ -n "$WRITE_PID" ]; then kill "$WRITE_PID" 2>/dev/null || :; wait "$WRITE_PID" 2>/dev/null || :; fi
  sudo rm -rf -- "$WORK"
}
trap cleanup EXIT
printf "WebDAV original content\n" > "$ROOT/nested/data.txt"
dd if=/dev/urandom of="$ROOT/test.bin" bs=1M count=2 status=none
HASH=$(sha256sum "$ROOT/test.bin" | awk '{print $1}')
openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 1 \
  -subj /CN=localhost -addext subjectAltName=DNS:localhost,IP:127.0.0.1 \
  -keyout "$WORK/webdav.key" -out "$WORK/webdav.crt" >/dev/null 2>&1
chmod 0600 "$WORK/webdav.key"
sudo install -m 0644 "$WORK/webdav.crt" /usr/local/share/ca-certificates/mb-webdav-r046.crt
sudo update-ca-certificates >/dev/null
PASSWORD=$(openssl rand -hex 20)
htpasswd -nbB mbci "$PASSWORD" > "$WORK/htpasswd"
rclone serve webdav "$ROOT" --addr 127.0.0.1:18919 --cert "$WORK/webdav.crt" \
  --key "$WORK/webdav.key" --htpasswd "$WORK/htpasswd" --read-only \
  --min-tls-version tls1.2 > "$WORK/server.log" 2>&1 &
SERVER_PID=$!
for n in $(seq 1 30); do
  if curl -fsS -u "mbci:$PASSWORD" https://localhost:18919/ >/dev/null 2>&1; then break; fi
  sleep 1
done
curl -fsS -u "mbci:$PASSWORD" https://localhost:18919/nested/data.txt >/dev/null
if curl -fsS https://localhost:18919/nested/data.txt >/dev/null 2>&1; then
  echo "FAIL unauthenticated server access" >&2; exit 1
fi
printf "%s %s %s\n" "https://localhost:18919/" mbci "$PASSWORD" | sudo tee -a /etc/davfs2/secrets >/dev/null
sudo chmod 0600 /etc/davfs2/secrets
unset PASSWORD
# davfs2 1.7.1 on newer Linux kernels may mount successfully but return
# EINVAL from getdents64/readdir due to an undersized FUSE buffer.
# Official davfs2 fix is 1.7.2; documented compatibility workaround: buf_size 64.
# Apply ONLY inside the disposable GitHub-hosted runner, before mounting.
printf '\n# Multi Backup CI: davfs2 kernel/FUSE readdir compatibility\nbuf_size 64\n' \
  | sudo tee -a /etc/davfs2/davfs2.conf >/dev/null
sudo mount -t davfs -o ro "https://localhost:18919/" "$RO"
# Do not mask readdir errors or accept an empty mounted directory.
if ! ls -la "$RO" > "$WORK/davfs-directory-list.txt"; then
  echo "FAIL: actual davfs2 directory readdir failed (check buf_size/kernel)" >&2
  exit 1
fi
test -r "$RO/nested/data.txt" && test -r "$RO/test.bin" || {
  echo "FAIL: expected authenticated WebDAV fixture is not readable" >&2; exit 1;
}
FSTYPE="$(findmnt -rn -T "$RO" -o FSTYPE)"
MOUNT_SOURCE="$(findmnt -rn -T "$RO" -o SOURCE)"
case "$FSTYPE" in
    fuse.davfs) ;;
    fuse)
      # Ubuntu's genuine davfs2 helper exposes a generic FUSE type.
      # The exact HTTPS source plus the Rust mountinfo attestation must agree.
      test "$MOUNT_SOURCE" = "https://localhost:18919/" || {
        echo "FAIL generic FUSE mount lacks exact HTTPS WebDAV origin" >&2; exit 1;
      } ;;
    *) printf 'FAIL unexpected WebDAV FSTYPE %s\n' "$FSTYPE" >&2; exit 1 ;;
esac
"$CLI" plugins probe --plugin webdav --mount "$RO" --json > "$WORK/probe.json"
if ! jq -e '.rw_mount_option == false and (.fs_type == "fuse.davfs" or .fs_type == "fuse")' "$WORK/probe.json" >/dev/null; then
  echo "FAIL WebDAV probe did not attest read-only HTTPS davfs2 mount" >&2
  cat "$WORK/probe.json" >&2
  exit 1
fi
# davfs2 shows a LOCAL recovery/cache directory named lost+found at mount
# root. Prove that it is not a remote WebDAV collection before excluding it.
HTTP_CODE="$(curl -sS -u "mbci:$(sudo awk '$1=="https://localhost:18919/" {print $3}' /etc/davfs2/secrets | tail -1)" \
  -X PROPFIND -H 'Depth: 0' -o "$WORK/virtual-davfs-response" \
  -w '%{http_code}' 'https://localhost:18919/lost+found/')"
test "$HTTP_CODE" = 404 || {
  echo "FAIL: davfs recovery directory is present on server or unverifiable; do not exclude" >&2
  exit 1
}
echo "PASS: WebDAV server has no lost+found; davfs2 generated local recovery directory can be excluded"
if "$CLI" plugins backup --input webdav --output posix --source "$RO" \
  --root "$STORE" --identity "$IDENTITY" --label not-attested >/dev/null 2>&1; then
  echo "FAIL WebDAV allowed backup without external snapshot attestation" >&2; exit 1
fi
age-keygen -o "$IDENTITY" >/dev/null 2>&1
RECEIVER=$(sed -n 's/^# public key: //p' "$IDENTITY" | head -1)
"$CLI" storage init --root "$STORE" --recipient "$RECEIVER" >/dev/null
"$CLI" plugins backup --input webdav --output posix --source "$RO" \
  --root "$STORE" --identity "$IDENTITY" --label "genuine davfs2 input" \
  --namespace data --source-snapshot-attested --davfs-cache-not-on-remote \
  --json > "$WORK/result.json"
SNAPSHOT=$(jq -er '.id | select(length==64)' "$WORK/result.json")
"$CLI" storage verify-chain --root "$STORE" --identity "$IDENTITY" --id "$SNAPSHOT" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$STORE" \
  --identity "$IDENTITY" --id "$SNAPSHOT" --destination "$WORK/recovered" >/dev/null
cmp "$ROOT/nested/data.txt" "$WORK/recovered/data/nested/data.txt"
cmp "$ROOT/test.bin" "$WORK/recovered/data/test.bin"
test "$HASH" = "$(sha256sum "$WORK/recovered/data/test.bin" | awk '{print $1}')"
if "$CLI" plugins restore --input posix --output webdav --root "$STORE" \
  --identity "$IDENTITY" --id "$SNAPSHOT" --destination "$RO/not-supported" >/dev/null 2>&1; then
  echo "FAIL unimplemented WebDAV output allowed" >&2; exit 1
fi
# r183: independent genuine HTTPS/Basic Auth RW WebDAV server and
# actual davfs2 RW mounted B-Out with exact ciphertext downloaded over HTTP.
# No production DAV endpoint or personal cloud account involved.
RW="$WORK/rw"
mkdir -p "$RW" "$WORK/local-output/nested"
printf 'real authenticated RW WebDAV age output\n' > "$WORK/local-output/nested/source.txt"
AUTH_PASS="$(sudo awk '$1=="https://localhost:18919/" {print $3}' /etc/davfs2/secrets | tail -1)"
test -n "$AUTH_PASS"
rclone serve webdav "$ROOT" --addr 127.0.0.1:18920 \
  --cert "$WORK/webdav.crt" --key "$WORK/webdav.key" \
  --htpasswd "$WORK/htpasswd" --min-tls-version tls1.2 \
  > "$WORK/write-server.log" 2>&1 &
WRITE_PID=$!
for n in $(seq 1 30);do
  if curl -fsS -u "mbci:$AUTH_PASS" https://localhost:18920/nested/data.txt >/dev/null 2>&1;then break;fi
  sleep 1
done
curl -fsS -u "mbci:$AUTH_PASS" https://localhost:18920/nested/data.txt >/dev/null
printf '%s %s %s\n' 'https://localhost:18920/' mbci "$AUTH_PASS" | sudo tee -a /etc/davfs2/secrets >/dev/null
sudo chmod 0600 /etc/davfs2/secrets
sudo mount -t davfs -o "rw,uid=$(id -u),gid=$(id -g),file_mode=0600,dir_mode=0700" https://localhost:18920/ "$RW"
# davfs2 mounted with sudo is root-owned unless the CI user's UID/GID are
# explicit; a hidden tempfile must be writable without running the Rust
# backup process as root.
test -w "$RW" && touch "$RW/.mb-write-probe"
rm -f -- "$RW/.mb-write-probe"
"$CLI" plugins probe --plugin webdav --mount "$RW" --json \
  | jq -e '.rw_mount_option == true and (.fs_type == "fuse.davfs" or .fs_type == "fuse")'
# Native HTTPS API B-Out is intentionally distinct from the mounted
# davfs2 B-Out: the latter ACKed cached writes without remote durability.
# We require actual HTTP PUT (If-None-Match: *) and an independent HTTPS GET
# for BOTH ciphertext and completion marker before returning success.
if "$CLI" plugins mounted-backup --output webdav --mount "$RO" \
  --source "$WORK/local-output" --filename should-not-write.tar.gz.age \
  --recipient "$RECEIVER" --identity "$IDENTITY" >/dev/null 2>&1;then
  echo 'FAIL: read-only WebDAV mount accepted B-Out' >&2;exit 1
fi
python3 - "$WORK/webdav-output.json" <<'PY'
import json,os,sys
path=sys.argv[1]
with open(path,"x",encoding="utf-8") as target:
    json.dump({"endpoint":"https://127.0.0.1:18920/","username":"mbci"},target)
os.chmod(path,0o600)
PY
export MULTI_BACKUP_WEBDAV_PASSWORD="$AUTH_PASS"
"$CLI" plugins webdav-http-backup \
  --config "$WORK/webdav-output.json" \
  --source "$WORK/local-output" --filename new-webdav.tar.gz.age \
  --recipient "$RECEIVER" --identity "$IDENTITY" \
  --json > "$WORK/webdav-http-out.json"
jq -e '.create_only and .encrypted and .independent_https_readback and (.verified_members > 0)' "$WORK/webdav-http-out.json"
# Verify the HTTP endpoint *again*, independent of the Rust verification;
# still no WebDAV FUSE cache or client-side mount in the proof.
curl --fail --silent --show-error -u "mbci:$AUTH_PASS" \
  https://localhost:18920/new-webdav.tar.gz.age \
  -o "$WORK/downloaded-ciphertext.tar.gz.age"
curl --fail --silent --show-error -u "mbci:$AUTH_PASS" \
  https://localhost:18920/new-webdav.tar.gz.age.sha256 \
  -o "$WORK/remote-completion.sha256"
ACTUAL_HASH="$(sha256sum "$WORK/downloaded-ciphertext.tar.gz.age" | awk '{print $1}')"
test "$ACTUAL_HASH" = "$(jq -r .sha256 "$WORK/webdav-http-out.json")"
test "$ACTUAL_HASH" = "$(awk 'NR==1{print $1}' "$WORK/remote-completion.sha256")"
"$CLI" plugins webdav-http-restore --config "$WORK/webdav-output.json" \
  --filename new-webdav.tar.gz.age --identity "$IDENTITY" \
  --destination "$WORK/restored-from-webdav" --json > "$WORK/webdav-recover.json"
jq -e '.restored_members>0' "$WORK/webdav-recover.json"
cmp "$WORK/local-output/nested/source.txt" \
  "$WORK/restored-from-webdav/multi-backup/webdav-native/nested/source.txt"
if "$CLI" plugins webdav-http-backup \
  --config "$WORK/webdav-output.json" \
  --source "$WORK/local-output" --filename new-webdav.tar.gz.age \
  --recipient "$RECEIVER" --identity "$IDENTITY" >/dev/null 2>&1;then
  echo 'FAIL: WebDAV native API overwrote existing encrypted object' >&2;exit 1
fi
if "$CLI" plugins webdav-http-restore --config "$WORK/webdav-output.json" \
  --filename new-webdav.tar.gz.age --identity "$IDENTITY" \
  --destination "$WORK/restored-from-webdav" >/dev/null 2>&1;then
  echo 'FAIL: WebDAV native restore overwrote existing local directory' >&2;exit 1
fi
unset MULTI_BACKUP_WEBDAV_PASSWORD AUTH_PASS
echo 'PASS genuine TLS-authenticated WebDAV API ciphertext create-only B-Out, independent GET, sha256 marker and age restore'

echo "PASS authentic TLS+basic-auth rclone WebDAV server, real read-only davfs2 mount, Rust encrypted backup/verify/byte-for-byte local restore"
echo "NOT CERTIFIED remote ETag/ACL metadata, large transfers and provider-specific WebDAV locking/HA; mounted davfs2 RW is not a durable B-Out"
