#!/usr/bin/env bash
# Real 3-worker Rook/Ceph RADOS Gateway CI. No simulation, no external cluster,
# no use of the runner's existing block disks: only loop devices backed by
# sparse files created INSIDE short-lived kind worker containers.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != "true" ] || [ "${RUNNER_ENVIRONMENT:-}" != "github-hosted" ]; then
  echo "REFUSED: this destructive test is restricted to isolated GitHub-hosted runners" >&2
  exit 3
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NAME=mb-ceph-rgw
ROOK_VERSION=v1.20.9
CEPH_IMAGE=quay.io/ceph/ceph:v20.2.4
TEMP="$(mktemp -d)"
PORT_PID=""
cleanup(){
  set +e
  if [ -n "$PORT_PID" ]; then kill "$PORT_PID" 2>/dev/null || true; wait "$PORT_PID" 2>/dev/null || true; fi
  kind delete cluster --name "$NAME" >/dev/null 2>&1 || true
  rm -rf -- "$TEMP"
}
trap cleanup EXIT
on_error(){
  echo "::error::Real Ceph RGW qualification failed (no PASS or release)." >&2
  kubectl -n rook-ceph get cephcluster,cephobjectstore,cephobjectstoreuser,pods,deployments,svc -o wide 2>&1 || true
  kubectl -n rook-ceph get events --sort-by=.lastTimestamp 2>&1 | tail -50 || true
  kubectl -n rook-ceph logs deploy/rook-ceph-operator --tail=100 2>&1 || true
  kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph -s 2>&1 || true
}
trap on_error ERR
for cmd in kind kubectl docker aws age-keygen python3; do command -v "$cmd" >/dev/null; done
test -x "$ROOT/rust/target/debug/multi-backup-native"

cat > "$TEMP/kind.yaml" <<'EOF'
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
  - role: control-plane
  - role: worker
  - role: worker
  - role: worker
EOF
kind create cluster --name "$NAME" --config "$TEMP/kind.yaml" --wait 240s
kubectl wait --for=condition=Ready node --all --timeout=180s
test "$(kubectl get nodes -o json | python3 -c 'import json,sys;print(len(json.load(sys.stdin)["items"]))')" -eq 4

# Create one *new* sparse file / dedicated loop device per disposable worker.
# Each container sees its own /dev; never select a runner host disk or preexisting
# disk name. osd_map is constructed only from the devices we created.
mapfile -t WORKERS < <(kind get nodes --name "$NAME" | grep -- '-worker' | sort)
test "${#WORKERS[@]}" -eq 3
OSD_NAMES=()
for node in "${WORKERS[@]}"; do
  dev="$(docker exec "$node" bash -euc '
    mkdir -p /var/lib/mb-ceph-test
    truncate -s 7G /var/lib/mb-ceph-test/osd.img
    if [ ! -c /dev/loop-control ]; then mknod /dev/loop-control c 10 237; fi
    for n in $(seq 0 127); do
      [ -b "/dev/loop$n" ] || mknod "/dev/loop$n" b 7 "$n"
    done
    losetup --find --show /var/lib/mb-ceph-test/osd.img
  ')"
  case "$dev" in /dev/loop[0-9]*) ;; *) echo "Unsafe OSD test device: $dev" >&2; exit 1;; esac
  echo "Dedicated disposable OSD volume: $node $dev"
  OSD_NAMES+=("${dev##*/}")
done
test "${#OSD_NAMES[@]}" -eq 3
test "${OSD_NAMES[0]}" != "${OSD_NAMES[1]}"
test "${OSD_NAMES[0]}" != "${OSD_NAMES[2]}"
test "${OSD_NAMES[1]}" != "${OSD_NAMES[2]}"

UPSTREAM="https://raw.githubusercontent.com/rook/rook/$ROOK_VERSION/deploy/examples"
for name in crds common operator; do
  curl --fail --location --retry 4 --silent --show-error "$UPSTREAM/$name.yaml" -o "$TEMP/$name.yaml"
done
# Only the operator of the disposable test cluster may use loop devices.
sed -i 's/ROOK_CEPH_ALLOW_LOOP_DEVICES: "false"/ROOK_CEPH_ALLOW_LOOP_DEVICES: "true"/' "$TEMP/operator.yaml"
grep -q 'ROOK_CEPH_ALLOW_LOOP_DEVICES: "true"' "$TEMP/operator.yaml"
kubectl apply -f "$TEMP/crds.yaml" -f "$TEMP/common.yaml"
kubectl apply -f "$TEMP/operator.yaml"
kubectl -n rook-ceph rollout status deploy/rook-ceph-operator --timeout=360s

# RADOS is genuine: 3 mons on distinct nodes, 3 OSD BlueStore devices, 1 mgr.
# Never use useAllDevices: true or a pattern that can consume host disks.
cat > "$TEMP/cluster.yaml" <<EOF
apiVersion: ceph.rook.io/v1
kind: CephCluster
metadata:
  name: mb-ceph
  namespace: rook-ceph
