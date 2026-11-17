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
  kubectl -n rook-ceph logs deploy/rook-ceph-operator --tail=180 2>&1 || true
  kubectl -n rook-ceph get cephcluster mb-ceph -o yaml 2>&1 | tail -170 || true
  kubectl -n rook-ceph get pods -o wide 2>&1 || true
  # Ceph must diagnose actual MON and OSD failures, not just a generic timeout.
  for selector in 'app=rook-ceph-mon' 'app=rook-ceph-osd' 'app=rook-ceph-osd-prepare'; do
    kubectl -n rook-ceph get pods -l "$selector" --no-headers 2>&1 || true
    for pod in $(kubectl -n rook-ceph get pods -l "$selector" -o name 2>/dev/null | head -5); do
      echo "::group::Diagnostic log $pod" >&2
      kubectl -n rook-ceph logs "$pod" --all-containers=true --tail=90 2>&1 || true
      kubectl -n rook-ceph describe "$pod" 2>&1 | tail -100 || true
      echo "::endgroup::" >&2
    done
  done
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
    extraMounts:
      - hostPath: /run/udev
        containerPath: /run/udev
        readOnly: true
  - role: worker
    extraMounts:
      - hostPath: /run/udev
        containerPath: /run/udev
        readOnly: true
  - role: worker
    extraMounts:
      - hostPath: /run/udev
        containerPath: /run/udev
        readOnly: true
EOF
# Rook/ceph-volume 20.x require host udev data for loop-device inventory.
# A kind node without the runner's /run/udev cannot prepare a real OSD.
test -d /run/udev/data || { echo "FAIL: runner has no live udev database" >&2; exit 3; }
command -v udevadm >/dev/null
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

# Rebuild udev observations for ONLY the ephemeral runner's loop devices.
# ceph-volume enumerates ALL loop sysfs devices, not only the requested one.
# The trigger is restricted to the loop subsystem; never trigger host disks.
sudo udevadm trigger --action=change --subsystem-match=block --sysname-match='loop*'
sudo udevadm settle --timeout=40
for i in 0 1 2; do
  device="${OSD_NAMES[$i]}"
  node="${WORKERS[$i]}"
  number="$(docker exec "$node" cat "/sys/block/$device/dev")"
  case "$number" in 7:*) ;; *) echo "FAIL: unsafe non-loop OSD major/minor" >&2; exit 1;; esac
  if ! test -s "/run/udev/data/b$number" ||
     ! docker exec "$node" test -s "/run/udev/data/b$number"; then
    echo "FAIL: $node $device has no authentic host udev database (b$number)" >&2
    exit 1
  fi
done
echo "PASS: three disposable OSD loop devices have real udev state inside kind"

UPSTREAM="https://raw.githubusercontent.com/rook/rook/$ROOK_VERSION/deploy/examples"
for name in crds common operator; do
  curl --fail --location --retry 4 --silent --show-error "$UPSTREAM/$name.yaml" -o "$TEMP/$name.yaml"
done
# Object-store-only Rook v1.20.x does NOT need Ceph CSI resources.
# operator.yaml contains optional OperatorConfig/Driver documents that require
# separately installed csi.ceph.io CRDs. Strip only these optional documents;
# preserve ConfigMap and actual rook-ceph-operator Deployment.
python3 - "$TEMP/operator.yaml" <<'PY'
from pathlib import Path
import sys
path=Path(sys.argv[1])
raw=path.read_text()
start=raw.index("# CSI operator resources managed by the admin.")
end=raw.index("# OLM: BEGIN OPERATOR DEPLOYMENT",start)
clean=raw[:start]+raw[end:]
clean=clean.replace('ROOK_CEPH_ALLOW_LOOP_DEVICES: "false"',
                    'ROOK_CEPH_ALLOW_LOOP_DEVICES: "true"')
clean=clean.replace('ROOK_CREATE_CSI_OPERATOR_RESOURCES: "true"',
                    'ROOK_CREATE_CSI_OPERATOR_RESOURCES: "false"')
