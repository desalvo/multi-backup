#!/usr/bin/env bash
# Genuine two-principal MinIO policy enforcement on an EPHEMERAL hosted runner.
set -Eeuo pipefail
umask 077
test "$GITHUB_ACTIONS" = true
test "$RUNNER_ENVIRONMENT" = github-hosted
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
TMP="$(mktemp -d)"
SERVER_PID=""
cleanup() {
  if test -n "$SERVER_PID"; then kill "$SERVER_PID" 2>/dev/null || :; wait "$SERVER_PID" 2>/dev/null || :; fi
  rm -rf -- "$TMP"
}
trap cleanup EXIT
export GOBIN="$TMP/bin" MC_CONFIG_DIR="$TMP/mc-config"
mkdir -p "$GOBIN" "$MC_CONFIG_DIR" "$TMP"/disk{1,2,3,4}
go install github.com/minio/minio@RELEASE.2025-10-15T17-29-55Z
go install github.com/minio/mc@RELEASE.2025-08-13T08-35-41Z
test -x "$GOBIN/mc"
export AWS_ACCESS_KEY_ID="ciroot$(openssl rand -hex 8)"
export AWS_SECRET_ACCESS_KEY="$(openssl rand -hex 24)"
export AWS_DEFAULT_REGION=us-east-1 AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
MINIO_ROOT_USER="$AWS_ACCESS_KEY_ID" MINIO_ROOT_PASSWORD="$AWS_SECRET_ACCESS_KEY" \
 "$GOBIN/minio" server "$TMP"/disk{1,2,3,4} \
 --address 127.0.0.1:9000 --console-address 127.0.0.1:9001 >"$TMP/minio.log" 2>&1 &
