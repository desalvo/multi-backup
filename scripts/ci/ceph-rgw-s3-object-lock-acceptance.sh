#!/usr/bin/env bash
# Actual Ceph RADOS Gateway: no mock, no MinIO, no outside cluster.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${RUNNER_ENVIRONMENT:-}" != github-hosted ] ||
   [ "${CEPH_REAL_CLUSTER_ATTESTED:-}" != true ]; then
  echo "REFUSED: Ceph S3 acceptance requires an attested disposable real cluster" >&2
  exit 3
fi
WORK="${1:?Dedicated temporary directory required}"
test -d "$WORK"
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
: "${AWS_ACCESS_KEY_ID:?Missing fresh local RGW account}"
: "${AWS_SECRET_ACCESS_KEY:?Missing fresh local RGW secret}"
export AWS_DEFAULT_REGION=us-east-1 AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
s3(){ aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"; }
test "$(s3 list-buckets --query 'length(Buckets)' --output text)" -ge 0
BUCKET=mb-ci-worm
CONTROL=mb-ci-control
s3 create-bucket --bucket "$BUCKET" --object-lock-enabled-for-bucket >/dev/null
s3 put-bucket-versioning --bucket "$BUCKET" \
  --versioning-configuration Status=Enabled >/dev/null
s3 put-object-lock-configuration --bucket "$BUCKET" \
  --object-lock-configuration '{"ObjectLockEnabled":"Enabled","Rule":{"DefaultRetention":{"Mode":"COMPLIANCE","Days":30}}}' >/dev/null
s3 create-bucket --bucket "$CONTROL" >/dev/null
# A full unsigned/keyless outage must NOT be accepted as a successful WORM test.
echo "real RGW unlocked delete-control" > "$WORK/delete-control.txt"
s3 put-object --bucket "$CONTROL" --key deletion-proof --body "$WORK/delete-control.txt" >/dev/null
s3 delete-object --bucket "$CONTROL" --key deletion-proof >/dev/null
if s3 head-object --bucket "$CONTROL" --key deletion-proof >/dev/null 2>&1; then
  echo "FAIL: control object survived normal delete; cannot certify denial" >&2
  exit 1
fi
echo "PASS: Ceph RGW control delete works using the SAME authentic user"

age-keygen -o "$WORK/key.age" >/dev/null 2>&1
age-keygen -o "$WORK/wrong.age" >/dev/null 2>&1
PUB="$(age-keygen -y "$WORK/key.age")"
export PUB WORK
python3 - <<'PY'
import json, os
config = {
    "provider": "ceph-rgw", "endpoint": "http://127.0.0.1:9000",
    "region": "us-east-1", "bucket": "mb-ci-worm",
    "prefix": "ceph/real/ci", "recipient": os.environ["PUB"],
    "retention_days": 2, "allow_loopback_http_for_testing": True
}
with open(os.path.join(os.environ["WORK"], "ceph.json"), "w", encoding="utf-8") as file:
    json.dump(config, file)
config["bucket"] = "mb-ci-control"
with open(os.path.join(os.environ["WORK"], "unlocked.json"), "w", encoding="utf-8") as file:
    json.dump(config, file)
PY
"$CLI" s3-vault preflight --config "$WORK/ceph.json"
if "$CLI" s3-vault preflight --config "$WORK/unlocked.json" >/dev/null 2>&1; then
  echo "FAIL: Rust claimed non-WORM Ceph RGW bucket was compliant" >&2; exit 1
fi
mkdir -p "$WORK/source/nested"
echo "Ceph actual multi-node encrypted backup test" > "$WORK/source/nested/example.txt"
dd if=/dev/urandom of="$WORK/source/data.bin" bs=1024 count=256 status=none
# r188: real Ceph RGW *B-In* reads a fixed object version into Rust age
# encrypted chunks, even if the current object has already changed.
S3_SOURCE_KEY=ceph/real/ci/r188/source-versioned.bin
OLD_VERSION="$(s3 put-object --bucket "$BUCKET" --key "$S3_SOURCE_KEY" \
  --body "$WORK/source/data.bin" --query VersionId --output text)"
test -n "$OLD_VERSION" && test "$OLD_VERSION" != None && test "$OLD_VERSION" != null
printf 'later Ceph RGW object version\n' > "$WORK/latest-s3-source.txt"
NEW_VERSION="$(s3 put-object --bucket "$BUCKET" --key "$S3_SOURCE_KEY" \
  --body "$WORK/latest-s3-source.txt" --query VersionId --output text)"
test -n "$NEW_VERSION" && test "$NEW_VERSION" != "$OLD_VERSION"
DIGEST="$(sha256sum "$WORK/source/data.bin" | awk '{print $1}')"
mkdir -p "$WORK/s3-import-store"
"$CLI" storage init --root "$WORK/s3-import-store" --recipient "$PUB" >/dev/null
"$CLI" plugins s3-object-backup --config "$WORK/ceph.json" \
  --object-key r188/source-versioned.bin --version-id "$OLD_VERSION" \
  --sha256 "$DIGEST" --root "$WORK/s3-import-store" \
  --identity "$WORK/key.age" --label "Real Ceph RGW pinned S3 B-In" \
  --json > "$WORK/s3-import.json"
IMPORT_ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$WORK/s3-import.json")"
"$CLI" storage verify --root "$WORK/s3-import-store" --identity "$WORK/key.age" \
  --id "$IMPORT_ID" >/dev/null
"$CLI" storage restore --root "$WORK/s3-import-store" \
  --identity "$WORK/key.age" --id "$IMPORT_ID" \
  --destination "$WORK/s3-import-recovered"
cmp "$WORK/source/data.bin" \
  "$WORK/s3-import-recovered/s3/$BUCKET/$S3_SOURCE_KEY"
if "$CLI" plugins s3-object-backup --config "$WORK/ceph.json" \
  --object-key r188/source-versioned.bin --version-id "$OLD_VERSION" \
  --sha256 "$(printf '0%.0s' {1..64})" \
  --root "$WORK/s3-import-store" --identity "$WORK/key.age" \
  --label "Wrong digest must fail" >/dev/null 2>&1;then
  echo "FAIL: genuine Ceph RGW B-In accepted wrong independent SHA256" >&2;exit 1
fi
echo "PASS: genuine Ceph RGW S3 B-In pinned older version, encrypted chunk restore and bad-hash rejection"

"$CLI" s3-vault backup --config "$WORK/ceph.json" --source "$WORK/source" > "$WORK/record.json"
read -r ID VERSION < <(python3 - "$WORK/record.json" <<'PY'
import json,sys
record=json.load(open(sys.argv[1],encoding="utf-8"))
assert len(record["id"]) == 32 and record["archive_version"]
print(record["id"],record["archive_version"])
PY
)
KEY="ceph/real/ci/snapshots/$ID.tar.gz.age"
MANIFEST="ceph/real/ci/manifests/$ID.json"
test "$VERSION" != null
s3 get-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
  > "$WORK/object-retention.json"
python3 - "$WORK/object-retention.json" <<'PY'
import datetime,json,sys
data=json.load(open(sys.argv[1],encoding="utf-8"))["Retention"]
assert data["Mode"] == "COMPLIANCE",data
expire=datetime.datetime.fromisoformat(data["RetainUntilDate"].replace("Z","+00:00"))
assert (expire-datetime.datetime.now(datetime.timezone.utc)).total_seconds()>24*3600
PY
# Require a manifest version and the lock on that version too.
MANIFEST_VER="$(s3 head-object --bucket "$BUCKET" --key "$MANIFEST" --query VersionId --output text)"
test -n "$MANIFEST_VER" && test "$MANIFEST_VER" != "None" && test "$MANIFEST_VER" != "null"
s3 get-object-retention --bucket "$BUCKET" --key "$MANIFEST" --version-id "$MANIFEST_VER" \
  --query 'Retention.Mode' --output text | grep -qx COMPLIANCE
"$CLI" s3-vault verify --config "$WORK/ceph.json" --id "$ID" --identity "$WORK/key.age"
if "$CLI" s3-vault verify --config "$WORK/ceph.json" --id "$ID" \
    --identity "$WORK/wrong.age" >/dev/null 2>&1; then
  echo "FAIL: wrong age identity verified S3 ciphertext" >&2;exit 1
fi
"$CLI" s3-vault restore --config "$WORK/ceph.json" --id "$ID" \
  --identity "$WORK/key.age" --destination "$WORK/restored"
RESTORE_FILES="$WORK/restored/multi-backup/s3-vault/$ID"
test -d "$RESTORE_FILES"
cmp "$WORK/source/data.bin" "$RESTORE_FILES/data.bin"
cmp "$WORK/source/nested/example.txt" "$RESTORE_FILES/nested/example.txt"
if "$CLI" s3-vault restore --config "$WORK/ceph.json" --id "$ID" \
   --identity "$WORK/key.age" --destination "$WORK/restored" >/dev/null 2>&1;then
  echo "FAIL: restore overwrote an existing path" >&2;exit 1
fi

# SAME administrative bucket owner's credentials, proven able to DELETE.
# A specific COMPLIANCE version cannot be deleted, even by bucket owner.
if s3 delete-object --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
    > "$WORK/delete-result.txt" 2>&1; then
  echo "FAIL: RGW removed retained COMPLIANCE snapshot version" >&2; exit 1
fi
if ! grep -Eqi 'AccessDenied|Forbidden|retention|object.?lock|403' "$WORK/delete-result.txt"; then
  echo "FAIL: delete failure was not a verified WORM retention denial" >&2
  sed -n '1,20p' "$WORK/delete-result.txt" >&2
  exit 1
fi
# A COMPLIANCE object must not permit shortening its retain-until date.
SOON="$(date -u -d '+3 hours' +%Y-%m-%dT%H:%M:%SZ)"
if s3 put-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
  --retention "Mode=COMPLIANCE,RetainUntilDate=$SOON" \
  > "$WORK/reduce-result.txt" 2>&1;then
  echo "FAIL: RGW accepted shortening COMPLIANCE retention" >&2;exit 1
fi
s3 get-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
  > "$WORK/retention-after.json"
python3 - "$WORK/object-retention.json" "$WORK/retention-after.json" <<'PY'
import json,sys,datetime
before=json.load(open(sys.argv[1],encoding="utf-8"))["Retention"]
after=json.load(open(sys.argv[2],encoding="utf-8"))["Retention"]
assert after["Mode"] == "COMPLIANCE"
parse=lambda s:datetime.datetime.fromisoformat(s.replace("Z","+00:00"))
assert parse(after["RetainUntilDate"]) >= parse(before["RetainUntilDate"])
PY
"$CLI" s3-vault verify --config "$WORK/ceph.json" --id "$ID" --identity "$WORK/key.age"
# An unversioned DELETE may hide the current manifest but Object Lock
# preserves the independently recorded version. Rescue MUST work.
s3 delete-object --bucket "$BUCKET" --key "$MANIFEST" > "$WORK/delete-marker.json"
if s3 head-object --bucket "$BUCKET" --key "$MANIFEST" >/dev/null 2>&1;then
  echo "FAIL: delete marker did not conceal current RGW manifest" >&2;exit 1
fi
if "$CLI" s3-vault verify --config "$WORK/ceph.json" --id "$ID" \
   --identity "$WORK/key.age" >/dev/null 2>&1;then
  echo "FAIL: unpinned RGW manifest lookup bypassed delete marker" >&2;exit 1
fi
"$CLI" s3-vault verify --config "$WORK/ceph.json" --id "$ID" \
  --identity "$WORK/key.age" --manifest-version "$MANIFEST_VER"
"$CLI" s3-vault restore --config "$WORK/ceph.json" --id "$ID" \
  --identity "$WORK/key.age" --manifest-version "$MANIFEST_VER" \
  --destination "$WORK/pinned-restored"
cmp "$WORK/source/data.bin" "$WORK/pinned-restored/multi-backup/s3-vault/$ID/data.bin"
cmp "$WORK/source/nested/example.txt" "$WORK/pinned-restored/multi-backup/s3-vault/$ID/nested/example.txt"
echo "PASS: real Ceph RGW S3 COMPLIANCE retention, denied deletion, Rust age restore and pinned recovery after delete marker"
