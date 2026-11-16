#!/usr/bin/env bash
# Actual disposable PostgreSQL16 -> age -> RustFS COMPLIANCE -> empty PG DR.
# Invoked only from the existing real RustFS GitHub-hosted acceptance lab.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${RUNNER_ENVIRONMENT:-}" != github-hosted ];then
  echo "REFUSED: isolated DB/data DR requires disposable GitHub-hosted CI" >&2;exit 3
fi
: "${MB_DR_CI_TMP:?Temporary RustFS work directory required}"
: "${MB_DR_CI_NATIVE:?Compiled Rust CLI required}"
: "${MB_DR_CI_PUB:?Temporary age recipient required}"
: "${MB_DR_CI_KEY:?Temporary age identity required}"
: "${MB_DR_CI_BUCKET:?Temporary S3 WORM bucket required}"
test -x "$MB_DR_CI_NATIVE" && test -f "$MB_DR_CI_KEY"
for cmd in docker aws psql pg_dump pg_restore age python3 openssl;do command -v "$cmd" >/dev/null;done
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$MB_DR_CI_TMP"
NATIVE="$MB_DR_CI_NATIVE"
PUB="$MB_DR_CI_PUB"
BUCKET="$MB_DR_CI_BUCKET"
PG_CONTAINER="mb-dr-pg16-${GITHUB_RUN_ID:-0}-$$"
cleanup(){ docker rm -f "$PG_CONTAINER" >/dev/null 2>&1 || :; }
trap cleanup EXIT
PG_USER=mb_dr_ci
PG_PASSWORD="$(openssl rand -hex 24)"
export PGPASSWORD="$PG_PASSWORD"
docker run -d --rm --name "$PG_CONTAINER" \
  --security-opt no-new-privileges:true --memory=768m --cpus=1.5 \
  --tmpfs /var/lib/postgresql/data:rw,nosuid,nodev,size=256m \
  -p 127.0.0.1:55433:5432 \
  -e POSTGRES_USER="$PG_USER" -e POSTGRES_PASSWORD="$PG_PASSWORD" \
  -e POSTGRES_DB=mb_original postgres:16 >/dev/null
PG_BASE="postgresql://${PG_USER}@127.0.0.1:55433"
PG_SOURCE="$PG_BASE/mb_original"
PG_TARGET="$PG_BASE/mb_target"
ready=0
for n in $(seq 1 90);do
  if psql "$PG_SOURCE" -X -q -t -A -v ON_ERROR_STOP=1 -c 'SELECT 1' >/dev/null 2>&1;then
    ready=1;break
  fi
  if [ "$(docker inspect -f '{{.State.Running}}' "$PG_CONTAINER" 2>/dev/null || :)" != true ];then break;fi
  sleep 1
done
if [ "$ready" -ne 1 ];then
  echo "FAIL PostgreSQL16 disposable container did not start" >&2
  docker logs "$PG_CONTAINER" --tail 40 >&2 || :;exit 1
fi
psql "$PG_SOURCE" -X -q -v ON_ERROR_STOP=1 -c \
  "CREATE TABLE dr_marker(id integer PRIMARY KEY,payload text NOT NULL);
   INSERT INTO dr_marker VALUES (1,'isolated-database-backup'),(2,'restore-with-pinned-manifests');"
psql "$PG_SOURCE" -X -q -v ON_ERROR_STOP=1 -c 'CREATE DATABASE mb_target;' >/dev/null
mkdir -m 0700 "$TMP/dr-db-source" "$TMP/dr-data-source" "$TMP/dr-stage" "$TMP/dr-bin"
# No plaintext PostgreSQL dump is ever stored on disk.
pg_dump --format=custom --no-owner --no-acl "$PG_SOURCE" |
  age --encrypt --recipient "$PUB" --output "$TMP/dr-db-source/internal-postgresql.dump.age"
printf 'paired encrypted data with a stable owner\n' > "$TMP/dr-data-source/probe.txt"
head -c 1048576 /dev/urandom > "$TMP/dr-data-source/binary.bin"
export TMP PUB BUCKET
python3 - <<'PY'
import json,os
for suffix in ('db','data'):
    cfg={'provider':'rustfs','endpoint':'http://localhost:9000',
         'region':'us-east-1','bucket':os.environ['BUCKET'],
         'prefix':f'ci/rustfs/r083-dr-{suffix}',
         'recipient':os.environ['PUB'],'retention_days':30,
         'allow_loopback_http_for_testing':True}
    with open(os.path.join(os.environ['TMP'],f'dr-{suffix}.json'),'w') as out:
        json.dump(cfg,out)
