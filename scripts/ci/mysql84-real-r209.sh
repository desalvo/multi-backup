#!/usr/bin/env bash
# Genuine Oracle MySQL 8.4 with Rust-native InnoDB encrypted B-In
# and create-only restore; no fake MariaDB substitute.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
NATIVE=rust/target/debug/multi-backup-native
test -x "$NATIVE"
W=$(mktemp -d /tmp/mb-mysql84.XXXXXXXX)
NAME=mb-mysql84-$$
PORT=13308
cleanup(){
 docker rm -f "$NAME" >/dev/null 2>&1 || :
 rm -rf -- "$W"
}
trap cleanup EXIT
MYSQL_PASS="$(openssl rand -hex 16)"
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:3306" \
 -e MYSQL_ROOT_PASSWORD="$MYSQL_PASS" mysql:8.4 >/dev/null
export MYSQL_PWD="$MYSQL_PASS"
sql(){
 mariadb --protocol=tcp --host=127.0.0.1 --port="$PORT" \
  --user=root --batch --skip-column-names --execute="$1"
}
ready=0
for attempt in $(seq 1 80);do
 if sql 'SELECT 1' >/dev/null 2>&1;then ready=1;break;fi
 sleep 2
done
if test "$ready" != 1;then
 echo 'FAIL: genuine MySQL 8.4 not reachable by MariaDB client' >&2
 docker logs --tail 25 "$NAME" 2>&1 | sed -e "s/$MYSQL_PASS/[redacted]/g" >&2 || :
 exit 1
fi
mysql_version=$(sql 'SELECT VERSION()')
case "$mysql_version" in
 8.4.*) ;;
 *) echo "FAIL: expected real Oracle MySQL 8.4; got $mysql_version" >&2;exit 1;;
esac
sql 'CREATE DATABASE mb_mysql84_source CHARACTER SET utf8mb4'
sql 'CREATE TABLE mb_mysql84_source.records(id INTEGER PRIMARY KEY, payload VARCHAR(128) NOT NULL, b LONGBLOB) ENGINE=InnoDB'
python3 - <<'PY' > "$W/inserts.sql"
for i in range(1,151):
    print("INSERT INTO mb_mysql84_source.records(id,payload,b) VALUES (%d, 'payload-%d', UNHEX('000102FF'));" % (i,i))
PY
MYSQL_PWD="$MYSQL_PASS" mariadb --protocol=tcp --host=127.0.0.1 \
 --port="$PORT" --user=root < "$W/inserts.sql"
test "$(sql 'SELECT COUNT(*) FROM mb_mysql84_source.records')" = 150
printf '[client]\npassword=%s\n' "$MYSQL_PASS" >"$W/mysql.cnf"
chmod 0600 "$W/mysql.cnf"
python3 - "$W" <<'PY'
import json,os,sys
p=sys.argv[1]
base={"engine":"mysql","host":"127.0.0.1","port":13308,
      "user":"root","credentials_file":p+"/mysql.cnf"}
for db,name in [("mb_mysql84_source","source"),("mb_mysql84_recovered","target")]:
    d=dict(base,database=db)
    path=os.path.join(p,name+".json")
    with open(path,"w") as f:json.dump(d,f)
    os.chmod(path,0o600)
PY
age-keygen -o "$W/key.age" >/dev/null 2>&1
PUB=$(age-keygen -y "$W/key.age")
"$NATIVE" storage init --root "$W/store" --recipient "$PUB" >/dev/null
"$NATIVE" plugins database-backup --profile-json "$W/source.json" \
 --root "$W/store" --identity "$W/key.age" \
 --label 'Genuine Oracle MySQL 8.4 InnoDB' --json >"$W/backup.json"
ID=$(jq -er .id "$W/backup.json")
"$NATIVE" storage verify-chain --root "$W/store" \
 --identity "$W/key.age" --id "$ID" >/dev/null
test "$(sql 'SELECT COUNT(*) FROM INFORMATION_SCHEMA.SCHEMATA WHERE SCHEMA_NAME="mb_mysql84_recovered"')" = 0
"$NATIVE" plugins database-restore-new --profile-json "$W/target.json" \
 --root "$W/store" --identity "$W/key.age" --id "$ID" \
 --confirm-target mysql:127.0.0.1:13308:mb_mysql84_recovered \
 --json >"$W/restore.json"
jq -e '.overwrote_existing==false and .streamed_bytes>0' "$W/restore.json"
test "$(sql 'SELECT COUNT(*) FROM mb_mysql84_recovered.records')" = 150
test "$(sql 'SELECT SUM(id) FROM mb_mysql84_recovered.records')" = 11325
test "$(sql 'SELECT COUNT(*) FROM mb_mysql84_recovered.records WHERE HEX(b)="000102FF"')" = 150
cmp <(sql 'SELECT id,payload,HEX(b) FROM mb_mysql84_source.records ORDER BY id') \
    <(sql 'SELECT id,payload,HEX(b) FROM mb_mysql84_recovered.records ORDER BY id')
if "$NATIVE" plugins database-restore-new --profile-json "$W/target.json" \
 --root "$W/store" --identity "$W/key.age" --id "$ID" \
 --confirm-target mysql:127.0.0.1:13308:mb_mysql84_recovered >/dev/null 2>&1;then
 echo 'FAIL MySQL existing recovery database overwritten' >&2;exit 1
fi
echo 'PASS real Oracle MySQL 8.4 InnoDB Rust encrypted B-In, blob preservation and create-only 150-row recovery'
echo 'NOT CERTIFIED Oracle MySQL GTID replication, binlog PITR or live DDL snapshot barriers'
