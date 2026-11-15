#!/usr/bin/env bash
# Actual Kubernetes API/server/etcd/CNI/scheduler on three kind nodes (two workers).
# Only stateless API + PostgreSQL TLS/session-control-plane acceptance here;
# no shared storage/physical writer fencing certification is implied.
set -euo pipefail
NS=multi-backup-ci
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
PF_PID=""
PF_ONE_PID=""
PF_TWO_PID=""
cleanup() {
  for pid in "$PF_PID" "$PF_ONE_PID" "$PF_TWO_PID"; do
    if [[ -n "$pid" ]]; then kill "$pid" >/dev/null 2>&1 || true; wait "$pid" 2>/dev/null || true; fi
  done
  rm -rf "$TMP"
}
trap cleanup EXIT
chmod 0700 "$TMP"
kubectl create namespace "$NS"
# Ephemeral private CA + SAN-authenticated DB server certificate; no CI secrets.
openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 2 \
  -subj /CN=mb-kind-ci-root -keyout "$TMP/ca.key" -out "$TMP/ca.crt" >/dev/null 2>&1
openssl req -new -newkey rsa:2048 -nodes -sha256 \
  -subj /CN=pg-cluster.multi-backup-ci.svc.cluster.local \
  -keyout "$TMP/server.key" -out "$TMP/server.csr" >/dev/null 2>&1
cat > "$TMP/server.ext" <<'EXT'
subjectAltName=DNS:pg-cluster,DNS:pg-cluster.multi-backup-ci.svc,DNS:pg-cluster.multi-backup-ci.svc.cluster.local
extendedKeyUsage=serverAuth
EXT
openssl x509 -req -days 2 -sha256 -in "$TMP/server.csr" \
  -CA "$TMP/ca.crt" -CAkey "$TMP/ca.key" -CAcreateserial \
  -extfile "$TMP/server.ext" -out "$TMP/server.crt" >/dev/null 2>&1
cp "$TMP/ca.crt" "$TMP/db-ca.pem"
# Public fixture and known CI-only password, both intentionally non-secret.
# DO NOT reuse these values for real deployments.
printf '%s\n' '$argon2id$v=19$m=65536,t=3,p=4$T3BlbkFJLU1CLUNJLXIwNDA$l6bp51sbsLonNBWiZD8ckafdxSbf/+nP19GSR0Tmzgc' > "$TMP/admin-hash"
openssl rand -hex 32 > "$TMP/private-gate"
openssl rand -hex 32 > "$TMP/recovery-secret"
DB_PASSWORD="$(openssl rand -hex 24)"
DSN="postgresql://mbuser:${DB_PASSWORD}@pg-cluster.multi-backup-ci.svc.cluster.local:5432/multi_backup?sslmode=verify-full"
kubectl -n "$NS" create secret generic mb-ci-db-tls \
  --from-file=server.crt="$TMP/server.crt" --from-file=server.key="$TMP/server.key" >/dev/null
kubectl -n "$NS" create secret generic mb-ci-postgres \
  --from-literal=password="$DB_PASSWORD" >/dev/null
kubectl -n "$NS" create secret generic mb-ci-app \
  --from-file=admin-hash="$TMP/admin-hash" \
  --from-file=private-gate="$TMP/private-gate" \
  --from-file=recovery-secret="$TMP/recovery-secret" \
  --from-file=db-ca.pem="$TMP/db-ca.pem" --from-literal=dsn="$DSN" >/dev/null
