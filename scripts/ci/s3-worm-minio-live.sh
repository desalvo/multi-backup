#!/usr/bin/env bash
# REAL MinIO Community Server: build from pinned upstream source, not removed images.
# Self-contained ephemeral test on a GitHub hosted runner only; never touch real buckets.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${RUNNER_ENVIRONMENT:-}" != github-hosted ]; then
  echo "Refusing to execute S3 destructive negative tests outside GitHub hosted CI" >&2
  exit 3
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
command -v go >/dev/null
command -v aws >/dev/null
command -v age-keygen >/dev/null
TMP="$(mktemp -d)"
SERVER_PID=""
cleanup() {
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || :; wait "$SERVER_PID" 2>/dev/null || :
  fi
  rm -rf -- "$TMP"
}
trap cleanup EXIT
export GOBIN="$TMP/bin"
mkdir -p "$GOBIN" "$TMP/disk1" "$TMP/disk2" "$TMP/disk3" "$TMP/disk4"
echo "Building MinIO from pinned official open-source release"
go install github.com/minio/minio@RELEASE.2025-10-15T17-29-55Z
test -x "$GOBIN/minio"
# CI credentials are short-lived per job; not set in source code or stored in backups.
export AWS_ACCESS_KEY_ID="ci-r047-access-$$"
export AWS_SECRET_ACCESS_KEY="$(openssl rand -hex 24)"
export AWS_DEFAULT_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
MINIO_ROOT_USER="$AWS_ACCESS_KEY_ID" MINIO_ROOT_PASSWORD="$AWS_SECRET_ACCESS_KEY" \
  "$GOBIN/minio" server "$TMP/disk1" "$TMP/disk2" "$TMP/disk3" "$TMP/disk4" \
  --address 127.0.0.1:9000 --console-address 127.0.0.1:9001 \
  > "$TMP/minio.log" 2>&1 &
