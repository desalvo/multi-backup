#!/usr/bin/env bash
# Genuine isolated Oracle Database Free container, RMAN backup-piece roundtrip
# through the compiled Multi Backup Rust POSIX encrypted store, then RMAN restore
# VALIDATE against the recovered pieces. No actual database is overwritten.
set -euo pipefail
umask 077
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
for exe in docker jq age-keygen cmp sha256sum openssl; do command -v "$exe" >/dev/null; done
WORK=$(mktemp -d)
TAG=$(openssl rand -hex 6)
NAME="mb-oracle-ci-$TAG"
# RMAN is deliberately unavailable in the SLIM Oracle Free image.
IMAGE="ghcr.io/gvenzl/oracle-free:23.26.3@sha256:7ed34d0ade89c91a553c9cbe9d42457c758fa29a762aaaa3137541ff30a2d1cb"
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf -- "$WORK"
}
trap cleanup EXIT
PASSWORD=$(openssl rand -hex 24)
docker run -d --name "$NAME" --shm-size 1g \
  -e ORACLE_PASSWORD="$PASSWORD" "$IMAGE" >/dev/null
unset PASSWORD
READY=false
for attempt in $(seq 1 70); do
  # Use the image's authoritative FREEPDB1 readiness check rather than
  # parsing SQL*Plus prompt/output formatting.
  if docker exec "$NAME" healthcheck.sh FREEPDB1 > "$WORK/health.log" 2>&1; then
    READY=true
    break
  fi
  if [[ "$(docker inspect -f '{{.State.Running}}' "$NAME")" != true ]]; then
    echo "FAIL: Oracle container stopped before readiness" >&2
    docker inspect -f 'exit={{.State.ExitCode}} oom={{.State.OOMKilled}}' "$NAME" >&2 || true
    # Print only broadly recognizable failure lines, not SQL or credentials.
    docker logs "$NAME" 2>&1 | grep -Ei '(^|[[:space:]])(error|failed|fatal|ORA-|TNS-|out of memory|permission denied|missing file|cannot)' | tail -25 >&2 || true
    exit 1
  fi
  sleep 10
done
if [[ "$READY" != true ]]; then
  echo "FAIL: Oracle Free failed its own FREEPDB1 healthcheck after bounded startup" >&2
  docker inspect -f 'state={{.State.Status}} exit={{.State.ExitCode}} oom={{.State.OOMKilled}}' "$NAME" >&2 || true
  docker logs "$NAME" 2>&1 | grep -Ei '(error|failed|fatal|ORA-|TNS-|memory|database is ready|database creation|initializ)' | tail -30 >&2 || true
  exit 1
fi
docker exec -u oracle "$NAME" sh -c 'test -x "$ORACLE_HOME/bin/rman" && test -x "$ORACLE_HOME/bin/sqlplus"'
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' > "$WORK/pdb.log" <<'SQL'
whenever sqlerror exit failure
set heading off feedback off pagesize 0 echo off verify off
select name || ':' || replace(open_mode, ' ', '') from v$pdbs where name = 'FREEPDB1';
exit
SQL
grep -Eq '^FREEPDB1:READWRITE$' "$WORK/pdb.log" || { echo "FAIL: expected isolated writable PDB missing" >&2; exit 1; }
docker exec -u oracle "$NAME" mkdir -p /tmp/mb-oracle-ci
cat > "$WORK/backup.rman" <<'RMAN'
BACKUP AS BACKUPSET PLUGGABLE DATABASE FREEPDB1 FORMAT '/tmp/mb-oracle-ci/pdb_%U.bkp' TAG 'MB_CI_PDB';
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' < "$WORK/backup.rman" > "$WORK/backup.log" 2>&1; then
  echo "FAIL: real RMAN PDB backup command failed" >&2; grep -E '^(RMAN-|ORA-)' "$WORK/backup.log" | head -10 >&2 || true; exit 1
fi
if grep -Eq '^RMAN-[0-9]+:|^ORA-[0-9]+:' "$WORK/backup.log" || ! grep -qi 'Finished backup at' "$WORK/backup.log"; then
  echo "FAIL: real RMAN backup did not finish cleanly" >&2; exit 1
fi
mkdir -p "$WORK/pieces"
docker cp "$NAME:/tmp/mb-oracle-ci/." "$WORK/pieces/" >/dev/null
find "$WORK/pieces" -type f -name 'pdb_*.bkp' -size +0c | grep -q . || { echo "FAIL: no real RMAN backup piece created" >&2; exit 1; }
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
"$CLI" storage init --root "$WORK/encrypted-store" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins backup --input posix --output posix --source "$WORK/pieces" \
  --root "$WORK/encrypted-store" --identity "$WORK/identity" --label oracle-free-real-rman-pieces \
  --namespace rman --top-namespace oracle --json > "$WORK/snapshot.json"
SNAP=$(jq -er .id "$WORK/snapshot.json")
"$CLI" storage verify --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$SNAP" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$WORK/encrypted-store" \
  --identity "$WORK/identity" --id "$SNAP" --destination "$WORK/recovered" --json >/dev/null
for file in "$WORK/pieces/"*.bkp; do
  test -f "$file"
  cmp "$file" "$WORK/recovered/oracle/rman/$(basename "$file")"
done
# Make RMAN read *only* files recovered from our encrypted Rust archive,
# not the original backup pieces (same catalogued paths).
docker exec -u oracle "$NAME" sh -c 'rm -f /tmp/mb-oracle-ci/pdb_*.bkp'
docker cp "$WORK/recovered/oracle/rman/." "$NAME:/tmp/mb-oracle-ci/" >/dev/null
docker exec -u root "$NAME" sh -c 'chown -R oracle:oinstall /tmp/mb-oracle-ci && chmod 0700 /tmp/mb-oracle-ci && chmod 0600 /tmp/mb-oracle-ci/*.bkp'
cat > "$WORK/restore-validate.rman" <<'RMAN'
RESTORE PLUGGABLE DATABASE FREEPDB1 VALIDATE;
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' < "$WORK/restore-validate.rman" > "$WORK/restore.log" 2>&1; then
  echo "FAIL: real RMAN restore validation against encrypted-store-recovered pieces failed" >&2; grep -E '^(RMAN-|ORA-)' "$WORK/restore.log" | head -10 >&2 || true; exit 1
fi
if grep -Eq '^RMAN-[0-9]+:|^ORA-[0-9]+:' "$WORK/restore.log" || ! grep -qi 'Finished restore at' "$WORK/restore.log"; then
  echo "FAIL: RMAN did not validate recovered PDB backup pieces" >&2; exit 1
fi
echo "PASS real Oracle Free PDB RMAN backup -> Rust age-encrypted store -> Rust restore -> RMAN restore VALIDATE"
echo "Scope: genuine RMAN PDB backup-piece backup and recovery validation, NOT live database physical restore, full-CDB/PITR or a dedicated Oracle Rust adapter."
