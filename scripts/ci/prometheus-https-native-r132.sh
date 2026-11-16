#!/usr/bin/env bash
# Real multi-backup-server Rust TLS Prometheus acceptance, not a fake echo.
# Disposable hosted CI only: private 127/8 listeners and fresh ephemeral CA.
set -Eeuo pipefail
umask 077
if [[ "${GITHUB_ACTIONS:-}" != true || "${RUNNER_ENVIRONMENT:-}" != github-hosted ]];then
  echo "REFUSED: genuine monitoring lab may only run on disposable GitHub-hosted CI" >&2
  exit 3
fi
for cmd in openssl curl python3;do command -v "$cmd" >/dev/null;done
SERVER=rust-server/target/debug/multi-backup-server
test -x "$SERVER"
ROOT="$(mktemp -d)"
PID=""
cleanup(){
  if [[ -n "$PID" ]];then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true;fi
  rm -rf -- "$ROOT"
}
trap cleanup EXIT
chmod 0700 "$ROOT"
mkdir -m 0700 "$ROOT/data" "$ROOT/exports"
# Public CI password verifier fixture: no real credential is stored/used.
printf '%s\n' '$argon2id$v=19$m=65536,t=3,p=4$T3BlbkFJLU1CLUNJLXIwNDA$l6bp51sbsLonNBWiZD8ckafdxSbf/+nP19GSR0Tmzgc' > "$ROOT/hash"
openssl rand -hex 32 > "$ROOT/prometheus-token"
TOKEN="$(cat "$ROOT/prometheus-token")"
openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 1 \
  -subj '/CN=localhost' \
  -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1' \
  -keyout "$ROOT/metrics.key" -out "$ROOT/metrics.crt" >/dev/null 2>&1
openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 1 \
  -subj '/CN=wrong-ci-ca' \
  -keyout "$ROOT/wrong.key" -out "$ROOT/wrong.crt" >/dev/null 2>&1
chmod 0600 "$ROOT/metrics.key" "$ROOT/prometheus-token"
export MULTI_BACKUP_ADMIN_USER=ciadmin
export MULTI_BACKUP_ADMIN_HASH_FILE="$ROOT/hash"
export MULTI_BACKUP_PUBLIC_ORIGIN=http://127.0.0.1:8080
export MULTI_BACKUP_BIND=127.0.0.1:18080
export MULTI_BACKUP_DATA_ROOT="$ROOT/data"
export MULTI_BACKUP_EXPORT_ROOT="$ROOT/exports"
export MULTI_BACKUP_AUDIT_LOG="$ROOT/audit.jsonl"
export MULTI_BACKUP_HARDENED_MODE=false
export MULTI_BACKUP_PROMETHEUS_ENABLED=true
export MULTI_BACKUP_PROMETHEUS_BIND=127.0.0.1:19502
export MULTI_BACKUP_PROMETHEUS_TOKEN_FILE="$ROOT/prometheus-token"
export MULTI_BACKUP_PROMETHEUS_TLS_CERT_FILE="$ROOT/metrics.crt"
export MULTI_BACKUP_PROMETHEUS_TLS_KEY_FILE="$ROOT/metrics.key"
export MULTI_BACKUP_REST_API_ENABLED=false
"$SERVER" >"$ROOT/server.log" 2>&1 &
PID="$!"
URL=https://127.0.0.1:19502/metrics
ready=0
for attempt in $(seq 1 60);do
  if ! kill -0 "$PID" 2>/dev/null;then
    echo "FAIL: native Rust server exited before HTTPS readiness" >&2
    tail -n 35 "$ROOT/server.log" >&2
    exit 1
  fi
  if curl --fail --silent --show-error --max-time 2 \
    --cacert "$ROOT/metrics.crt" -H "Authorization: Bearer $TOKEN" \
    "$URL" >"$ROOT/metrics.txt" 2>/dev/null;then
    ready=1
    break
  fi
  sleep 1
done
if [[ "$ready" -ne 1 ]];then
  echo "FAIL: native Rust metrics HTTPS did not become ready" >&2
  tail -n 45 "$ROOT/server.log" >&2
  exit 1
fi
# Assert ACTUAL Rust production exposition; no fabricated extend fixture.
grep -qx 'multi_backup_up 1' "$ROOT/metrics.txt"
grep -qx 'multi_backup_cluster_nodes_total 1' "$ROOT/metrics.txt"
grep -qx 'multi_backup_database_primary 0' "$ROOT/metrics.txt"
if grep -Eiq 'secret|token_once|private_path|manifest|age.identity' "$ROOT/metrics.txt";then
  echo "FAIL: monitoring exposed credential/recovery metadata" >&2;exit 1
fi
code="$(curl -sS --max-time 4 --cacert "$ROOT/metrics.crt" \
  -o /dev/null -w '%{http_code}' "$URL")"
[[ "$code" == 401 ]] || { echo "FAIL: metrics without token returned $code" >&2;exit 1; }
code="$(curl -sS --max-time 4 --cacert "$ROOT/metrics.crt" \
  -H "Authorization: Bearer invalid-$(openssl rand -hex 8)" \
  -o /dev/null -w '%{http_code}' "$URL")"
[[ "$code" == 401 ]] || { echo "FAIL: wrong token returned $code" >&2;exit 1; }
if curl --fail --silent --show-error --max-time 4 \
  --cacert "$ROOT/wrong.crt" -H "Authorization: Bearer $TOKEN" \
  "$URL" >/dev/null 2>&1;then
  echo "FAIL: TLS accepted an independent untrusted CA" >&2;exit 1
fi
if curl --fail --silent --show-error --max-time 4 \
  -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:19502/metrics >/dev/null 2>&1;then
  echo "FAIL: HTTPS metrics listener also accepted plaintext HTTP" >&2;exit 1
fi
# Confirm the running server is bound only to the expected local address.
if command -v ss >/dev/null;then
  ss -lnt | grep -Eq '127\.0\.0\.1:19502[[:space:]]' || {
    echo "FAIL: native Prometheus is not bound to loopback" >&2;exit 1;
  }
fi
echo "PASS native Rust Prometheus HTTPS with trusted X.509, bearer, genuine counters; wrong credentials, foreign CA and plaintext HTTP denied"
echo "Scope: real standalone server on ephemeral loopback, not external VLAN or PostgreSQL REST API"
