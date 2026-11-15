#!/usr/bin/env bash
# Actual Kubernetes API/server/etcd/CNI/scheduler on three kind nodes (two workers).
# Only stateless API + PostgreSQL TLS/session-control-plane acceptance here;
# no shared storage/physical writer fencing certification is implied.
set -euo pipefail
NS=multi-backup-ci
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
PF_PID=""
cleanup() {
  if [[ -n "$PF_PID" ]]; then kill "$PF_PID" >/dev/null 2>&1 || true; wait "$PF_PID" 2>/dev/null || true; fi
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
# This is a published, TEST-ONLY Argon2id fixture; not a real account password.
printf '%s\n' '$argon2id$v=19$m=65536,t=3,p=4$mxzRPI71Fc4HKAuFpBfrFQ$sT2wAmv5Qrsyq8bzBSgkDeGnJCUHYHE5DIhEf/0hjgA' > "$TMP/admin-hash"
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
# Failure is safe: executor is explicitly DISABLED, so no writer is evicted.
OLD="${PODS[0]}"
kubectl -n "$NS" delete pod "$OLD" --wait=false
kubectl -n "$NS" wait --for=delete "pod/$OLD" --timeout=180s
kubectl -n "$NS" rollout status deployment/mb-ci-server --timeout=360s
check_pods
mapfile -t NEW_PODS < <(kubectl -n "$NS" get pods -l app=mb-ci-server -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}')
[[ ! " ${NEW_PODS[*]} " =~ " $OLD " ]] || { echo "Terminated pod still selected" >&2; exit 1; }
for pod in "${NEW_PODS[@]}"; do check_db_replica "$pod"; done
test "$(http_code http://127.0.0.1:18080/healthz)" = 200
echo "Multi-node pod replacement, service recovery, PostgreSQL registration: PASS"