if "kind: Deployment" not in clean or "kind: OperatorConfig" in clean or "kind: Driver" in clean:
    raise SystemExit("FAIL: Rook object-store-only manifest transformation")
if 'ROOK_CEPH_ALLOW_LOOP_DEVICES: "true"' not in clean or 'ROOK_CREATE_CSI_OPERATOR_RESOURCES: "false"' not in clean:
    raise SystemExit("FAIL: Rook options not configured safely")
path.write_text(clean)
PY
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
cluster_errors=0
crush_fixed=0
osd_pod_fixed=0
for attempt in $(seq 1 55); do
  if kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph status -f json \
      > "$TEMP/ceph-status.json" 2>/dev/null &&
     kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph osd tree -f json \
      > "$TEMP/osd-tree.json" 2>/dev/null &&
     python3 "$ROOT/scripts/ci/ceph-assert-cluster.py" "$TEMP/ceph-status.json" "$TEMP/osd-tree.json" \
      >/dev/null 2>&1; then ready=1;break;fi
  # A shared host kernel exposes disposable loop devices to every kind
  # worker. Rook may schedule an OSD onto the WRONG virtual worker despite
  # using the correct device. Repair Kubernetes *pod placement first*,
  # validated against real daemon IDs, unique dedicated loop paths and
  # the explicitly allocated worker/device bindings. Never change a real
  # storage cluster; ensure this is our disposable kind context.
  if [ "$osd_pod_fixed" -eq 0 ] && [ "$attempt" -ge 8 ] &&
     [ -s "$TEMP/ceph-status.json" ] && [ -s "$TEMP/osd-tree.json" ];then
    if kubectl -n rook-ceph get pods -l app=rook-ceph-osd -o json            > "$TEMP/osd-pods.json" 2>/dev/null &&
       python3 "$ROOT/scripts/ci/ceph-osd-node-align.py"            "$TEMP/ceph-status.json" "$TEMP/osd-tree.json" "$TEMP/osd-pods.json"            "${WORKERS[0]}:${OSD_NAMES[0]}"            "${WORKERS[1]}:${OSD_NAMES[1]}"            "${WORKERS[2]}:${OSD_NAMES[2]}"            > "$TEMP/pod-node-moves" 2> "$TEMP/pod-node-guard.err";then
      osd_pod_fixed=1
      if [ -s "$TEMP/pod-node-moves" ];then
        test "$(kubectl config current-context)" = "kind-$NAME"
        echo "Fixing real OSD pod placement to its verified disposable loop device's owning kind worker"
        while read -r osdid worker;do
          # Strategic merge preserves command, securityContext and unrelated
          # env. Patching nodeSelector ALONE leaves stale daemon CRUSH host.
          kubectl -n rook-ceph patch deployment "rook-ceph-osd-$osdid" \
            --type=strategic \
            -p "{\"spec\":{\"template\":{\"spec\":{\"nodeSelector\":{\"kubernetes.io/hostname\":\"$worker\"},\"containers\":[{\"name\":\"osd\",\"env\":[{\"name\":\"ROOK_CRUSHMAP_HOSTNAME\",\"value\":\"$worker\"}]}]}}}}"
          kubectl -n rook-ceph rollout status deployment/"rook-ceph-osd-$osdid" --timeout=150s
          observed_node="$(kubectl -n rook-ceph get deployment "rook-ceph-osd-$osdid" -o json |
            python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["spec"]["template"]["spec"]["nodeSelector"].get("kubernetes.io/hostname",""))')"
          observed_host="$(kubectl -n rook-ceph get deployment "rook-ceph-osd-$osdid" -o json |
            python3 -c 'import json,sys; d=json.load(sys.stdin); cs=d["spec"]["template"]["spec"]["containers"]; osds=[c for c in cs if c["name"]=="osd"]; assert len(osds)==1; print(next((x["value"] for x in osds[0]["env"] if x["name"]=="ROOK_CRUSHMAP_HOSTNAME"),""))')"
          if [ "$observed_node" != "$worker" ] || [ "$observed_host" != "$worker" ];then
            echo "FAIL OSD Deployment CRUSH hostname or worker reset during rollout" >&2
            exit 1
          fi
        done < "$TEMP/pod-node-moves"
        sleep 8
        continue
      fi
    fi
  fi
  # Work around a Ceph kind auto-placement defect ONLY after actual
  # three-node running OSD/device identity is independently verified.
  # Never change CRUSH on untrusted operator clusters or emulate OSDs.
  if [ "$crush_fixed" -eq 0 ] && [ "$attempt" -ge 8 ] &&
     [ -s "$TEMP/ceph-status.json" ] && [ -s "$TEMP/osd-tree.json" ];then
    if kubectl -n rook-ceph get pods -l app=rook-ceph-osd -o json            > "$TEMP/osd-pods.json" 2>/dev/null &&
       python3 "$ROOT/scripts/ci/ceph-osd-crush-align.py"            "$TEMP/ceph-status.json" "$TEMP/osd-tree.json" "$TEMP/osd-pods.json"            "${WORKERS[0]}:${OSD_NAMES[0]}"            "${WORKERS[1]}:${OSD_NAMES[1]}"            "${WORKERS[2]}:${OSD_NAMES[2]}"            > "$TEMP/crush-verified-moves" 2> "$TEMP/crush-guard.err";then
      if [ -s "$TEMP/crush-verified-moves" ];then
        echo "Reconcile actual CRUSH logical hosts with independently proven kind worker/device identities"
        while read -r osdid node;do
          kubectl -n rook-ceph exec deploy/rook-ceph-tools --              ceph osd crush move "osd.$osdid" "root=default" "host=$node"
        done < "$TEMP/crush-verified-moves"
      fi
      crush_fixed=1
    fi
  fi
  # Stop early if operator has persistently rejected an invalid cluster config.
  # This avoids burning CI minutes on a deployment that cannot progress.
  state="$(kubectl -n rook-ceph get cephcluster mb-ceph -o jsonpath='{.status.state}' 2>/dev/null || true)"
  message="$(kubectl -n rook-ceph get cephcluster mb-ceph -o jsonpath='{.status.message}' 2>/dev/null || true)"
  if [ "$state" = Error ]; then
    cluster_errors=$((cluster_errors + 1))
    if [ "$cluster_errors" -ge 3 ]; then
      echo "FAIL: Rook CephCluster reports a persistent configuration error: $message" >&2
      on_error
      exit 1
    fi
  else
    cluster_errors=0
  fi
  sleep 8