PY
"$NATIVE" s3-vault preflight --config "$TMP/dr-db.json"
"$NATIVE" s3-vault preflight --config "$TMP/dr-data.json"
"$NATIVE" s3-vault backup --config "$TMP/dr-db.json" --source "$TMP/dr-db-source" > "$TMP/dr-db-receipt.json"
"$NATIVE" s3-vault backup --config "$TMP/dr-data.json" --source "$TMP/dr-data-source" > "$TMP/dr-data-receipt.json"
python3 - "$TMP/dr-db-receipt.json" "$TMP/dr-data-receipt.json" > "$TMP/dr-pins.txt" <<'PY'
import json,sys
records=[json.load(open(p)) for p in sys.argv[1:]]
for record in records:
    assert len(record['id'])==32 and record['manifest_version']
    assert record['archive_version'] and record['files']>=1
print(records[0]['id'],records[0]['manifest_version'])
print(records[1]['id'],records[1]['manifest_version'])
PY
read -r DB_ID DB_PIN < "$TMP/dr-pins.txt"
read -r DATA_ID DATA_PIN < <(tail -n 1 "$TMP/dr-pins.txt")
s3(){ aws --endpoint-url http://127.0.0.1:9000 --region us-east-1 s3api "$@"; }
# Independent manifest delete markers must not defeat pinned COMPLIANCE restore.
for spec in "db:$DB_ID" "data:$DATA_ID";do
  kind="${spec%%:*}"
  id="${spec#*:}"
  object="ci/rustfs/r083-dr-${kind}/manifests/${id}.json"
  s3 delete-object --bucket "$BUCKET" --key "$object" >/dev/null
  if s3 head-object --bucket "$BUCKET" --key "$object" >/dev/null 2>&1;then
    echo "FAIL delete marker not created for $kind" >&2;exit 1
  fi
done
ln -s "$NATIVE" "$TMP/dr-bin/multi-backup-native"
export PATH="$TMP/dr-bin:$PATH"
export MULTI_BACKUP_DR_CONFIRMATION=ISOLATED-NEW-EMPTY-CLUSTER
export MULTI_BACKUP_DR_DB_TARGET_DSN="$PG_TARGET"
export MULTI_BACKUP_DR_DB_WORM_CONFIG="$TMP/dr-db.json"
export MULTI_BACKUP_DR_DB_SNAPSHOT_ID="$DB_ID"
export MULTI_BACKUP_DR_DB_MANIFEST_VERSION="$DB_PIN"
export MULTI_BACKUP_DR_DATA_WORM_CONFIG="$TMP/dr-data.json"
export MULTI_BACKUP_DR_DATA_ID="$DATA_ID"
export MULTI_BACKUP_DR_DATA_MANIFEST_VERSION="$DATA_PIN"
export MULTI_BACKUP_DR_AGE_IDENTITY="$MB_DR_CI_KEY"
export MULTI_BACKUP_DR_STAGE_ROOT="$TMP/dr-stage"
bash "$ROOT/scripts/db-disaster-restore.sh"
source_rows="$(psql "$PG_SOURCE" -X -q -t -A -v ON_ERROR_STOP=1 -c 'SELECT id,payload FROM dr_marker ORDER BY id')"
restored_rows="$(psql "$PG_TARGET" -X -q -t -A -v ON_ERROR_STOP=1 -c 'SELECT id,payload FROM dr_marker ORDER BY id')"
if [ "$source_rows" != "$restored_rows" ] || [ "$(printf '%s\n' "$restored_rows" | wc -l)" -ne 2 ];then
  echo "FAIL PostgreSQL DR rows mismatch" >&2;exit 1
fi
mapfile -t dr_dirs < <(find "$TMP/dr-stage" -type d \
  -path "*/data/multi-backup/s3-vault/$DATA_ID" -print)
if [ "${#dr_dirs[@]}" -ne 1 ];then
  echo "FAIL paired data restore directory missing or ambiguous" >&2;exit 1
fi
restored_dir="${dr_dirs[0]}"
cmp "$TMP/dr-data-source/probe.txt" "$restored_dir/probe.txt"
cmp "$TMP/dr-data-source/binary.bin" "$restored_dir/binary.bin"
mkdir -m 0700 "$TMP/dr-retry-empty"
export MULTI_BACKUP_DR_STAGE_ROOT="$TMP/dr-retry-empty"
if bash "$ROOT/scripts/db-disaster-restore.sh" > "$TMP/retry.log" 2>&1;then
  echo "FAIL repeated restore into nonempty target succeeded" >&2;exit 1
fi
if [ -n "$(find "$TMP/dr-retry-empty" -mindepth 1 -print -quit)" ];then
  echo "FAIL refusal staged unexpected data" >&2;exit 1
fi
echo "PASS real isolated PG16 -> age -> RustFS WORM -> pinned DB+data restore"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ];then
  {
    echo '### Isolated database + paired data disaster recovery (real services)'
    echo '- PostgreSQL16 dump encrypted before disk + RustFS COMPLIANCE backup: PASS'
    echo '- Both retained manifests recovered by pinned version after delete markers: PASS'
    echo '- Restored rows and companion data byte-identical; nonempty target refused'
    echo '- Scope: ephemeral lab; no Patroni three-node failover certification'
  } >> "$GITHUB_STEP_SUMMARY"
fi
