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
cleanup() {
  set +e
  if mountpoint -q "$RO"; then sudo umount "$RO"; fi
  if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null || :; wait "$SERVER_PID" 2>/dev/null || :; fi
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
sudo mount -t davfs -o ro "https://localhost:18919/" "$RO"
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
if "$CLI" plugins backup --input webdav --output posix --source "$RO" \
  --root "$STORE" --identity "$IDENTITY" --label not-attested >/dev/null 2>&1; then
  echo "FAIL WebDAV allowed backup without external snapshot attestation" >&2; exit 1
fi
age-keygen -o "$IDENTITY" >/dev/null 2>&1
RECEIVER=$(sed -n 's/^# public key: //p' "$IDENTITY" | head -1)
"$CLI" storage init --root "$STORE" --recipient "$RECEIVER" >/dev/null
"$CLI" plugins backup --input webdav --output posix --source "$RO" \
  --root "$STORE" --identity "$IDENTITY" --label "genuine davfs2 input" \
  --namespace data --source-snapshot-attested --json > "$WORK/result.json"
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
echo "PASS authentic TLS+basic-auth rclone WebDAV server, real read-only davfs2 mount, Rust encrypted backup/verify/byte-for-byte local restore"
echo "NOT CERTIFIED real server-side snapshots, cache ETag fidelity, full metadata, WebDAV output/remote restore"