unset DSN
kubectl -n "$NS" apply -f "$ROOT/tests/kubernetes/ci-cluster.yaml"
kubectl -n "$NS" rollout status deployment/mb-ci-postgres --timeout=240s
kubectl -n "$NS" rollout status deployment/mb-ci-server --timeout=360s
check_pods() {
  kubectl -n "$NS" get pods -l app=mb-ci-server -o json | python3 -c '
import sys,json
pods=json.load(sys.stdin)["items"]
assert len(pods)==2, f"expected 2 server pods, got {len(pods)}"
nodes=[p["spec"].get("nodeName") for p in pods]
assert all(nodes) and len(set(nodes))==2, f"not scheduled on different worker nodes: {nodes}"
assert all(p["status"].get("phase")=="Running" for p in pods)
assert all(any(c.get("type")=="Ready" and c.get("status")=="True"
               for c in p["status"].get("conditions",[])) for p in pods)
print("Two ready Rust server replicas on distinct real Kubernetes workers:", nodes)
'
}
check_pods
mapfile -t PODS < <(kubectl -n "$NS" get pods -l app=mb-ci-server -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')
check_db_replica() {
  local id="$1" observed
  [[ "$id" =~ ^[a-zA-Z0-9-]+$ ]] || exit 1
  observed="$(kubectl -n "$NS" exec deployment/mb-ci-postgres -c postgres -- \
    psql -U mbuser -d multi_backup -At \
    -c "SELECT COUNT(*) FROM mb_cluster_replicas WHERE replica_id='${id}' AND last_seen>now()-interval '90 seconds'")"
  [[ "$observed" == 1 ]] || { echo "PG verified-TLS replica registration missing: $id" >&2; exit 1; }
}
for pod in "${PODS[@]}"; do check_db_replica "$pod"; done
echo "Real PostgreSQL 16 TLS verify-full control plane: both replicas registered"
kubectl -n "$NS" port-forward service/mb-ci-server 18080:8080 > "$TMP/port-forward.log" 2>&1 &
PF_PID="$!"
ready=0
for i in {1..45}; do
  if curl -fsS --max-time 2 http://127.0.0.1:18080/healthz >/dev/null 2>&1; then ready=1; break; fi
  sleep 2
done
[[ "$ready" == 1 ]] || { cat "$TMP/port-forward.log" >&2; exit 1; }
http_code() {
  curl --silent --show-error --max-time 5 -o /dev/null -w '%{http_code}' "$@"
}
test "$(http_code http://127.0.0.1:18080/healthz)" = 200
test "$(http_code http://127.0.0.1:18080/login)" = 404
test "$(http_code -H 'X-Multi-Backup-Private-Gate: invalid' http://127.0.0.1:18080/login)" = 404
test "$(http_code -H "X-Multi-Backup-Private-Gate: $(cat "$TMP/private-gate")" http://127.0.0.1:18080/login)" = 200
test "$(http_code -H "X-Multi-Backup-Private-Gate: $(cat "$TMP/private-gate")" http://127.0.0.1:18080/api/cluster/status)" = 401
echo "ClusterIP Service and hardened private gateway: PASS"

# Authenticate through Pod A, use the SAME PostgreSQL-backed session on Pod B,
# then revoke on Pod B and prove rejection on Pod A. A Service port-forward
# alone is pinned to one pod, so direct private pod forwards are intentional.
kubectl -n "$NS" port-forward "pod/${PODS[0]}" 18181:8080 > "$TMP/pod-one.log" 2>&1 &
PF_ONE_PID="$!"
kubectl -n "$NS" port-forward "pod/${PODS[1]}" 18182:8080 > "$TMP/pod-two.log" 2>&1 &
PF_TWO_PID="$!"
for port in 18181 18182; do
  ready=0
  for i in {1..45}; do
    if curl -fsS --max-time 2 "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then ready=1; break; fi
    sleep 1
  done
  [[ "$ready" == 1 ]] || { echo "Pod port forward failed on $port" >&2; exit 1; }
done
GATE="$(cat "$TMP/private-gate")"
login_code="$(curl -sS --max-time 15 -D "$TMP/login-headers" -o /dev/null -w '%{http_code}' \
  -H "X-Multi-Backup-Private-Gate: $GATE" \
  -H 'Origin: https://backup.example.invalid' \
  --data-urlencode 'username=ciadmin' \
  --data-urlencode 'password=MB-CI-Only-Password-2026!' \
  http://127.0.0.1:18181/login)"
[[ "$login_code" == 303 ]] || { echo "Real Pod A login failed: HTTP $login_code" >&2; exit 1; }
token="$(sed -nE 's/^[Ss]et-[Cc]ookie:[[:space:]]*mb_session=([[:xdigit:]]{64}).*/\1/p' "$TMP/login-headers" | tr -d '\r' | head -n1)"
[[ "$token" =~ ^[[:xdigit:]]{64}$ ]] || { echo "Login session cookie missing" >&2; exit 1; }
for port in 18181 18182; do
  test "$(http_code -H "X-Multi-Backup-Private-Gate: $GATE" \
    -H "Cookie: mb_session=$token" "http://127.0.0.1:$port/")" = 200
  test "$(http_code -H "X-Multi-Backup-Private-Gate: $GATE" \
    -H "Cookie: mb_session=$token" "http://127.0.0.1:$port/api/cluster/status")" = 200
done
csrf="$(curl -fsS -H "X-Multi-Backup-Private-Gate: $GATE" \
  -H "Cookie: mb_session=$token" http://127.0.0.1:18182/ |
  sed -nE 's/.*name="csrf-token" content="([[:xdigit:]]{64})".*/\1/p' | head -n1)"
