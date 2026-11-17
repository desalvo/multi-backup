#!/usr/bin/env bash
# HTTPS oVirt Engine and IMAGEIO *protocol simulation*, not a real Engine.
set -Eeuo pipefail
umask 077
test "$GITHUB_ACTIONS" = true && test "$RUNNER_ENVIRONMENT" = github-hosted
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
TMP="$(mktemp -d)"
PID=""
cleanup(){
 if test -n "$PID";then kill "$PID" 2>/dev/null || :;wait "$PID" 2>/dev/null || :;fi
 rm -rf -- "$TMP"
}
trap cleanup EXIT
mkdir -m 0700 "$TMP/store"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
 -subj "/CN=mb-ovirt-protocol-root" -addext "basicConstraints=critical,CA:TRUE" \
 -keyout "$TMP/ca.key" -out "$TMP/ca.pem" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
 -subj "/CN=localhost" -keyout "$TMP/server.key" \
 -out "$TMP/server.csr" >/dev/null 2>&1
printf 'subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\n' > "$TMP/san.ext"
openssl x509 -req -in "$TMP/server.csr" -CA "$TMP/ca.pem" -CAkey "$TMP/ca.key" \
 -CAcreateserial -days 1 -out "$TMP/server.pem" \
 -extfile "$TMP/san.ext" >/dev/null 2>&1
sudo cp "$TMP/ca.pem" /usr/local/share/ca-certificates/multi-backup-ovirt-r213-test.crt
sudo update-ca-certificates >/dev/null
PORT=18734
export MB_OVIRT_VM=11111111-1111-1111-1111-111111111111
export MB_OVIRT_BACKUP=22222222-2222-2222-2222-222222222222
export MB_OVIRT_DISK=33333333-3333-3333-3333-333333333333
export MB_OVIRT_IMAGE=44444444-4444-4444-4444-444444444444
export MB_OVIRT_TRANSFER=55555555-5555-5555-5555-555555555555
export MB_OVIRT_TEST_TOKEN=mb-r213-throwaway-protocol-credential
python3 - "$TMP/payload.raw" "$TMP/config.json" "$TMP/mode.json" <<'PY'
import hashlib,json,os,sys
from pathlib import Path
data=bytes(range(256))*4096
Path(sys.argv[1]).write_bytes(data)
base="https://localhost:18734"
config={
 "engine_endpoint":base+"/","bearer_token":os.environ["MB_OVIRT_TEST_TOKEN"],
 "vm_id":os.environ["MB_OVIRT_VM"],"backup_id":os.environ["MB_OVIRT_BACKUP"],
 "disk_id":os.environ["MB_OVIRT_DISK"],"transfer_id":os.environ["MB_OVIRT_TRANSFER"],
 "transfer_url":base+"/images/"+os.environ["MB_OVIRT_TRANSFER"],
 "expected_bytes":len(data),"expected_format":"raw",
 "expected_sha256":hashlib.sha256(data).hexdigest()
}
p=Path(sys.argv[2]);p.write_text(json.dumps(config));p.chmod(0o600)
Path(sys.argv[3]).write_text('{"mode":"correct"}')
PY
python3 "$ROOT/scripts/ci/ovirt-https-imageio-fixture-r213.py" \
 --cert "$TMP/server.pem" --key "$TMP/server.key" \
 --data "$TMP/payload.raw" --state "$TMP/mode.json" \
 --port "$PORT" --token "$MB_OVIRT_TEST_TOKEN" >"$TMP/server.log" 2>&1 &
PID=$!
ok=0
for _ in $(seq 1 40);do
 if curl --silent --show-error --cacert "$TMP/ca.pem" \
   -H "Authorization: Bearer $MB_OVIRT_TEST_TOKEN" \
   -H 'Version: 4' -H 'Accept: application/json' \
   "https://localhost:$PORT/ovirt-engine/api/vms/$MB_OVIRT_VM/backups/$MB_OVIRT_BACKUP" \
   | grep -q '"ready"';then ok=1;break;fi
 if ! kill -0 "$PID" 2>/dev/null;then break;fi
 sleep 1
done
test "$ok" = 1 || { echo 'FAIL oVirt HTTPS fixture startup' >&2;exit 1;}
age-keygen -o "$TMP/age.key" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TMP/age.key")"
"$NATIVE" storage init --root "$TMP/store" --recipient "$RECIPIENT" >/dev/null
"$NATIVE" plugins ovirt-imageio-backup --config "$TMP/config.json" \
 --root "$TMP/store" --identity "$TMP/age.key" --label "read-only protocol lab r213" \
 --json > "$TMP/receipt.json"
ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/receipt.json")"
"$NATIVE" storage restore --root "$TMP/store" --identity "$TMP/age.key" \
 --id "$ID" --destination "$TMP/restore" >/dev/null
RECOVERED="$TMP/restore/ovirt/vm/$MB_OVIRT_VM/backup/$MB_OVIRT_BACKUP/$MB_OVIRT_DISK.raw"
cmp "$TMP/payload.raw" "$RECOVERED"
for mode in not-ready wrong-disk incomplete-chain wrong-direction bad-body short-body;do
 printf '{"mode":"%s"}\n' "$mode" > "$TMP/mode.json"
 if "$NATIVE" plugins ovirt-imageio-backup --config "$TMP/config.json" \
   --root "$TMP/store" --identity "$TMP/age.key" --label "negative-$mode" \
   --json >"$TMP/negative-$mode.json" 2>"$TMP/negative-$mode.err";then
   echo "FAIL oVirt adapter accepted $mode" >&2;exit 1
 fi
done
printf '{"mode":"correct"}\n' > "$TMP/mode.json"
chmod 0644 "$TMP/config.json"
if "$NATIVE" plugins ovirt-imageio-backup --config "$TMP/config.json" \
  --root "$TMP/store" --identity "$TMP/age.key" --label "unsafe config" \
  >/dev/null 2>&1;then
  echo 'FAIL owner-only oVirt config permission bypass' >&2;exit 1
fi
echo 'PASS authenticated Engine/IMAGEIO TLS protocol, READY+disk+backup proof, age raw byte restore, six fail-closed modes and 0600 config'
echo 'NOT verified real oVirt Engine, VM host, ticket format compatibility, VM creation/restore or boot'
