#!/usr/bin/env bash
# Real MinIO WORM wire-protocol acceptance. Never uses production buckets.
set -Eeuo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
TMP="$(mktemp -d)"
CONTAINER="multi-backup-worm-$$"
MC_IMAGE="${MC_IMAGE:-quay.io/minio/mc:latest}"
MINIO_IMAGE="${MINIO_IMAGE:-quay.io/minio/minio:latest}"
cleanup() { docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$TMP"; }
trap cleanup EXIT

: "${AWS_ACCESS_KEY_ID:=ci-vault-access}"
: "${AWS_SECRET_ACCESS_KEY:=ci-vault-strong-secret-not-production}"
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

docker run -d --rm --name "$CONTAINER" \
    -p 127.0.0.1:9000:9000 \
    -e MINIO_ROOT_USER="$AWS_ACCESS_KEY_ID" \
    -e MINIO_ROOT_PASSWORD="$AWS_SECRET_ACCESS_KEY" \
    "$MINIO_IMAGE" server /data >/dev/null
mc() {
  docker run --rm --network host --entrypoint /bin/sh \
    -e "MINIO_USER=$AWS_ACCESS_KEY_ID" \
    -e "MINIO_PASSWORD=$AWS_SECRET_ACCESS_KEY" "$MC_IMAGE" \
    -c 'mc alias set lab http://127.0.0.1:9000 "$MINIO_USER" "$MINIO_PASSWORD" >/dev/null && mc "$@"' -- "$@"
}
for i in $(seq 1 45); do
    if mc ls lab >/dev/null 2>&1; then break; fi
    sleep 1
done
mc mb --with-lock lab/vault-worm
mc retention set --default COMPLIANCE 30d lab/vault-worm
mc mb lab/vault-no-lock
age-keygen -o "$TMP/identity" >/dev/null 2>&1
age-keygen -o "$TMP/wrong-identity" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TMP/identity")"
export RECIPIENT TMP
python3 - <<'PY'
import json,os
base={"provider":"minio","endpoint":"http://localhost:9000","region":"us-east-1",
      "bucket":"vault-worm","prefix":"ci/test-r047","recipient":os.environ["RECIPIENT"],
      "retention_days":2,"allow_loopback_http_for_testing":True}
with open(os.path.join(os.environ["TMP"],"s3.json"),"w",encoding="utf-8") as f: json.dump(base,f)
base["bucket"]="vault-no-lock"
with open(os.path.join(os.environ["TMP"],"unsafe.json"),"w",encoding="utf-8") as f: json.dump(base,f)
PY
"$NATIVE" s3-vault preflight --config "$TMP/s3.json"
if "$NATIVE" s3-vault preflight --config "$TMP/unsafe.json"; then
    echo "FAIL: S3 vault accepted bucket without Object Lock" >&2; exit 1
fi
mkdir "$TMP/source"
printf 'immutable S3 vault sample\n' > "$TMP/source/hello.txt"
"$NATIVE" s3-vault backup --config "$TMP/s3.json" --source "$TMP/source" > "$TMP/result.json"
ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/result.json")"
VER="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["archive_version"])' "$TMP/result.json")"
"$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/identity"
if "$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/wrong-identity"; then
    echo "FAIL: incorrect identity passed verification" >&2; exit 1
fi
"$NATIVE" s3-vault restore --config "$TMP/s3.json" --id "$ID" \
    --identity "$TMP/identity" --destination "$TMP/restored"
test "$(find "$TMP/restored" -type f | wc -l)" -eq 1
grep -Rq 'immutable S3 vault sample' "$TMP/restored"
if "$NATIVE" s3-vault restore --config "$TMP/s3.json" --id "$ID" \
    --identity "$TMP/identity" --destination "$TMP/restored"; then
    echo "FAIL: restore overwrote existing path" >&2; exit 1
fi
# Privileged root must NOT be able to delete the exact retained object version.
if mc rm --version-id "$VER" "lab/vault-worm/ci/test-r047/snapshots/$ID.tar.gz.age"; then
    echo "FAIL: protected COMPLIANCE version was deleted" >&2; exit 1
fi
"$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/identity"
echo "PASS: MinIO real S3 backup/verify/restore + locked-version delete rejection"
