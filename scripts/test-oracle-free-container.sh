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
# Oracle Free starts in NOARCHIVELOG, so a PDB online RMAN backup fails (RMAN-06817).
# Modify ONLY this disposable CI instance; no production account is available.
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' > "$WORK/archivelog.log" 2>&1 <<'SQL'
whenever sqlerror exit failure
shutdown immediate;
startup mount;
alter database archivelog;
alter database open;
exit
SQL
then
  echo "FAIL: cannot enable ARCHIVELOG on disposable Oracle Free" >&2
  grep -E 'ORA-|SP2-|ERROR' "$WORK/archivelog.log" | head -15 >&2 || true
  exit 1
fi
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' > "$WORK/reopened.log" 2>&1 <<'SQL'
whenever sqlerror exit failure
set heading off feedback off pagesize 0 echo off verify off
select log_mode from v$database;
select name || ':' || replace(open_mode, ' ', '') from v$pdbs where name='FREEPDB1';
exit
SQL
then
  echo "FAIL: Oracle failed to reopen in ARCHIVELOG mode" >&2
  exit 1
fi
grep -q 'ARCHIVELOG' "$WORK/reopened.log" || { echo "FAIL: Oracle ARCHIVELOG not enabled" >&2; exit 1; }
if ! grep -q 'FREEPDB1:READWRITE' "$WORK/reopened.log"; then
  if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' > "$WORK/open-pdb.log" 2>&1 <<'SQL'
whenever sqlerror exit failure
alter pluggable database FREEPDB1 open;
exit
SQL
  then
    echo "FAIL: cannot open FREEPDB1 after ARCHIVELOG switch" >&2
    grep -E 'ORA-|SP2-|ERROR' "$WORK/open-pdb.log" | head -15 >&2 || true
    exit 1
  fi
fi
echo "Disposable Oracle Free ARCHIVELOG mode verified"
# Physical recovery verification: a real PDB user table present BEFORE RMAN.
# Only this isolated disposable Oracle Free container is modified.
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' > "$WORK/sentinel-create.log" 2>&1 <<'SQL'
whenever sqlerror exit failure
alter session set container=FREEPDB1;
create user MB_RECOVERY identified by "MB_ONLY_DISPOSABLE_CI_2026" default tablespace USERS quota unlimited on USERS;
grant create session, create table to MB_RECOVERY;
create table MB_RECOVERY.PHYSICAL_MARKER (id number primary key, phase varchar2(64));
insert into MB_RECOVERY.PHYSICAL_MARKER values (1,'BEFORE_REAL_RMAN');
commit;
exit
SQL
then
  echo "FAIL: cannot create persisted Oracle Free PDB restore marker" >&2
  grep -E 'ORA-|SP2-|ERROR' "$WORK/sentinel-create.log" | head -12 >&2 || true
  exit 1
fi
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
"$CLI" plugins oracle-rman-media-backup --pieces "$WORK/pieces" \
  --root "$WORK/encrypted-store" --identity "$WORK/identity" \
  --label oracle-free-real-rman-pieces --json > "$WORK/snapshot.json"
jq -e '.provider == "oracle" and .encrypted == true and .verified == true and .pieces > 0' "$WORK/snapshot.json" >/dev/null
SNAP=$(jq -er .id "$WORK/snapshot.json")
"$CLI" storage verify --root "$WORK/encrypted-store" --identity "$WORK/identity" --id "$SNAP" >/dev/null

# Real RMAN backup pieces are retained ONLY as authenticated age archives.
# An unrelated recovery identity must fail before any recovered piece is
# published. The test never mutates the original Oracle database.
age-keygen -o "$WORK/wrong-identity" >/dev/null 2>&1
if "$CLI" storage verify --root "$WORK/encrypted-store" \
  --identity "$WORK/wrong-identity" --id "$SNAP" >/dev/null 2>&1;then
  echo 'FAIL: Oracle RMAN encrypted backup accepted unrelated age identity' >&2
  exit 1
fi

"$CLI" plugins oracle-rman-media-restore --root "$WORK/encrypted-store" \
  --identity "$WORK/identity" --id "$SNAP" --destination "$WORK/recovered" \
  --json >"$WORK/native-restore.json"
