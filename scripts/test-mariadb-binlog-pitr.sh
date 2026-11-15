#!/usr/bin/env bash
# Real MariaDB 11.4 ROW binlog PITR from the Multi Backup Rust logical dump.
# Captures binlog file+exact start/stop positions and rejects transactions
# committed after the chosen boundary.
set -euo pipefail
umask 077
WORK=$(mktemp -d)
SOURCE="mb-maria-pitr-src-$$"
TARGET="mb-maria-pitr-dst-$$"
trap 'docker rm -f "$SOURCE" "$TARGET" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
for binary in docker mariadb mariadb-binlog jq age-keygen; do command -v "$binary"; done
mkdir -p "$WORK/logs" "$WORK/sealed" "$WORK/store"
sudo chown -R 999:999 "$WORK/logs"
age-keygen -o "$WORK/key" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/key" | head -1)
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
printf '[client]\npassword=mariadb_ci_password\n' > "$WORK/client.cnf"
chmod 600 "$WORK/client.cnf"
docker run -d --name "$SOURCE" -e MARIADB_ROOT_PASSWORD=mariadb_ci_password \
    -p 13306:3306 -v "$WORK/logs:/binlogs" \
    mariadb:11.4 --log-bin=/binlogs/binlog --server-id=711 \
    --binlog-format=ROW --sync-binlog=1 --innodb-flush-log-at-trx-commit=1 > /dev/null
mysql() {
    local port="$1"
    shift
    MYSQL_PWD=mariadb_ci_password mariadb --protocol=tcp -h 127.0.0.1 \
        -P "$port" -u root -N -B --execute "$1"
}
ready() {
    local port="$1"
    for _ in $(seq 1 90); do
        if mysql "$port" 'SELECT 1' >/dev/null 2>&1; then return 0; fi
        sleep 2
    done
    return 1
}
ready 13306
mysql 13306 'CREATE DATABASE mb_pitr_source CHARACTER SET utf8mb4'
mysql 13306 'CREATE TABLE mb_pitr_source.records (id INTEGER PRIMARY KEY, payload VARCHAR(80) NOT NULL) ENGINE=InnoDB'
for n in $(seq 1 10); do
    mysql 13306 "INSERT INTO mb_pitr_source.records VALUES ($n, 'initial-$n')"
done
jq -n --arg creds "$WORK/client.cnf" \
    '{engine:"mysql",host:"127.0.0.1",port:13306,user:"root",database:"mb_pitr_source",credentials_file:$creds}' \
    > "$WORK/source.json"
# Actual Rust plugin FULL logical backup, not only a raw mariadb-dump command.
"$CLI" plugins database-backup --profile-json "$WORK/source.json" \
    --root "$WORK/store" --identity "$WORK/key" --label mariadb-binlog-base \
    --json > "$WORK/base.json"
BASE_ID=$(jq -r '.id' "$WORK/base.json")
test -n "$BASE_ID"
"$CLI" storage verify --root "$WORK/store" --identity "$WORK/key" --id "$BASE_ID"
# No writers run while dump/FLUSH LOGS selects an explicit zero-gap baseline.
mysql 13306 'FLUSH BINARY LOGS'
STATUS=$(mysql 13306 'SHOW MASTER STATUS')
START_FILE=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $1}')
START_POS=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $2}')
test "$START_POS" -ge 4
test -f "$WORK/logs/$START_FILE"
for n in $(seq 11 30); do
    mysql 13306 "INSERT INTO mb_pitr_source.records VALUES ($n, 'before-boundary-$n')"
done
STATUS=$(mysql 13306 'SHOW MASTER STATUS')
STOP_FILE=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $1}')
STOP_POS=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $2}')
test "$START_FILE" = "$STOP_FILE"
test "$STOP_POS" -gt "$START_POS"
mysql 13306 "INSERT INTO mb_pitr_source.records VALUES (999, 'AFTER-PITR-BOUNDARY')"
test "$(mysql 13306 'SELECT COUNT(*) FROM mb_pitr_source.records')" = 31
mysql 13306 'FLUSH BINARY LOGS'
docker stop "$SOURCE" >/dev/null
test -s "$WORK/logs/$START_FILE"
"$CLI" archive create --source "$WORK/logs" \
    --output "$WORK/sealed/mariabinlog.tar.gz.age" --recipient "$RECIPIENT" \
    --prefix mariabinlog
"$CLI" archive verify --archive "$WORK/sealed/mariabinlog.tar.gz.age" \
    --identity "$WORK/key"
"$CLI" archive extract --archive "$WORK/sealed/mariabinlog.tar.gz.age" \
    --identity "$WORK/key" --destination "$WORK/restored-logs"
# A newly initialized and physically different DB server is required.
docker run -d --name "$TARGET" -e MARIADB_ROOT_PASSWORD=mariadb_ci_password \
    -p 13307:3306 mariadb:11.4 \
    --log-bin --server-id=712 --binlog-format=ROW > /dev/null
ready 13307
jq -n --arg creds "$WORK/client.cnf" \
    '{engine:"mysql",host:"127.0.0.1",port:13307,user:"root",database:"mb_pitr_source",credentials_file:$creds}' \
    > "$WORK/target.json"
"$CLI" plugins database-restore-new --profile-json "$WORK/target.json" \
    --root "$WORK/store" --identity "$WORK/key" --id "$BASE_ID" \
    --confirm-target mysql:127.0.0.1:13307:mb_pitr_source --json
test "$(mysql 13307 'SELECT COUNT(*) FROM mb_pitr_source.records')" = 10
# Replay only committed events through selected binlog position; all
# transactional events after STOP_POS must be absent from recovery.
mariadb-binlog --start-position="$START_POS" --stop-position="$STOP_POS" \
    "$WORK/restored-logs/mariabinlog/$START_FILE" |
    MYSQL_PWD=mariadb_ci_password mariadb --protocol=tcp -h 127.0.0.1 \
    -P 13307 -u root
test "$(mysql 13307 'SELECT COUNT(*) FROM mb_pitr_source.records')" = 30
test "$(mysql 13307 'SELECT SUM(id) FROM mb_pitr_source.records')" = 465
test "$(mysql 13307 'SELECT COUNT(*) FROM mb_pitr_source.records WHERE id=999')" = 0
if "$CLI" plugins database-restore-new --profile-json "$WORK/target.json" \
    --root "$WORK/store" --identity "$WORK/key" --id "$BASE_ID" \
    --confirm-target mysql:127.0.0.1:13307:mb_pitr_source; then
    echo "SECURITY FAILURE: existing PITR target overwritten" >&2
    exit 1
fi
test "$(mysql 13307 'SELECT COUNT(*) FROM mb_pitr_source.records')" = 30
echo "PASS MariaDB 11.4 REAL binlog PITR: base 10 + replay 20 recovered, post-position row rejected; Rust ciphertext store verified"
