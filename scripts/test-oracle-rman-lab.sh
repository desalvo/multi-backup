#!/usr/bin/env bash
# Oracle RMAN read/restore-validation on a dedicated licensed LAB only.
# This is not an executable Multi Backup Oracle backup/restore adapter.
set -euo pipefail
umask 077
: "$ORACLE_HOME" "$ORACLE_SID" "$MB_ORACLE_EXPECTED_DB_NAME" "$MB_ORACLE_LAB_APPROVAL"
[[ "$MB_ORACLE_LAB_APPROVAL" == "LICENSED_DISPOSABLE_ORACLE_LAB" ]] || { echo "FAIL: Oracle lab approval required" >&2; exit 1; }
[[ "$ORACLE_SID" =~ ^[A-Za-z][A-Za-z0-9_]{0,29}$ ]] || { echo "FAIL: invalid Oracle SID" >&2; exit 1; }
[[ "$MB_ORACLE_EXPECTED_DB_NAME" =~ ^[A-Z][A-Z0-9_]{0,29}$ ]] || { echo "FAIL: invalid expected database name" >&2; exit 1; }
[[ "$ORACLE_HOME" == /* && -d "$ORACLE_HOME" && ! -L "$ORACLE_HOME" ]] || { echo "FAIL: invalid Oracle home" >&2; exit 1; }
[[ $(id -u) -ne 0 ]] || { echo "FAIL: Oracle field job must run as an unprivileged account" >&2; exit 1; }
MARKER=/etc/multi-backup-ci/allow-disposable-oracle
test -f "$MARKER" && test ! -L "$MARKER" && [[ "$(stat -c %u "$MARKER")" == 0 ]] || { echo "FAIL: root-owned lab marker missing" >&2; exit 1; }
[[ "$(cat "$MARKER")" == "DEDICATED_LICENSED_ORACLE_LAB_ONLY" ]] || { echo "FAIL: field runner is not an approved Oracle lab" >&2; exit 1; }
test -x "$ORACLE_HOME/bin/rman"
test -x "$ORACLE_HOME/bin/sqlplus"
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
# Oracle still has a planned, non-executable provider entry. Do not permit
# a generic POSIX dump to masquerade as an RMAN full/recoverable backup.
"$CLI" plugins plan --mode backup --input oracle --output posix --json | jq -e '.executable == false and .preview_executable == false' >/dev/null
"$CLI" plugins plan --mode restore --input posix --output oracle --json | jq -e '.executable == false and .preview_executable == false' >/dev/null
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
# Local OS authentication only; no plaintext database password in argv,
# workflow variables, repository contents, logs or shell history.
set +e
"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba" > "$WORK/sqlplus.log" 2>&1 <<'SQL'
whenever sqlerror exit failure
set heading off feedback off pagesize 0 verify off echo off
select name || ':' || replace(open_mode, ' ', '') || ':' || database_role from v$database;
exit
SQL
SQL_RC=$?
set -e
if [[ $SQL_RC -ne 0 ]]; then echo "FAIL: Oracle local OS authentication or database role check failed" >&2; exit 1; fi
DB_RESULT=$(tr -d '\r\n[:space:]' < "$WORK/sqlplus.log")
[[ "$DB_RESULT" == "$MB_ORACLE_EXPECTED_DB_NAME:READWRITE:PRIMARY" ]] || { echo "FAIL: wrong Oracle database, mode or role; refusing validation" >&2; exit 1; }
cat > "$WORK/validate.rman" <<'RMAN'
BACKUP VALIDATE CHECK LOGICAL DATABASE;
RESTORE DATABASE VALIDATE;
EXIT;
RMAN
# BACKUP VALIDATE reads the database but produces no backup; RESTORE
# DATABASE VALIDATE reads *existing* RMAN pieces but performs no restore.
set +e
"$ORACLE_HOME/bin/rman" target / nocatalog cmdfile="$WORK/validate.rman" log="$WORK/rman.log" >/dev/null 2>&1
RMAN_RC=$?
set -e
if [[ $RMAN_RC -ne 0 ]] || grep -Eq '^(RMAN-00569:|RMAN-0300[0-9]:|ORA-[0-9]{5}:)' "$WORK/rman.log"; then
  echo "FAIL: genuine RMAN BACKUP VALIDATE or RESTORE DATABASE VALIDATE failed; inspect the private lab log" >&2
  grep -E '^(RMAN-00569:|RMAN-0300[0-9]:|ORA-[0-9]{5}:)' "$WORK/rman.log" | head -15 >&2 || true
  exit 1
fi
grep -Eqi 'finished backup at|finished validate at' "$WORK/rman.log" || { echo "FAIL: RMAN source validation completion evidence missing" >&2; exit 1; }
grep -Eqi 'finished restore at' "$WORK/rman.log" || { echo "FAIL: RMAN existing-backup restore-validation completion evidence missing" >&2; exit 1; }
echo "PASS Oracle licensed lab RMAN source and existing-backup validation (no bytes backed up/restored by Multi Backup)"
echo "Oracle Multi Backup adapter and isolated new-target recovery are still NOT certified."
