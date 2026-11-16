#!/usr/bin/env bash
# Real Ceph RGW S3 Object Lock field qualification. NEVER run on production.
# Private lab only: prerequisites are checked before ANY S3 PUT.
set -Eeuo pipefail
umask 077
test "${CEPH_WORM_FIELD_APPROVAL:-}" = "YES-DEDICATED-DISPOSABLE-CI-BUCKET" || {
  echo "NOT RUN: explicit disposable Ceph RGW lab approval is required" >&2; exit 3;
}
: "${CEPH_S3_VAULT_CONFIG:?NOT RUN: provide private Ceph config}"
: "${CEPH_ADMIN_ACCESS_KEY_ID:?NOT RUN: independent Ceph admin key}"
: "${CEPH_ADMIN_SECRET_ACCESS_KEY:?NOT RUN: independent Ceph admin secret}"
: "${CEPH_ADMIN_CONTROL_BUCKET:?NOT RUN: independent delete-control bucket}"
test -x rust/target/debug/multi-backup-native || {
  echo "NOT RUN: compile Rust native executable first" >&2; exit 3;
}
command -v aws >/dev/null || {echo "NOT RUN: awscli needed ONLY for adversarial test" >&2; exit 3;}
command -v age-keygen >/dev/null || {echo "NOT RUN: age needed" >&2; exit 3;}
mapfile -t FIELDS < <(python3 - "$CEPH_S3_VAULT_CONFIG" "$CEPH_ADMIN_CONTROL_BUCKET" <<'PY'
import json,sys
with open(sys.argv[1],encoding='utf-8') as f: c=json.load(f)
b=c.get("bucket","");control=sys.argv[2]
if c.get("provider")!="ceph-rgw" or not c.get("endpoint","").startswith("https://"):
    raise SystemExit("Ceph RGW HTTPS provider required")
if not b.endswith("-ci-worm") or not control.endswith("-ci-control") or b==control:
    raise SystemExit("ONLY dedicated -ci-worm and -ci-control buckets are permitted")
print(c["endpoint"]);print(b);print(c.get("prefix","").strip("/"));print(control)
PY
)
ENDPOINT="${FIELDS[0]}"; BUCKET="${FIELDS[1]}"; PREFIX="${FIELDS[2]}"
CONTROL="${FIELDS[3]}"
ROOT="$(mktemp -d)"
trap 'rm -rf -- "$ROOT"' EXIT
NATIVE=rust/target/debug/multi-backup-native
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
"$NATIVE" s3-vault preflight --config "$CEPH_S3_VAULT_CONFIG"
# Prove independent admin has real delete permission on a different,
# unprotected, disposable control bucket before treating denial as WORM.
printf 'control permissions\n' > "$ROOT/control.txt"
ADMIN_AWS_ACCESS_KEY_ID="$CEPH_ADMIN_ACCESS_KEY_ID"
ADMIN_AWS_SECRET_ACCESS_KEY="$CEPH_ADMIN_SECRET_ACCESS_KEY"
admin_aws() {
    AWS_ACCESS_KEY_ID="$ADMIN_AWS_ACCESS_KEY_ID" \
    AWS_SECRET_ACCESS_KEY="$ADMIN_AWS_SECRET_ACCESS_KEY" \
      aws --endpoint-url "$ENDPOINT" --region us-east-1 "$@"
}
SENTINEL="multi-backup-r047-delete-control-$$"
admin_aws s3api put-object --bucket "$CONTROL" --key "$SENTINEL" --body "$ROOT/control.txt" >/dev/null
admin_aws s3api delete-object --bucket "$CONTROL" --key "$SENTINEL" >/dev/null
age-keygen -o "$ROOT/identity" >/dev/null 2>&1
PUBLIC="$(age-keygen -y "$ROOT/identity")"
# Must match configured public age recipient; refusing any key mismatch.
export PUBLIC CEPH_S3_VAULT_CONFIG
python3 - <<'PY'
import json,os
c=json.load(open(os.environ["CEPH_S3_VAULT_CONFIG"],encoding="utf-8"))
assert c["recipient"]==os.environ["PUBLIC"],"Use a private CI config with the test's generated public recipient"
PY
mkdir "$ROOT/source"
printf 'Ceph RGW genuine encrypted restore test\n' > "$ROOT/source/real.txt"
"$NATIVE" s3-vault backup --config "$CEPH_S3_VAULT_CONFIG" --source "$ROOT/source" > "$ROOT/record.json"
read -r ID VERSION < <(python3 - "$ROOT/record.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1],encoding="utf-8"));print(r["id"],r["archive_version"])
PY
)
"$NATIVE" s3-vault verify --config "$CEPH_S3_VAULT_CONFIG" --id "$ID" --identity "$ROOT/identity"
"$NATIVE" s3-vault restore --config "$CEPH_S3_VAULT_CONFIG" --id "$ID" \
 --identity "$ROOT/identity" --destination "$ROOT/restored"
grep -Rq 'Ceph RGW genuine encrypted restore test' "$ROOT/restored"
KEY="snapshots/$ID.tar.gz.age"
if [ -n "$PREFIX" ];then KEY="$PREFIX/$KEY"; fi
if admin_aws s3api delete-object --bucket "$BUCKET" --key "$KEY" --version-id "$VERSION" >/dev/null 2>&1;then
  echo "FAIL: independently proven admin deleted COMPLIANCE retained version" >&2;exit 1
fi
"$NATIVE" s3-vault verify --config "$CEPH_S3_VAULT_CONFIG" --id "$ID" --identity "$ROOT/identity"
echo "PASS: Ceph RGW real S3 COMPLIANCE retention, admin negative version-delete, verified restore"
echo "NOTE: retained test version remains until expiry; do not silently purge it"
