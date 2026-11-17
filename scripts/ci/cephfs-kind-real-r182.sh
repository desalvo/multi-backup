#!/usr/bin/env bash
# Real CephFS in an ephemeral 3 MON/3 OSD Rook kind cluster, never a mock.
set -Eeuo pipefail
umask 077
test "$GITHUB_ACTIONS" = true && test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(kubectl config current-context)" = kind-mb-ceph-rgw
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TEMP="$1"
test -d "$TEMP" && test -x "$ROOT/rust/target/debug/multi-backup-native"
kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph -s
cat > "$TEMP/cephfs.yaml" <<'YAML'
apiVersion: ceph.rook.io/v1
kind: CephFilesystem
metadata: { name: mb-ci-fs, namespace: rook-ceph }
spec:
  metadataPool:
    replicated: { size: 3 }
  dataPools:
    - name: data0
      replicated: { size: 3 }
  preservePoolsOnDelete: false
  metadataServer:
    activeCount: 1
    activeStandby: true
YAML
kubectl apply -f "$TEMP/cephfs.yaml"
READY=0
for n in $(seq 1 75);do
  if kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph fs status mb-ci-fs -f json \
      >"$TEMP/fs-status.json" 2>/dev/null &&
    python3 - "$TEMP/fs-status.json" <<'PY'
import json,sys
data=json.load(open(sys.argv[1]))
# Ceph "fs status -f json" is NOT stable as a dict across releases:
# Ceph Tentacle 20 emits mdsmap as a LIST. Recurse through objects,
# requiring an actual active MDS entry, not just a Filesystem CRD.
def active_mds(item):
    if isinstance(item,dict):
        state=item.get("state")
        if state in ("active","up:active","up:active (resolve)"):
            return True
        return any(active_mds(v) for v in item.values())
    if isinstance(item,list):
        return any(active_mds(v) for v in item)
    return False
if not active_mds(data):
    raise SystemExit(1)
PY
  then READY=1;break;fi
  sleep 8
done
test "$READY" = 1 || { echo 'FAIL: CephFS never got a real active MDS' >&2; exit 1; }
kubectl -n rook-ceph exec deploy/rook-ceph-tools -- ceph fs status mb-ci-fs
cat > "$TEMP/client.yaml" <<'YAML'
apiVersion: v1
kind: Pod
metadata:
  name: mb-ci-cephfs-client
  namespace: rook-ceph
  labels: { app: mb-ci-cephfs-client }
spec:
  restartPolicy: Never
  containers:
    - name: client
      # The Rook cluster is Ceph Tentacle 20.2.4 (aes256k CephX).
      # Ubuntu 24.04 ships old Ceph 19.2.3: it cannot parse Tentacle
      # keys, so a matching pinned Ceph client image is mandatory.
      image: quay.io/ceph/ceph:v20.2.4
      command: ["/bin/bash", "-lc", "sleep 1800"]
      securityContext:
        privileged: true
      resources:
        requests: { memory: 128Mi, cpu: 100m }
        limits: { memory: 2Gi }
YAML
kubectl apply -f "$TEMP/client.yaml"
kubectl -n rook-ceph wait --for=condition=Ready pod/mb-ci-cephfs-client --timeout=300s
# Install only client tools, never downgrade CephX authentication to legacy AES.
# CentOS Stream 9 Ceph image uses dnf. age is provided by EPEL 9.
kubectl -n rook-ceph exec mb-ci-cephfs-client -- bash -lc '
  set -euo pipefail
  command -v ceph
  if ! command -v ceph-fuse >/dev/null; then dnf -y install ceph-fuse; fi
  if ! command -v jq >/dev/null; then dnf -y install jq; fi
  if ! command -v age-keygen >/dev/null; then
    dnf -y install https://dl.fedoraproject.org/pub/epel/epel-release-latest-9.noarch.rpm
    dnf -y install age
  fi
  command -v ceph-fuse && command -v age-keygen && command -v jq
'
# Auth material never leaves the disposable cluster, even in logs/artifacts.
kubectl -n rook-ceph exec deploy/rook-ceph-tools -- tar -C / -cf - etc/ceph \
  | kubectl -n rook-ceph exec -i mb-ci-cephfs-client -- tar -C / -xf -
kubectl -n rook-ceph cp "$ROOT/rust/target/debug/multi-backup-native" \
  mb-ci-cephfs-client:/usr/local/bin/multi-backup-native
kubectl -n rook-ceph cp "$ROOT/tests/cephfs/rook-real-client-r182.sh" \
  mb-ci-cephfs-client:/tmp/cephfs-acceptance.sh
kubectl -n rook-ceph exec mb-ci-cephfs-client -- bash /tmp/cephfs-acceptance.sh
echo 'PASS: real 3 OSD CephFS MDS, actual client mounts, Rust encrypted full and ciphertext output'
