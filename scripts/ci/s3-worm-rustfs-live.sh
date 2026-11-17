#!/usr/bin/env bash
# Genuine RustFS S3 WORM compatibility acceptance; no mock, no external endpoint.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${RUNNER_ENVIRONMENT:-}" != github-hosted ]; then
  echo "REFUSED: requires disposable GitHub hosted CI" >&2; exit 3
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
for dep in docker aws age-keygen python3 openssl;do command -v "$dep" >/dev/null;done
TMP="$(mktemp -d)"
CONTAINER="mb-rustfs-ci-$$"
IMAGE="docker.io/rustfs/rustfs:1.0.3"
cleanup(){ docker rm -f "$CONTAINER" >/dev/null 2>&1 || :;rm -rf -- "$TMP"; }
trap cleanup EXIT
export AWS_ACCESS_KEY_ID="rustfs-ci-$(openssl rand -hex 8)"
export AWS_SECRET_ACCESS_KEY="$(openssl rand -hex 24)"
export AWS_DEFAULT_REGION=us-east-1 AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
docker pull "$IMAGE" >/dev/null
IMAGE_SHA="$(docker image inspect "$IMAGE" --format '{{.Id}}')"
docker run -d --rm --name "$CONTAINER" \
  --security-opt no-new-privileges:true -p 127.0.0.1:9000:9000 \
  --tmpfs /data:rw,nosuid,nodev,uid=10001,gid=10001,size=512m \
  -e RUSTFS_VOLUMES=/data -e RUSTFS_ADDRESS=0.0.0.0:9000 \
  -e RUSTFS_CONSOLE_ENABLE=false \
  -e RUSTFS_ACCESS_KEY="$AWS_ACCESS_KEY_ID" \
  -e RUSTFS_SECRET_KEY="$AWS_SECRET_ACCESS_KEY" "$IMAGE" >/dev/null