SERVER_PID=$!
admin(){ aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"; }
READY=0
for n in $(seq 1 90);do
  if admin list-buckets >/dev/null 2>&1;then READY=1;break;fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null;then break;fi
  sleep 1
done
if test "$READY" != 1;then tail -n 40 "$TMP/minio.log" >&2;exit 1;fi
BUCKET=worm-r211-iam
admin create-bucket --bucket "$BUCKET" --object-lock-enabled-for-bucket >/dev/null
admin put-bucket-versioning --bucket "$BUCKET" --versioning-configuration Status=Enabled >/dev/null
admin put-object-lock-configuration --bucket "$BUCKET" \
 --object-lock-configuration '{"ObjectLockEnabled":"Enabled","Rule":{"DefaultRetention":{"Mode":"COMPLIANCE","Days":30}}}' >/dev/null
"$GOBIN/mc" alias set ephemeral http://127.0.0.1:9000 "$AWS_ACCESS_KEY_ID" "$AWS_SECRET_ACCESS_KEY" >/dev/null
python3 - "$ROOT" "$TMP" <<'PY'
import json,sys,pathlib
root,out=map(pathlib.Path,sys.argv[1:])
for role in ("writer","recovery"):
    text=(root/"docs"/"policies"/f"worm-{role}-iam.example.json").read_text()
    text=text.replace("__BUCKET__","worm-r211-iam").replace("__PREFIX__","ci/r211")
    (out/f"{role}.json").write_text(json.dumps(json.loads(text)))
PY
WRITER="ciwriter$(openssl rand -hex 5)"
RECOVERY="cirecovery$(openssl rand -hex 5)"
WPASS="$(openssl rand -hex 24)"
RPASS="$(openssl rand -hex 24)"
"$GOBIN/mc" admin policy create ephemeral mb-ci-writer "$TMP/writer.json" >/dev/null
"$GOBIN/mc" admin policy create ephemeral mb-ci-recovery "$TMP/recovery.json" >/dev/null
"$GOBIN/mc" admin user add ephemeral "$WRITER" "$WPASS" >/dev/null
"$GOBIN/mc" admin user add ephemeral "$RECOVERY" "$RPASS" >/dev/null
"$GOBIN/mc" admin policy attach ephemeral mb-ci-writer --user="$WRITER" >/dev/null
"$GOBIN/mc" admin policy attach ephemeral mb-ci-recovery --user="$RECOVERY" >/dev/null
writer_api(){
  env AWS_ACCESS_KEY_ID="$WRITER" AWS_SECRET_ACCESS_KEY="$WPASS" \
   aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"
}
recovery_api(){
  env AWS_ACCESS_KEY_ID="$RECOVERY" AWS_SECRET_ACCESS_KEY="$RPASS" \
   aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"
}
writer(){
  env -u AWS_ACCESS_KEY_ID -u AWS_SECRET_ACCESS_KEY -u AWS_SESSION_TOKEN \
    MB_WORM_WRITER_ACCESS_KEY_ID="$WRITER" MB_WORM_WRITER_SECRET_ACCESS_KEY="$WPASS" \
    "$NATIVE" vault-worm "$@"
}
recovery(){
  env -u AWS_ACCESS_KEY_ID -u AWS_SECRET_ACCESS_KEY -u AWS_SESSION_TOKEN \
    MB_WORM_RECOVERY_ACCESS_KEY_ID="$RECOVERY" MB_WORM_RECOVERY_SECRET_ACCESS_KEY="$RPASS" \
    "$NATIVE" vault-worm "$@"
}
deny(){
 if "$@" >"$TMP/denied.log" 2>&1;then echo "FAIL IAM allowed forbidden request" >&2;exit 1;fi
 if ! grep -Eiq 'AccessDenied|Forbidden|403|not authorized' "$TMP/denied.log";then
   echo "FAIL denied without real IAM 403" >&2;sed -n '1,12p' "$TMP/denied.log" >&2;exit 1
 fi
}
age-keygen -o "$TMP/identity" >/dev/null 2>&1
age-keygen -o "$TMP/wrong" >/dev/null 2>&1
PUB="$(age-keygen -y "$TMP/identity")"
export PUB TMP
python3 - <<'PY'
import json,os
json.dump({"provider":"minio","endpoint":"http://127.0.0.1:9000",
 "region":"us-east-1","bucket":"worm-r211-iam","prefix":"ci/r211",
 "recipient":os.environ["PUB"],"retention_days":2,
 "allow_loopback_http_for_testing":True},open(os.environ["TMP"]+"/config.json","w"))
PY
mkdir -p "$TMP/source"
echo 'immutable two-user backup acceptance' > "$TMP/source/probe"
writer preflight --config "$TMP/config.json" > "$TMP/preflight.json"
writer backup --config "$TMP/config.json" --source "$TMP/source" > "$TMP/receipt.json"
ID="$(jq -er .record.id "$TMP/receipt.json")"
MV="$(jq -er .record.manifest_version "$TMP/receipt.json")"
AV="$(jq -er .record.archive_version "$TMP/receipt.json")"
jq -e '.attestation.recovery_verified==false' "$TMP/receipt.json" >/dev/null
OBJECT="ci/r211/snapshots/$ID.tar.gz.age"
MANIFEST="ci/r211/manifests/$ID.json"
deny writer_api get-object --bucket "$BUCKET" --key "$OBJECT" --version-id "$AV" "$TMP/leak.age"
deny writer_api delete-object --bucket "$BUCKET" --key "$OBJECT"
deny writer_api delete-object --bucket "$BUCKET" --key "$OBJECT" --version-id "$AV"
deny writer_api list-object-versions --bucket "$BUCKET"
deny writer_api put-bucket-policy --bucket "$BUCKET" --policy "file://$TMP/writer.json"
deny recovery_api put-object --bucket "$BUCKET" --key ci/r211/snapshots/unauthorized --body "$TMP/source/probe"
deny recovery_api delete-object --bucket "$BUCKET" --key "$MANIFEST"
deny recovery_api delete-object --bucket "$BUCKET" --key "$OBJECT" --version-id "$AV"
deny recovery_api list-object-versions --bucket "$BUCKET"
recovery_api get-object --bucket "$BUCKET" --key "$OBJECT" --version-id "$AV" "$TMP/readback.age" >/dev/null
test -s "$TMP/readback.age"
recovery verify --config "$TMP/config.json" --id "$ID" \
 --manifest-version "$MV" --identity "$TMP/identity" > "$TMP/verify.json"
jq -e '.attestation.recovery_verified==true' "$TMP/verify.json" >/dev/null
recovery restore --config "$TMP/config.json" --id "$ID" \
 --manifest-version "$MV" --identity "$TMP/identity" \
 --destination "$TMP/restore" >/dev/null
cmp "$TMP/source/probe" "$TMP/restore/multi-backup/s3-vault/$ID/probe"
if recovery verify --config "$TMP/config.json" --id "$ID" \
 --manifest-version "$MV" --identity "$TMP/wrong" >/dev/null 2>&1;then
 echo 'FAIL wrong decryption key accepted' >&2;exit 1
fi
if recovery restore --config "$TMP/config.json" --id "$ID" \
 --manifest-version "$MV" --identity "$TMP/identity" \
 --destination "$TMP/restore" >/dev/null 2>&1;then
 echo 'FAIL overwrite accepted' >&2;exit 1
fi
echo 'PASS REAL MinIO independent non-admin writer/recovery IAM denial, COMPLIANCE retention and age restore'
echo 'LIMIT no physical 3-host root/storage-loss or cross-provider IAM portability certification'