jq -e '.provider == "oracle" and .verified == true and .pieces > 0' "$WORK/native-restore.json" >/dev/null
if "$CLI" plugins oracle-rman-media-restore --root "$WORK/encrypted-store" \
  --identity "$WORK/identity" --id "$SNAP" --destination "$WORK/recovered" \
  >/dev/null 2>&1; then echo 'FAIL Oracle media adapter overwrote existing restore' >&2; exit 1; fi
if "$CLI" plugins oracle-rman-media-restore --root "$WORK/encrypted-store" \
  --identity "$WORK/wrong-identity" --id "$SNAP" --destination "$WORK/recovered-wrong" \
  >/dev/null 2>&1; then echo 'FAIL Oracle media adapter accepted wrong key' >&2; exit 1; fi
test ! -e "$WORK/recovered-wrong"
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
# Stage A: genuine PDB physical RESTORE and RECOVER (not just VALIDATE).
cat > "$WORK/close-pdb.sql" <<'SQL'
whenever sqlerror exit failure
alter pluggable database FREEPDB1 close immediate;
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/close-pdb.sql" > "$WORK/pdb-close.log" 2>&1 ||
 { echo "FAIL: PDB did not close before physical restore" >&2; exit 1; }
cat > "$WORK/physical-pdb.rman" <<'RMAN'
RESTORE PLUGGABLE DATABASE FREEPDB1;
RECOVER PLUGGABLE DATABASE FREEPDB1;
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' \
 < "$WORK/physical-pdb.rman" > "$WORK/physical-pdb.log" 2>&1;then
 echo 'FAIL: real PDB physical RESTORE/RECOVER failed' >&2
 grep -E '^(ORA-|RMAN-)' "$WORK/physical-pdb.log" | head -15 >&2 || true;exit 1
fi
grep -qi 'Finished restore at' "$WORK/physical-pdb.log" &&
 grep -qi 'Finished recover at' "$WORK/physical-pdb.log" || {
 echo 'FAIL: PDB physical restore did not complete both phases' >&2;exit 1; }
cat > "$WORK/check-before.sql" <<'SQL'
whenever sqlerror exit failure
set heading off feedback off pagesize 0
alter pluggable database FREEPDB1 open;
alter session set container=FREEPDB1;
select 'MBPDB:'||count(*) from MB_RECOVERY.PHYSICAL_MARKER where id=1 and phase='BEFORE_REAL_RMAN';
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/check-before.sql" > "$WORK/physical-pdb-check.log" 2>&1 ||
 { echo 'FAIL: physical PDB restore did not reopen' >&2;exit 1; }
grep -q 'MBPDB:1' "$WORK/physical-pdb-check.log" ||
 { echo 'FAIL: physical PDB restore lost persisted table marker' >&2;exit 1; }
echo 'PASS genuine Oracle Free PDB physical RMAN RESTORE+RECOVER from Rust-recovered backup pieces'

# The PDB media is no longer needed in this ephemeral lab; bound disk usage
# before the larger full-CDB backup. Never delete user-provided data.
docker exec -u oracle "$NAME" sh -c 'rm -f /tmp/mb-oracle-ci/pdb_*.bkp'
rm -rf -- "$WORK/pieces" "$WORK/recovered"

# Stage B: full CDB backup including root, PDB, archived redo and controlfile.
# Local Oracle Free has ARCHIVELOG enabled; use only Oracle native basic
# BACKUPSET functionality (NO advanced compression or TDE licensing assumptions).
cat > "$WORK/backup-cdb.rman" <<'RMAN'
BACKUP AS BACKUPSET DATABASE FORMAT '/tmp/mb-oracle-ci/cdb_%U.bkp' TAG 'MB_REAL_FULL_CDB';
SQL 'ALTER SYSTEM ARCHIVE LOG CURRENT';
BACKUP AS BACKUPSET ARCHIVELOG ALL FORMAT '/tmp/mb-oracle-ci/arch_%U.bkp' TAG 'MB_CI_REDO';
BACKUP AS BACKUPSET CURRENT CONTROLFILE FORMAT '/tmp/mb-oracle-ci/ctl_%U.bkp' TAG 'MB_CI_CTL';
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' \
 < "$WORK/backup-cdb.rman" > "$WORK/full-cdb-backup.log" 2>&1;then
 echo "FAIL: real CDB plus ARCHIVELOG/CONTROLFILE RMAN backup failed" >&2
 grep -E '^(RMAN-|ORA-)' "$WORK/full-cdb-backup.log" | head -15 >&2 || true; exit 1