s3(){ aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"; }
ready=0
for n in $(seq 1 90);do
  if s3 list-buckets >/dev/null 2>&1;then ready=1;break;fi
  if [ "$(docker inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null || true)" != true ];then break;fi
  sleep 1
done
if [ "$ready" -ne 1 ];then
  echo "FAIL real RustFS server not healthy" >&2
  docker logs "$CONTAINER" --tail 65 >&2 || :
  exit 1
fi
echo "PASS real RustFS server started ($IMAGE_SHA)"
BUCKET=rustfs-ci-worm
CONTROL=rustfs-ci-control
s3 create-bucket --bucket "$BUCKET" --object-lock-enabled-for-bucket >/dev/null
s3 put-bucket-versioning --bucket "$BUCKET" --versioning-configuration Status=Enabled >/dev/null
s3 put-object-lock-configuration --bucket "$BUCKET" \
  --object-lock-configuration '{"ObjectLockEnabled":"Enabled","Rule":{"DefaultRetention":{"Mode":"COMPLIANCE","Days":30}}}' >/dev/null
s3 get-bucket-versioning --bucket "$BUCKET" --query Status --output text | grep -qx Enabled
s3 get-object-lock-configuration --bucket "$BUCKET" \
  --query 'ObjectLockConfiguration.Rule.DefaultRetention.Mode' --output text | grep -qx COMPLIANCE
s3 create-bucket --bucket "$CONTROL" >/dev/null
echo 'delete this unprotected control object' > "$TMP/delete-control.txt"
s3 put-object --bucket "$CONTROL" --key sentinel --body "$TMP/delete-control.txt" >/dev/null
s3 delete-object --bucket "$CONTROL" --key sentinel >/dev/null
if s3 head-object --bucket "$CONTROL" --key sentinel >/dev/null 2>&1;then
  echo "FAIL ordinary deletion did not work" >&2;exit 1
fi
age-keygen -o "$TMP/key.age" >/dev/null 2>&1
age-keygen -o "$TMP/wrong.age" >/dev/null 2>&1
PUB="$(age-keygen -y "$TMP/key.age")"
export PUB TMP
python3 - <<'PY'
import json,os
cfg={"provider":"rustfs","endpoint":"http://localhost:9000","region":"us-east-1",
 "bucket":"rustfs-ci-worm","prefix":"ci/rustfs/r050","recipient":os.environ["PUB"],
 "retention_days":2,"allow_loopback_http_for_testing":True}
with open(os.path.join(os.environ["TMP"],"rustfs.json"),"w") as f:json.dump(cfg,f)
cfg["bucket"]="rustfs-ci-control"
with open(os.path.join(os.environ["TMP"],"not-worm.json"),"w") as f:json.dump(cfg,f)
PY
"$NATIVE" s3-vault preflight --config "$TMP/rustfs.json"
if "$NATIVE" s3-vault preflight --config "$TMP/not-worm.json" >/dev/null 2>&1;then
  echo "FAIL RustFS unlocked bucket accepted as WORM" >&2;exit 1
fi
mkdir -p "$TMP/source/nested"
echo 'genuine RustFS S3 restore' > "$TMP/source/nested/hello.txt"
dd if=/dev/urandom of="$TMP/source/binary.bin" bs=1024 count=256 status=none
"$NATIVE" s3-vault backup --config "$TMP/rustfs.json" --source "$TMP/source" > "$TMP/record.json"
read -r ID VERSION < <(python3 - "$TMP/record.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]));assert len(r["id"])==32 and r["archive_version"] not in ("","null",None)
print(r["id"],r["archive_version"])
PY
)
KEY="ci/rustfs/r050/snapshots/$ID.tar.gz.age"
MANIFEST="ci/rustfs/r050/manifests/$ID.json"
s3 get-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" > "$TMP/retention-before.json"
MANIFEST_VER="$(s3 head-object --bucket "$BUCKET" --key "$MANIFEST" --query VersionId --output text)"
test -n "$MANIFEST_VER" && test "$MANIFEST_VER" != null && test "$MANIFEST_VER" != None
s3 get-object-retention --bucket "$BUCKET" --key "$MANIFEST" --version-id "$MANIFEST_VER" \
  --query Retention.Mode --output text | grep -qx COMPLIANCE
python3 - "$TMP/retention-before.json" <<'PY'
import datetime,json,sys
r=json.load(open(sys.argv[1]))["Retention"]
until=datetime.datetime.fromisoformat(r["RetainUntilDate"].replace("Z","+00:00"))
assert r["Mode"]=="COMPLIANCE"
assert (until-datetime.datetime.now(datetime.timezone.utc)).total_seconds()>86400
PY
"$NATIVE" s3-vault verify --config "$TMP/rustfs.json" --id "$ID" --identity "$TMP/key.age"
if "$NATIVE" s3-vault verify --config "$TMP/rustfs.json" --id "$ID" --identity "$TMP/wrong.age" >/dev/null 2>&1;then
  echo "FAIL incorrect age key accepted" >&2;exit 1
fi
"$NATIVE" s3-vault restore --config "$TMP/rustfs.json" --id "$ID" \
  --identity "$TMP/key.age" --destination "$TMP/restore"
# Native archive::create intentionally prefixes source members with
# multi-backup/s3-vault/<snapshot-id>; verify that exact namespace.
RESTORE_FILES="$TMP/restore/multi-backup/s3-vault/$ID"
test -d "$RESTORE_FILES"
cmp "$TMP/source/nested/hello.txt" "$RESTORE_FILES/nested/hello.txt"
cmp "$TMP/source/binary.bin" "$RESTORE_FILES/binary.bin"
if "$NATIVE" s3-vault restore --config "$TMP/rustfs.json" --id "$ID" \
  --identity "$TMP/key.age" --destination "$TMP/restore" >/dev/null 2>&1;then
  echo "FAIL overwrite accepted" >&2;exit 1
fi
if s3 delete-object --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
    >"$TMP/delete.log" 2>&1;then
  echo "FAIL RustFS deleted retained COMPLIANCE version" >&2;exit 1
fi
if ! grep -Eqi 'AccessDenied|Forbidden|403|retention|object.?lock' "$TMP/delete.log";then
  echo "FAIL delete failed without retention denial" >&2
  sed -n '1,15p' "$TMP/delete.log" >&2;exit 1
fi
SOON="$(date -u -d '+3 hours' +%Y-%m-%dT%H:%M:%SZ)"
if s3 put-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" \
   --retention "Mode=COMPLIANCE,RetainUntilDate=$SOON" \
   >"$TMP/shorten.log" 2>&1;then
  echo "FAIL RustFS shortened COMPLIANCE retention" >&2;exit 1
fi
s3 get-object-retention --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" >"$TMP/retention-after.json"
python3 - "$TMP/retention-before.json" "$TMP/retention-after.json" <<'PY'
import datetime,json,sys
old,new=(json.load(open(p))["Retention"] for p in sys.argv[1:])
parse=lambda s:datetime.datetime.fromisoformat(s.replace("Z","+00:00"))
assert new["Mode"]=="COMPLIANCE"
assert parse(new["RetainUntilDate"])>=parse(old["RetainUntilDate"])
PY
"$NATIVE" s3-vault verify --config "$TMP/rustfs.json" --id "$ID" --identity "$TMP/key.age"
# A bare DELETE without versionId may create a delete marker and hide an
# immutable manifest. The independently escrowed version ID MUST still
# support verified age decryption and restore after this attack.
s3 delete-object --bucket "$BUCKET" --key "$MANIFEST" > "$TMP/delete-marker.json"
if s3 head-object --bucket "$BUCKET" --key "$MANIFEST" >/dev/null 2>&1;then
  echo "FAIL: RustFS did not hide current manifest with a delete marker" >&2;exit 1
fi
if "$NATIVE" s3-vault verify --config "$TMP/rustfs.json" --id "$ID" --identity "$TMP/key.age" >/dev/null 2>&1;then
  echo "FAIL: mutable manifest lookup unexpectedly ignored the delete marker" >&2;exit 1
fi
"$NATIVE" s3-vault verify --config "$TMP/rustfs.json" --id "$ID" \
  --manifest-version "$MANIFEST_VER" --identity "$TMP/key.age"
"$NATIVE" s3-vault restore --config "$TMP/rustfs.json" --id "$ID" \
  --manifest-version "$MANIFEST_VER" --identity "$TMP/key.age" --destination "$TMP/pinned-restore"
cmp "$TMP/source/binary.bin" "$TMP/pinned-restore/multi-backup/s3-vault/$ID/binary.bin"
# r209: anti-ransomware vault MUST use a genuinely external COMPLIANCE
# authority, never the application's local append-only filesystem policy.
"$NATIVE" vault-worm preflight --config "$TMP/rustfs.json" >"$TMP/worm-preflight.json"
python3 - "$TMP/worm-preflight.json" <<'PY'
import json,sys
p=json.load(open(sys.argv[1]))
assert p["externally_enforced_object_lock"] is True
assert p["same_host_root_compromise_certified"] is False
assert p["protected_manifest_version_required"] is True
PY
if "$NATIVE" vault-worm preflight --config "$TMP/not-worm.json" >/dev/null 2>&1;then
 echo 'FAIL: locked vault accepted an actual unprotected S3 bucket' >&2;exit 1
fi
"$NATIVE" vault-worm backup --config "$TMP/rustfs.json" \
 --source "$TMP/source" --identity "$TMP/key.age" >"$TMP/worm-receipt.json"
read -r WORM_ID WORM_VER < <(python3 - "$TMP/worm-receipt.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]))
assert r["attestation"]["externally_enforced_object_lock"]
assert not r["attestation"]["same_host_root_compromise_certified"]
m=r["record"]["manifest_version"]
assert m and r["record"]["archive_version"] and r["record"]["files"]>0
print(r["record"]["id"],m)
PY
)
WORM_OBJECT="ci/rustfs/r050/snapshots/$WORM_ID.tar.gz.age"
WORM_MANIFEST="ci/rustfs/r050/manifests/$WORM_ID.json"
if s3 delete-object --bucket "$BUCKET" --key "$WORM_OBJECT" \
 --version-id "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["record"]["archive_version"])' "$TMP/worm-receipt.json")" >/dev/null 2>&1;then
 echo 'FAIL: externally locked backup version deleted under credentials' >&2;exit 1
fi
s3 delete-object --bucket "$BUCKET" --key "$WORM_MANIFEST" >/dev/null
if "$NATIVE" vault-worm verify --config "$TMP/rustfs.json" --id "$WORM_ID" \
 --identity "$TMP/key.age" >/dev/null 2>&1;then
 echo 'FAIL: a protected WORM vault accepted an unpinned manifest' >&2;exit 1
fi
"$NATIVE" vault-worm verify --config "$TMP/rustfs.json" \
 --id "$WORM_ID" --manifest-version "$WORM_VER" \
 --identity "$TMP/key.age" >"$TMP/worm-verified.json"
"$NATIVE" vault-worm restore --config "$TMP/rustfs.json" \
 --id "$WORM_ID" --manifest-version "$WORM_VER" \
 --identity "$TMP/key.age" --destination "$TMP/worm-new-restored" \
 >"$TMP/worm-restored.json"
cmp "$TMP/source/nested/hello.txt" \
 "$TMP/worm-new-restored/multi-backup/s3-vault/$WORM_ID/nested/hello.txt"
cmp "$TMP/source/binary.bin" \
 "$TMP/worm-new-restored/multi-backup/s3-vault/$WORM_ID/binary.bin"
if "$NATIVE" vault-worm restore --config "$TMP/rustfs.json" \
 --id "$WORM_ID" --manifest-version "$WORM_VER" \
 --identity "$TMP/wrong.age" --destination "$TMP/unauthorized-worm" >/dev/null 2>&1;then
 echo 'FAIL: protected vault accepted an unrelated age key' >&2;exit 1
fi
if "$NATIVE" vault-worm restore --config "$TMP/rustfs.json" \
 --id "$WORM_ID" --manifest-version "$WORM_VER" \
 --identity "$TMP/key.age" --destination "$TMP/worm-new-restored" >/dev/null 2>&1;then
 echo 'FAIL: protected vault overwrote a prior recovered directory' >&2;exit 1
fi
echo 'PASS actual RustFS immutable COMPLIANCE anti-ransomware vault, pinned age receipt, undeletable encrypted version, remote delete-marker rescue and verified NEW restore'
echo 'NOT CERTIFIED physical host loss, same-host root compromise, compromise of remote service implementation or encryption-key loss'

# r178 B-In: genuine pinned-version object import to age-encrypted chunk
# store, not a mock or mutable "latest version" read.
INPUT_KEY=ci/rustfs/r050/source-versioned.txt
printf 'Original versioned RustFS source bytes\n' > "$TMP/s3-original.txt"
OLD_VERSION="$(s3 put-object --bucket "$BUCKET" --key "$INPUT_KEY" \
  --body "$TMP/s3-original.txt" --query VersionId --output text)"
test -n "$OLD_VERSION" && test "$OLD_VERSION" != None && test "$OLD_VERSION" != null
printf 'New conflicting latest RustFS version\n' > "$TMP/s3-newer.txt"
s3 put-object --bucket "$BUCKET" --key "$INPUT_KEY" \
  --body "$TMP/s3-newer.txt" >/dev/null
DIGEST="$(sha256sum "$TMP/s3-original.txt" | cut -d' ' -f1)"
mkdir "$TMP/age-object-store"
"$NATIVE" storage init --root "$TMP/age-object-store" --recipient "$PUB"
"$NATIVE" plugins s3-object-backup --config "$TMP/rustfs.json" \
  --object-key source-versioned.txt --version-id "$OLD_VERSION" \
  --sha256 "$DIGEST" --root "$TMP/age-object-store" \
  --identity "$TMP/key.age" --label "Pinned RustFS object" --json > "$TMP/imported.json"
IMPORTED_ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/imported.json")"
"$NATIVE" storage restore --root "$TMP/age-object-store" --identity "$TMP/key.age" \
  --id "$IMPORTED_ID" --destination "$TMP/object-recovery"
cmp "$TMP/s3-original.txt" \
  "$TMP/object-recovery/s3/$BUCKET/$INPUT_KEY"
if "$NATIVE" plugins s3-object-backup --config "$TMP/rustfs.json" \
  --object-key source-versioned.txt --version-id "$OLD_VERSION" \
  --sha256 "$(printf '0%.0s' {1..64})" --root "$TMP/age-object-store" \
  --identity "$TMP/key.age" --label "Reject corrupt digest" >/dev/null 2>&1;then
  echo "FAIL: RustFS B-In accepted an independent mismatching SHA256" >&2
  exit 1
fi
echo "PASS: S3 B-In genuine RustFS old version pinned across newer write, SHA256 and age restore"
export MB_DR_CI_TMP="$TMP" MB_DR_CI_NATIVE="$NATIVE" MB_DR_CI_PUB="$PUB" \
  MB_DR_CI_KEY="$TMP/key.age" MB_DR_CI_BUCKET="$BUCKET"
bash "$ROOT/scripts/ci/db-disaster-rustfs-live.sh"
echo "PASS real RustFS versioned COMPLIANCE, verified age backup and pinned recovery after delete marker"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ];then
  {
    echo "### RustFS real S3 WORM acceptance"
    echo "- Image $IMAGE SHA $IMAGE_SHA"
    echo "- S3 versioning + COMPLIANCE retention and mandatory age restore: PASS"
    echo "- Unlocked control delete succeeded; version-specific delete and retention-shortening denied"
    echo "- Scope: one ephemeral RustFS server, not hardware/admin tamper resistance"
  } >> "$GITHUB_STEP_SUMMARY"
fi
