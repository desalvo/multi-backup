#!/usr/bin/env bash
# Black-box OCI security regression: no real user secrets or production vaults.
# The test starts a public development image behind a loopback-only host port.
set -euo pipefail
IMAGE="${1:-multi-backup:test}"
tmp="$(mktemp -d)"
container=""
cleanup() {
  if [[ -n "$container" ]]; then
    docker rm -f "$container" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp" || true
}
trap cleanup EXIT
chmod 0755 "$tmp"
# Publicly known test-only PHC fixture. This password is NOT a deployment secret.
printf '%s\n' '$argon2id$v=19$m=65536,t=3,p=4$mxzRPI71Fc4HKAuFpBfrFQ$sT2wAmv5Qrsyq8bzBSgkDeGnJCUHYHE5DIhEf/0hjgA' > "$tmp/admin-hash"
gate="$(openssl rand -hex 32)"
recovery="$(openssl rand -hex 32)"
while [[ "$gate" == "$recovery" ]]; do recovery="$(openssl rand -hex 32)"; done
printf '%s\n' "$gate" > "$tmp/private-gate"
printf '%s\n' "$recovery" > "$tmp/recovery-secret"
chmod 0640 "$tmp"/{admin-hash,private-gate,recovery-secret}
# The OCI runtime uses UID:GID 10001:10001, not the host runner user.
sudo chown 10001:10001 "$tmp"/{admin-hash,private-gate,recovery-secret}
container="$(docker run -d --publish 127.0.0.1::8080 \
    --mount "type=bind,src=$tmp,dst=/run/secrets,readonly" \
    --env MULTI_BACKUP_ADMIN_USER=ciadmin \
    --env MULTI_BACKUP_ADMIN_HASH_FILE=/run/secrets/admin-hash \
    --env MULTI_BACKUP_PUBLIC_ORIGIN=https://backup.example.org \
    --env MULTI_BACKUP_HARDENED_MODE=true \
    --env MULTI_BACKUP_PRIVATE_GATE_SECRET_FILE=/run/secrets/private-gate \
    --env MULTI_BACKUP_RECOVERY_SECRET_FILE=/run/secrets/recovery-secret \
    "$IMAGE")"
port="$(docker port "$container" 8080/tcp | head -n1)"
[[ "$port" == 127.0.0.1:* ]] || { echo "Refusing non-loopback published port: $port" >&2; exit 1; }
url="http://$port"
ready=0
for attempt in {1..40}; do
  if curl --fail --silent --max-time 2 "$url/healthz" >/dev/null; then ready=1; break; fi
  sleep 1
done
if [[ "$ready" -ne 1 ]]; then
  echo 'Container did not reach a live healthz state' >&2
  docker logs "$container" >&2 || true
  exit 1
fi
code() { curl --silent --show-error --max-time 5 -o /dev/null -w '%{http_code}' "$@"; }
expect() {
  local wanted="$1" actual
  shift
  actual="$(code "$@")"
  [[ "$actual" == "$wanted" ]] || { echo "HTTP regression: expected $wanted, received $actual" >&2; exit 1; }
}
expect 200 "$url/healthz"
expect 404 "$url/login"
expect 404 -H 'X-Multi-Backup-Private-Gate: invalid' "$url/login"
expect 200 -H "X-Multi-Backup-Private-Gate: $gate" "$url/login"
expect 401 -H "X-Multi-Backup-Private-Gate: $gate" "$url/api/backups"
expect 401 -H "X-Multi-Backup-Private-Gate: $gate" "$url/api/security/status"
expect 401 -H "X-Multi-Backup-Private-Gate: $gate" -H "X-Multi-Backup-Recovery-Authorization: $recovery" \
  -H 'Content-Type: application/json' \
  --data '{"backup_id":"nonexistent","selections":[]}' "$url/api/export"
echo 'OCI gateway security smoke: PASS (health, unknown/wrong/correct gate, session, recovery)'