fi
mkdir -p "$WORK/cdb-pieces"
docker cp "$NAME:/tmp/mb-oracle-ci/." "$WORK/cdb-pieces/" >/dev/null
rm -f "$WORK/cdb-pieces"/pdb_*.bkp
find "$WORK/cdb-pieces" -type f -name 'cdb_*.bkp' -size +0c | grep -q . ||
 { echo 'FAIL: no whole CDB RMAN backup piece' >&2;exit 1; }
find "$WORK/cdb-pieces" -type f -name 'arch_*.bkp' -size +0c | grep -q . ||
 { echo 'FAIL: no archived redo backup piece' >&2;exit 1; }
find "$WORK/cdb-pieces" -type f -name 'ctl_*.bkp' -size +0c | grep -q . ||
 { echo 'FAIL: no RMAN controlfile backup piece' >&2;exit 1; }
"$CLI" plugins oracle-rman-media-backup --pieces "$WORK/cdb-pieces" \
 --root "$WORK/encrypted-store" --identity "$WORK/identity" \
 --label oracle-free-complete-cdb-controlfile-redo --json > "$WORK/cdb-manifest.json"
CDB_SNAP=$(jq -er .id "$WORK/cdb-manifest.json")
"$CLI" storage verify --root "$WORK/encrypted-store" \
 --identity "$WORK/identity" --id "$CDB_SNAP" >/dev/null
"$CLI" plugins oracle-rman-media-restore --root "$WORK/encrypted-store" \
 --identity "$WORK/identity" --id "$CDB_SNAP" --destination "$WORK/cdb-recovered" \
 --json > "$WORK/cdb-restore.json"
jq -e '.provider=="oracle" and .verified and .pieces>=3' "$WORK/cdb-restore.json" >/dev/null
for file in "$WORK/cdb-pieces/"*.bkp;do
 cmp "$file" "$WORK/cdb-recovered/oracle/rman/$(basename "$file")"
done
# The Oracle RMAN catalog references exactly the original media location:
# remove ALL unencrypted originals, replace them with Rust-recovered media.
docker exec -u oracle "$NAME" sh -c 'rm -f /tmp/mb-oracle-ci/cdb_*.bkp /tmp/mb-oracle-ci/arch_*.bkp /tmp/mb-oracle-ci/ctl_*.bkp'
# The physically recovered files must be the age-deciphered copies.
docker exec -u oracle "$NAME" sh -c 'test -z "$(find /tmp/mb-oracle-ci -type f -name "*.bkp" -print -quit)"'
docker cp "$WORK/cdb-recovered/oracle/rman/." "$NAME:/tmp/mb-oracle-ci/" >/dev/null
docker exec -u root "$NAME" sh -c 'chown -R oracle:oinstall /tmp/mb-oracle-ci && chmod 0700 /tmp/mb-oracle-ci && chmod 0600 /tmp/mb-oracle-ci/*.bkp'
rm -rf -- "$WORK/cdb-pieces" "$WORK/cdb-recovered"

# Stage C: marker/SCN crossing the backup's recovery window.
# A row inserted after the full CDB backup must survive COMPLETE recovery,
# but must disappear when RMAN restores to the earlier target SCN.
cat > "$WORK/record-target-scn.sql" <<'SQL'
whenever sqlerror exit failure
set heading off feedback off pagesize 0 echo off verify off
select 'MBSNC:'||trim(to_char(current_scn)) from v$database;
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/record-target-scn.sql" > "$WORK/scn.log" 2>&1 ||
 { echo 'FAIL: cannot read genuine Oracle target SCN' >&2;exit 1; }
SCN=$(sed -n 's/^[[:space:]]*MBSNC:\([0-9][0-9]*\).*$/\1/p' "$WORK/scn.log" | head -1)
[[ "$SCN" =~ ^[0-9]+$ ]] && test "$SCN" -gt 1000 ||
 { echo 'FAIL: invalid Oracle PITR target SCN' >&2;exit 1; }
cat > "$WORK/late-write.sql" <<'SQL'
whenever sqlerror exit failure
alter session set container=FREEPDB1;
insert into MB_RECOVERY.PHYSICAL_MARKER values (2,'AFTER_CDB_BACKUP');
commit;
alter session set container=CDB$ROOT;
alter system archive log current;
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/late-write.sql" > "$WORK/late-write.log" 2>&1 ||
 { echo 'FAIL: Oracle post-backup redo marker missing' >&2;exit 1; }

