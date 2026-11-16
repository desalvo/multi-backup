#!/usr/bin/env bash
# Explicit, fail-closed DR onto a NEW EMPTY PostgreSQL logical cluster.
# Never stop Patroni, drop a database, overwrite local stores or contact
# a remote personal backup source implicitly.
set -Eeuo pipefail
umask 077
if [ "${MULTI_BACKUP_DR_CONFIRMATION:-}" != "ISOLATED-NEW-EMPTY-CLUSTER" ];then
  echo "REFUSED: disaster recovery requires an isolated NEW EMPTY PostgreSQL cluster" >&2
  exit 3
fi
: "${MULTI_BACKUP_DR_DB_TARGET_DSN:?New EMPTY PostgreSQL target DSN required}"
: "${MULTI_BACKUP_DR_DB_WORM_CONFIG:?Immutable DB S3 config required}"
: "${MULTI_BACKUP_DR_DB_SNAPSHOT_ID:?Database backup id required}"
: "${MULTI_BACKUP_DR_AGE_IDENTITY:?Offline age identity required}"
: "${MULTI_BACKUP_DR_STAGE_ROOT:?Private new empty DR staging root required}"
if [ -n "${MULTI_BACKUP_DR_DATA_ID:-}" ];then
  : "${MULTI_BACKUP_DR_DATA_WORM_CONFIG:?Paired data S3 config required}"
fi
for cmd in age psql pg_restore multi-backup-native;do command -v "$cmd" >/dev/null;done
if command -v systemctl >/dev/null;then
  if systemctl -q is-active multi-backup-server.service;then
    echo "REFUSED: stop all Multi Backup applications BEFORE database recovery" >&2;exit 3
  fi
fi
test -f "$MULTI_BACKUP_DR_AGE_IDENTITY"
test -d "$MULTI_BACKUP_DR_STAGE_ROOT"
[ ! -L "$MULTI_BACKUP_DR_STAGE_ROOT" ]
[ "$(stat -c %a "$MULTI_BACKUP_DR_STAGE_ROOT")" = "700" ]
# Refuse to connect to a PostgreSQL standby; never force a Patroni promotion.
role="$(psql "$MULTI_BACKUP_DR_DB_TARGET_DSN" -X -q -t -A -v ON_ERROR_STOP=1 \
  -c 'SELECT pg_is_in_recovery()')"
if [ "$role" != f ];then
  echo "REFUSED: DR database target is a replica or unavailable" >&2;exit 3
fi
# Refuse to restore into an existing logical schema containing any user table.
tables="$(psql "$MULTI_BACKUP_DR_DB_TARGET_DSN" -X -q -t -A -v ON_ERROR_STOP=1 \
  -c "SELECT count(*) FROM pg_catalog.pg_class c
   JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
   WHERE c.relkind IN ('r','p','m') AND n.nspname NOT IN
   ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%'")"
if [ "$tables" != 0 ];then
  echo "REFUSED: PostgreSQL restore target already contains user objects" >&2;exit 3
fi
# Check both independent immutable archives BEFORE modifying any DB rows.
multi-backup-native s3-vault preflight --config "$MULTI_BACKUP_DR_DB_WORM_CONFIG"
multi-backup-native s3-vault verify --config "$MULTI_BACKUP_DR_DB_WORM_CONFIG" \
  --id "$MULTI_BACKUP_DR_DB_SNAPSHOT_ID" --identity "$MULTI_BACKUP_DR_AGE_IDENTITY"
if [ -n "${MULTI_BACKUP_DR_DATA_ID:-}" ];then
  multi-backup-native s3-vault preflight --config "$MULTI_BACKUP_DR_DATA_WORM_CONFIG"
  multi-backup-native s3-vault verify --config "$MULTI_BACKUP_DR_DATA_WORM_CONFIG" \
    --id "$MULTI_BACKUP_DR_DATA_ID" --identity "$MULTI_BACKUP_DR_AGE_IDENTITY"
fi
work="$(mktemp -d "$MULTI_BACKUP_DR_STAGE_ROOT/dr-XXXXXXXX")"
chmod 0700 "$work"
# Deliberately preserve the staged recovery material on success for operator
# review; on failure the encrypted artifacts are retained without overwrite.
trap 'echo "DR staging directory retained for operator inspection: $work" >&2' EXIT
multi-backup-native s3-vault restore --config "$MULTI_BACKUP_DR_DB_WORM_CONFIG" \
  --id "$MULTI_BACKUP_DR_DB_SNAPSHOT_ID" \
  --identity "$MULTI_BACKUP_DR_AGE_IDENTITY" --destination "$work/db"
mapfile -d '' -t db_files < <(find "$work/db" -type f -name internal-postgresql.dump.age -print0)
if [ "${#db_files[@]}" -ne 1 ];then
  echo "REFUSED: immutable archive missing exactly one encrypted PostgreSQL dump" >&2
  exit 1
fi
if [ -n "${MULTI_BACKUP_DR_DATA_ID:-}" ];then
  # Destination is a NEW directory, not the old primary node data root.
  multi-backup-native s3-vault restore --config "$MULTI_BACKUP_DR_DATA_WORM_CONFIG" \
    --id "$MULTI_BACKUP_DR_DATA_ID" --identity "$MULTI_BACKUP_DR_AGE_IDENTITY" \
    --destination "$work/data"
fi
# The inner age envelope is authenticated and streamed directly into
# pg_restore; there is NO plaintext PostgreSQL dump on local disk.
age --decrypt --identity "$MULTI_BACKUP_DR_AGE_IDENTITY" "${db_files[0]}" |
 pg_restore --dbname "$MULTI_BACKUP_DR_DB_TARGET_DSN" --single-transaction \
   --exit-on-error --no-owner --no-acl
echo "DB logical restore completed into new empty cluster. Data archive (if requested) staged: $work"
echo "DO NOT enable servers until matching data snapshots, ownership and logical integrity have been independently validated"