[[ "$csrf" =~ ^[[:xdigit:]]{64}$ ]] || { echo "Shared CSRF token missing" >&2; exit 1; }
test "$(http_code -X POST -H "X-Multi-Backup-Private-Gate: $GATE" \
  -H 'Origin: https://backup.example.invalid' \
  -H "Cookie: mb_session=$token" -H "x-csrf-token: $csrf" \
  http://127.0.0.1:18182/logout)" = 303
test "$(http_code -H "X-Multi-Backup-Private-Gate: $GATE" \
  -H "Cookie: mb_session=$token" http://127.0.0.1:18181/)" = 303
echo "Cross-Pod A/B login, admin status, shared CSRF and logout revocation: PASS"
kill "$PF_ONE_PID" "$PF_TWO_PID" >/dev/null 2>&1 || true
wait "$PF_ONE_PID" 2>/dev/null || true
wait "$PF_TWO_PID" 2>/dev/null || true
PF_ONE_PID=""
PF_TWO_PID=""
# Failure is safe: executor is explicitly DISABLED, so no writer is evicted.
OLD="${PODS[0]}"
kubectl -n "$NS" delete pod "$OLD" --wait=false
kubectl -n "$NS" wait --for=delete "pod/$OLD" --timeout=180s
kubectl -n "$NS" rollout status deployment/mb-ci-server --timeout=360s
check_pods
mapfile -t NEW_PODS < <(kubectl -n "$NS" get pods -l app=mb-ci-server -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')
[[ ! " ${NEW_PODS[*]} " =~ " $OLD " ]] || { echo "Terminated pod still selected" >&2; exit 1; }
for pod in "${NEW_PODS[@]}"; do check_db_replica "$pod"; done
# kubectl port-forward to a Service binds a *single chosen backing pod*.
# It may die when that pod is replaced: establish a NEW connection to prove
# the new Service endpoints are routable, rather than reusing a stale tunnel.
kill "$PF_PID" >/dev/null 2>&1 || true
wait "$PF_PID" 2>/dev/null || true
PF_PID=""
kubectl -n "$NS" port-forward service/mb-ci-server 18080:8080 > "$TMP/port-forward-after.log" 2>&1 &
PF_PID="$!"
ready=0
for i in {1..45}; do
  if curl -fsS --max-time 2 http://127.0.0.1:18080/healthz >/dev/null 2>&1; then ready=1; break; fi
  sleep 2
done
[[ "$ready" == 1 ]] || { cat "$TMP/port-forward-after.log" >&2; exit 1; }
test "$(http_code http://127.0.0.1:18080/healthz)" = 200
echo "Multi-node pod replacement, service recovery, PostgreSQL registration: PASS"