# Stage D: real WHOLE CDB physical RESTORE/RECOVER with current redo.
cat > "$WORK/cdb-mount.sql" <<'SQL'
whenever sqlerror exit failure
shutdown immediate;
startup mount;
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/cdb-mount.sql" > "$WORK/cdb-mount.log" 2>&1 ||
 { echo 'FAIL: Oracle CDB mount preparation failed' >&2;exit 1; }
cat > "$WORK/whole-cdb.rman" <<'RMAN'
RESTORE DATABASE;
RECOVER DATABASE;
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' \
 < "$WORK/whole-cdb.rman" > "$WORK/cdb-recovery.log" 2>&1;then
 echo 'FAIL: full physical CDB restore/recover failed' >&2
 grep -E '^(RMAN-|ORA-)' "$WORK/cdb-recovery.log" | head -18 >&2 || true;exit 1
fi
grep -qi 'Finished restore at' "$WORK/cdb-recovery.log" &&
 grep -qi 'Finished recover at' "$WORK/cdb-recovery.log" ||
 { echo 'FAIL: full CDB RMAN did not physically restore and recover' >&2;exit 1; }
cat > "$WORK/open-cdb.sql" <<'SQL'
whenever sqlerror exit failure
alter database open;
alter pluggable database FREEPDB1 open;
alter session set container=FREEPDB1;
set heading off feedback off pagesize 0
select 'FULLCDB:'||count(*) from MB_RECOVERY.PHYSICAL_MARKER where id in (1,2);
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/open-cdb.sql" > "$WORK/full-cdb-check.log" 2>&1 ||
 { echo 'FAIL: physically recovered CDB/PDB could not open' >&2;exit 1; }
grep -q 'FULLCDB:2' "$WORK/full-cdb-check.log" ||
 { echo 'FAIL: complete CDB recovery did not replay post-backup redo' >&2;exit 1; }
echo 'PASS physical full Oracle Free CDB RMAN RESTORE+RECOVER and post-backup redo replay'

# Stage E: FULL CDB point-in-time recovery to exact SCN BEFORE late insert.
# Destructive RESETLOGS happens ONLY in the ephemeral Oracle Free container.
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/cdb-mount.sql" > "$WORK/pitr-mount.log" 2>&1 ||
 { echo 'FAIL: PITR re-mount failed' >&2;exit 1; }
cat > "$WORK/pitr-cdb.rman" <<RMAN
RUN {
 SET UNTIL SCN $SCN;
 RESTORE DATABASE;
 RECOVER DATABASE;
}
EXIT;
RMAN
if ! docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/rman" target / nocatalog' \
 < "$WORK/pitr-cdb.rman" > "$WORK/pitr.log" 2>&1;then
 echo 'FAIL: real physical whole-CDB PITR RMAN failed' >&2
 grep -E '^(RMAN-|ORA-)' "$WORK/pitr.log" | head -18 >&2 || true;exit 1
fi
cat > "$WORK/check-pitr.sql" <<'SQL'
whenever sqlerror exit failure
alter database open resetlogs;
alter pluggable database FREEPDB1 open;
alter session set container=FREEPDB1;
set heading off feedback off pagesize 0
select 'PITR-EARLY:'||count(*) from MB_RECOVERY.PHYSICAL_MARKER where id=1;
select 'PITR-LATE:'||count(*) from MB_RECOVERY.PHYSICAL_MARKER where id=2;
exit
SQL
docker exec -u oracle -i "$NAME" sh -c '"$ORACLE_HOME/bin/sqlplus" -L -s "/ as sysdba"' \
 < "$WORK/check-pitr.sql" > "$WORK/pitr-check.log" 2>&1 ||
 { echo 'FAIL: PITR CDB RESETLOGS/open failed' >&2;exit 1; }
grep -q 'PITR-EARLY:1' "$WORK/pitr-check.log" &&
 grep -q 'PITR-LATE:0' "$WORK/pitr-check.log" ||
 { echo 'FAIL: PITR did not restore expected BEFORE and exclude AFTER marker' >&2;exit 1; }
echo 'PASS real Oracle Free PDB and CDB PHYSICAL restore plus CDB PITR at a pinned SCN from Rust age-encrypted recovery'