SERVER_PID=$!
s3() { aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"; }
ready=0
for n in $(seq 1 60); do
  if s3 list-buckets >/dev/null 2>&1; then ready=1; break; fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then break; fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "FAIL: actual MinIO server did not become available" >&2
  sed -n '1,90p' "$TMP/minio.log" >&2
  exit 1
fi
echo "PASS: official MinIO source binary started"

# Both buckets are created INSIDE this temporary MinIO instance only.
s3 create-bucket --bucket vault-worm --object-lock-enabled-for-bucket >/dev/null
s3 put-bucket-versioning --bucket vault-worm --versioning-configuration Status=Enabled >/dev/null
s3 put-object-lock-configuration --bucket vault-worm \
  --object-lock-configuration '{"ObjectLockEnabled":"Enabled","Rule":{"DefaultRetention":{"Mode":"COMPLIANCE","Days":30}}}' >/dev/null
s3 create-bucket --bucket vault-no-lock >/dev/null
# A genuine ability to delete from an unlocked control bucket rules out
# an unavailable S3 endpoint or missing administrative permissions.
echo 'delete control' > "$TMP/control.txt"
s3 put-object --bucket vault-no-lock --key disposable --body "$TMP/control.txt" >/dev/null
s3 delete-object --bucket vault-no-lock --key disposable >/dev/null
if s3 head-object --bucket vault-no-lock --key disposable >/dev/null 2>&1; then
  echo "FAIL: ordinary control object was not deleted" >&2; exit 1
fi
echo "PASS: admin delete control against an unlocked test bucket"

age-keygen -o "$TMP/identity" >/dev/null 2>&1
age-keygen -o "$TMP/wrong-identity" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TMP/identity")"
export RECIPIENT TMP
python3 - <<'PY'
import json,os
base={"provider":"minio","endpoint":"http://localhost:9000","region":"us-east-1",
      "bucket":"vault-worm","prefix":"ci/test-r047","recipient":os.environ["RECIPIENT"],
      "retention_days":2,"allow_loopback_http_for_testing":True}
with open(os.path.join(os.environ["TMP"],"s3.json"),"w",encoding="utf8") as f: json.dump(base,f)
base["bucket"]="vault-no-lock"
with open(os.path.join(os.environ["TMP"],"unsafe.json"),"w",encoding="utf8") as f: json.dump(base,f)
PY

"$NATIVE" s3-vault preflight --config "$TMP/s3.json"
if "$NATIVE" s3-vault preflight --config "$TMP/unsafe.json"; then
  echo "FAIL: S3 vault accepted bucket without Object Lock" >&2; exit 1
fi
mkdir -p "$TMP/source"
printf 'immutable S3 vault sample\n' > "$TMP/source/hello.txt"
"$NATIVE" s3-vault backup --config "$TMP/s3.json" --source "$TMP/source" > "$TMP/result.json"
ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/result.json")"
VER="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["archive_version"])' "$TMP/result.json")"
"$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/identity"
if "$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/wrong-identity"; then
  echo "FAIL: bad age identity passed verification" >&2; exit 1
fi
"$NATIVE" s3-vault restore --config "$TMP/s3.json" --id "$ID" \
  --identity "$TMP/identity" --destination "$TMP/restored"
test "$(find "$TMP/restored" -type f | wc -l)" -eq 1
grep -Rq 'immutable S3 vault sample' "$TMP/restored"
if "$NATIVE" s3-vault restore --config "$TMP/s3.json" --id "$ID" \
  --identity "$TMP/identity" --destination "$TMP/restored"; then
  echo "FAIL: S3 restore overwrote an existing directory" >&2; exit 1
fi
# Version-specific deletion MUST be denied by MinIO, including the root user.
if s3 delete-object --bucket vault-worm --key "ci/test-r047/snapshots/$ID.tar.gz.age" \
    --version-id "$VER" >/dev/null 2>&1; then
  echo "FAIL: COMPLIANCE-retained archive version was deleted" >&2; exit 1
fi
"$NATIVE" s3-vault verify --config "$TMP/s3.json" --id "$ID" --identity "$TMP/identity"
# r178 independent REAL MinIO B-In versioned object import: immutable
# versionId + caller-provided raw SHA256, verified age restore into a new path.
INPUT_OBJECT=ci/test-r047/source-20261116.txt
printf 'MinIO original versioned S3 source\n' > "$TMP/input-old.txt"
OLD="$(s3 put-object --bucket vault-worm --key "$INPUT_OBJECT" \
  --body "$TMP/input-old.txt" --query VersionId --output text)"
test -n "$OLD" && test "$OLD" != null && test "$OLD" != None
printf 'MinIO newer mutable current source\n' > "$TMP/input-new.txt"
s3 put-object --bucket vault-worm --key "$INPUT_OBJECT" \
  --body "$TMP/input-new.txt" >/dev/null
HASH="$(sha256sum "$TMP/input-old.txt" | cut -d' ' -f1)"
mkdir -p "$TMP/import-store"
"$NATIVE" storage init --root "$TMP/import-store" --recipient "$RECIPIENT"
"$NATIVE" plugins s3-object-backup --config "$TMP/s3.json" \
 --object-key source-20261116.txt --version-id "$OLD" --sha256 "$HASH" \
 --root "$TMP/import-store" --identity "$TMP/identity" \
 --label "MinIO older exact source version" --json > "$TMP/pinned-input.json"
SNAPSHOT="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/pinned-input.json")"
"$NATIVE" storage restore --root "$TMP/import-store" --identity "$TMP/identity" \
  --id "$SNAPSHOT" --destination "$TMP/input-restored"
cmp "$TMP/input-old.txt" "$TMP/input-restored/s3/vault-worm/$INPUT_OBJECT"
if "$NATIVE" plugins s3-object-backup --config "$TMP/s3.json" \
   --object-key source-20261116.txt --version-id "$OLD" \
   --sha256 "$(printf '0%.0s' {1..64})" --root "$TMP/import-store" \
   --identity "$TMP/identity" --label "Expect rejection" >/dev/null 2>&1;then
  echo "FAIL: MinIO S3 B-In accepted incorrect independently pinned SHA" >&2
  exit 1
fi
echo "PASS: real MinIO S3 B-In old-version snapshot, encrypted chunk restore, wrong-hash denial"
echo "PASS: real MinIO source build, Object Lock COMPLIANCE negative delete, encrypted backup/verify/restore"