done
if [ "$ready" -ne 1 ]; then
  echo "FAIL: genuine Ceph 3-MON 3-OSD quorum never reached after at most 440s" >&2
  if [ -s "$TEMP/ceph-status.json" ]; then
    echo "::group::Ceph final status JSON" >&2
    python3 -m json.tool "$TEMP/ceph-status.json" | head -100 >&2 || true
    echo "::endgroup::" >&2
  fi
  [ ! -s "$TEMP/osd-tree.json" ] || python3 -m json.tool "$TEMP/osd-tree.json" >&2 || true
  [ ! -s "$TEMP/pod-node-guard.err" ] || cat "$TEMP/pod-node-guard.err" >&2 || true
  [ ! -s "$TEMP/crush-guard.err" ] || cat "$TEMP/crush-guard.err" >&2 || true
  on_error
  exit 1
fi
python3 "$ROOT/scripts/ci/ceph-assert-cluster.py" "$TEMP/ceph-status.json" "$TEMP/osd-tree.json"
echo "PASS: three distinct virtual kind-worker Ceph OSD/CRUSH hosts (not physical fault domains)"

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

if [ "${MB_CEPHFS_LAB:-0}" = 1 ]; then
  # Reuse THIS exact 3-MON/3-OSD disposable kind Rook/Ceph lab instead of
  # creating a second mock lab or spending twice the CI time.
  bash "$ROOT/scripts/ci/cephfs-kind-real-r182.sh" "$TEMP"
fi

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