spec:
  dataDirHostPath: /var/lib/rook-mb-ci
  cephVersion:
    image: $CEPH_IMAGE
    allowUnsupported: false
  mon:
    count: 3
    allowMultiplePerNode: false
  mgr:
    count: 1
    allowMultiplePerNode: false
    modules:
      - name: rook
        enabled: false
  dashboard:
    enabled: false
  crashCollector:
    disable: true
  skipUpgradeChecks: true
  storage:
    useAllNodes: false
    useAllDevices: false
    allowDeviceClassUpdate: false
    nodes:
      - name: "${WORKERS[0]}"
        devices:
          - name: "${OSD_NAMES[0]}"
      - name: "${WORKERS[1]}"
        devices:
          - name: "${OSD_NAMES[1]}"
      - name: "${WORKERS[2]}"
        devices:
          - name: "${OSD_NAMES[2]}"
  cephConfig:
    global:
      osd_pool_default_size: "3"
      osd_memory_target: "805306368"
  monitoring:
    enabled: false
---
apiVersion: ceph.rook.io/v1
kind: CephObjectStore
metadata:
  name: mb-ci-store
  namespace: rook-ceph
spec:
  metadataPool:
    replicated:
      size: 3
  dataPool:
    replicated:
      size: 3
  preservePoolsOnDelete: false
  gateway:
    port: 80
    instances: 1
---
apiVersion: ceph.rook.io/v1
kind: CephObjectStoreUser
metadata:
  name: mb-ci-user
  namespace: rook-ceph
spec:
  store: mb-ci-store
  displayName: "CI disposable RGW S3 user"
EOF
kubectl apply -f "$TEMP/cluster.yaml"

curl --fail --location --retry 4 --silent --show-error \
  "$UPSTREAM/toolbox.yaml" -o "$TEMP/toolbox.yaml"
kubectl apply -f "$TEMP/toolbox.yaml"
kubectl -n rook-ceph rollout status deploy/rook-ceph-tools --timeout=900s

# Poll Ceph itself, not just pod readiness. Require 3 MON quorum, 3 UP+IN
# OSD and three distinct CRUSH hosts. No MOCK/skip/single-OSD fallback.
ready=0
for attempt in $(seq 1 90); do
  if kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph status -f json \
      > "$TEMP/ceph-status.json" 2>/dev/null &&
     kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph osd tree -f json \
      > "$TEMP/osd-tree.json" 2>/dev/null &&
     python3 "$ROOT/scripts/ci/ceph-assert-cluster.py" "$TEMP/ceph-status.json" "$TEMP/osd-tree.json" \
      >/dev/null 2>&1; then ready=1;break;fi
  sleep 10
done
if [ "$ready" -ne 1 ]; then
  echo "FAIL: genuine Ceph 3-MON 3-OSD quorum never reached" >&2
  exit 1
fi
python3 "$ROOT/scripts/ci/ceph-assert-cluster.py" "$TEMP/ceph-status.json" "$TEMP/osd-tree.json"
echo "PASS: real Ceph MON quorum and 3 physically distinct kind-node OSDs"

kubectl -n rook-ceph wait --for=condition=Ready pod -l app=rook-ceph-rgw --timeout=900s
SECRET=rook-ceph-object-user-mb-ci-store-mb-ci-user
for n in $(seq 1 60);do
  if kubectl -n rook-ceph get secret "$SECRET" >/dev/null 2>&1;then break;fi
  sleep 5
done
export AWS_ACCESS_KEY_ID
export AWS_SECRET_ACCESS_KEY
AWS_ACCESS_KEY_ID="$(kubectl -n rook-ceph get secret "$SECRET" -o jsonpath='{.data.AccessKey}' | base64 -d)"
AWS_SECRET_ACCESS_KEY="$(kubectl -n rook-ceph get secret "$SECRET" -o jsonpath='{.data.SecretKey}' | base64 -d)"
test -n "$AWS_ACCESS_KEY_ID" && test -n "$AWS_SECRET_ACCESS_KEY"
export AWS_DEFAULT_REGION=us-east-1 AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
kubectl -n rook-ceph port-forward service/rook-ceph-rgw-mb-ci-store 9000:80 \
  --address 127.0.0.1 > "$TEMP/port-forward.log" 2>&1 &
PORT_PID=$!
success=0
for n in $(seq 1 60);do
  if aws --endpoint-url http://127.0.0.1:9000 s3api list-buckets >/dev/null 2>&1; then
    success=1;break
  fi
  if ! kill -0 "$PORT_PID" 2>/dev/null; then break; fi
  sleep 2
done
if [ "$success" -ne 1 ]; then
 echo "FAIL: genuine Ceph RGW endpoint unavailable" >&2
 tail -25 "$TEMP/port-forward.log" >&2
 exit 1
fi
export CEPH_REAL_CLUSTER_ATTESTED=true
bash "$ROOT/scripts/ci/ceph-rgw-s3-object-lock-acceptance.sh" "$TEMP"
echo "PASS: real multi-node Ceph RGW S3 native Rust WORM acceptance"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Genuine 3-node Ceph RGW S3 acceptance"
    echo "- Rook $ROOK_VERSION, Ceph $CEPH_IMAGE"
    echo "- Ceph MON quorum: 3; OSD UP+IN on different workers: 3"
    echo "- Real RGW S3 Object Lock COMPLIANCE, encrypted Rust backup/verify/restore: PASS"
    echo "- Deletion and retention-shortening: rejected by Ceph RGW, control delete: succeeded"
    echo "- This disposable CI does not certify physical-admin or host-root resistance"
  } >> "$GITHUB_STEP_SUMMARY"
fi
