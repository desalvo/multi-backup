#!/usr/bin/env bash
# Genuine three-node Galera 4. Backup/PITR use actual Multi Backup Rust CLI.
# No simulated wsrep, docker privilege escalation or production databases.
set -euo pipefail
umask 077
for binary in docker age-keygen jq mariadb mariadb-binlog; do command -v "$binary"; done
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
RUNID="$(date +%s)-$$"
NET="mb-galera-ci-$RUNID"
N1="mb-galera-one-$RUNID"
N2="mb-galera-two-$RUNID"
N3="mb-galera-three-$RUNID"
TARGET="mb-galera-recovery-$RUNID"
IMAGE="mb-galera-ci:$RUNID"
WORK="$(mktemp -d)"
cleanup(){
    docker rm -f "$N1" "$N2" "$N3" "$TARGET" >/dev/null 2>&1 || true
    docker network rm "$NET" >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT
mkdir -p "$WORK/cnf" "$WORK/store" "$WORK/binlog"
docker build -q -f ci/galera/Dockerfile -t "$IMAGE" . >/dev/null
docker network create "$NET" >/dev/null
for index in 1 2 3; do
    case "$index" in
      1) name="$N1";;
      2) name="$N2";;
      3) name="$N3";;
    esac
    cat > "$WORK/cnf/$index.cnf" <<EOF
[mariadb]
bind_address=0.0.0.0
server_id=$((7110+index))
gtid_domain_id=$((7210+index))
log_bin=/var/lib/mysql/mbgalera-binlog
log_slave_updates=ON
binlog_format=ROW
sync_binlog=1
innodb_flush_log_at_trx_commit=1
default_storage_engine=InnoDB
innodb_autoinc_lock_mode=2
wsrep_on=ON
wsrep_provider=/usr/lib/galera/libgalera_smm.so
wsrep_cluster_name=mb_galera_ci
wsrep_cluster_address=gcomm://$N1
wsrep_node_name=$name
wsrep_node_address=$name
wsrep_sst_method=rsync
wsrep_gtid_mode=ON
wsrep_gtid_domain_id=7300
EOF
done
mysql(){
    local port="$1"; shift
    MYSQL_PWD=galera_ci_password mariadb --protocol=tcp -h 127.0.0.1 \
        -P "$port" -u root -N -B --execute "$1"
}
status(){
    local port="$1" var="$2"
    mysql "$port" "SHOW GLOBAL STATUS LIKE '$var'" | cut -f2
}
ready(){
    local port="$1" count="$2"
    for _ in $(seq 1 90); do
        if [ "$(status "$port" wsrep_cluster_size 2>/dev/null || :)" = "$count" ] &&
           [ "$(status "$port" wsrep_cluster_status 2>/dev/null || :)" = Primary ] &&
           [ "$(status "$port" wsrep_ready 2>/dev/null || :)" = ON ] &&
           [ "$(status "$port" wsrep_local_state_comment 2>/dev/null || :)" = Synced ]; then
            return 0
        fi
        sleep 2
    done
    echo "Unhealthy Galera member on $port (required size=$count)" >&2
    for name in "$N1" "$N2" "$N3"; do docker logs "$name" 2>&1 | tail -35 || :; done
    return 1
}
# Bootstrap first Primary Component, then join two real SST/IST members.
docker run -d --name "$N1" --hostname "$N1" --network "$NET" \
    -e MARIADB_ROOT_PASSWORD=galera_ci_password -p 127.0.0.1:13316:3306 \
    -v "$WORK/cnf/1.cnf:/etc/mysql/conf.d/galera.cnf:ro" \
    "$IMAGE" --wsrep-new-cluster >/dev/null
ready 13316 1
for index in 2 3; do
    if [ "$index" = 2 ]; then name="$N2"; else name="$N3"; fi
    port=$((13315+index))
    docker run -d --name "$name" --hostname "$name" --network "$NET" \
        -e MARIADB_ROOT_PASSWORD=galera_ci_password \
        -p "127.0.0.1:$port:3306" \
        -v "$WORK/cnf/$index.cnf:/etc/mysql/conf.d/galera.cnf:ro" "$IMAGE" >/dev/null
    ready "$port" "$index"
done
for port in 13316 13317 13318; do ready "$port" 3; done
test "$(mysql 13317 "SHOW GLOBAL VARIABLES LIKE 'wsrep_gtid_mode'" | cut -f2)" = ON
test "$(mysql 13317 "SHOW GLOBAL VARIABLES LIKE 'log_slave_updates'" | cut -f2)" = ON
# Write from all three different nodes and ensure full convergence.
mysql 13316 'CREATE DATABASE mb_galera_live CHARACTER SET utf8mb4'
mysql 13316 'CREATE TABLE mb_galera_live.records(id INT PRIMARY KEY,payload VARCHAR(64) NOT NULL) ENGINE=InnoDB'
for n in $(seq 1 30); do
    port=$((13316+n%3))
    mysql "$port" "INSERT INTO mb_galera_live.records VALUES ($n,'initial-$n')"
done
wait_rows(){
    local count="$1"
    for _ in $(seq 1 60); do
        if [ "$(mysql 13316 'SELECT COUNT(*) FROM mb_galera_live.records' 2>/dev/null || :)" = "$count" ] &&
           [ "$(mysql 13317 'SELECT COUNT(*) FROM mb_galera_live.records' 2>/dev/null || :)" = "$count" ] &&
           [ "$(mysql 13318 'SELECT COUNT(*) FROM mb_galera_live.records' 2>/dev/null || :)" = "$count" ]; then
           return 0
        fi
        sleep 1
    done
    echo "Galera replication did not converge at $count rows" >&2
    return 1
}
wait_rows 30
# Lose one node. Remaining two must retain Primary quorum and accept writes.
docker kill "$N3" >/dev/null
ready 13316 2
ready 13317 2
for n in $(seq 31 40); do
    port=13316
    if [ $((n%2)) = 0 ]; then port=13317; fi
    mysql "$port" "INSERT INTO mb_galera_live.records VALUES ($n,'during-outage-$n')"
done
test "$(mysql 13316 'SELECT COUNT(*) FROM mb_galera_live.records')" = 40
test "$(mysql 13317 'SELECT COUNT(*) FROM mb_galera_live.records')" = 40
docker start "$N3" >/dev/null
for port in 13316 13317 13318; do ready "$port" 3; done
wait_rows 40

# Quiescent logical full dump from Galera node2. It must use Rust's actual
# encrypted chunk store rather than an external SQL dump written to disk.
age-keygen -o "$WORK/identity" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
test -n "$RECIPIENT"
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
printf '[client]\npassword=galera_ci_password\n' > "$WORK/credentials.cnf"
chmod 600 "$WORK/credentials.cnf"
jq -n --arg creds "$WORK/credentials.cnf" \
    '{engine:"mysql",host:"127.0.0.1",port:13317,user:"root",database:"mb_galera_live",credentials_file:$creds,galera_expected_nodes:3}' \
    > "$WORK/source.json"
"$CLI" plugins database-backup --profile-json "$WORK/source.json" \
    --root "$WORK/store" --identity "$WORK/identity" --label galera-full \
    --json > "$WORK/base.json"
BASE_ID=$(jq -r '.id' "$WORK/base.json")
test "$BASE_ID" != null && test -n "$BASE_ID"
"$CLI" storage verify --root "$WORK/store" --identity "$WORK/identity" --id "$BASE_ID"
# A healthy two-node Primary must fail the explicit 3-node backup policy.
docker kill "$N3" >/dev/null
ready 13316 2
ready 13317 2
if "$CLI" plugins database-backup --profile-json "$WORK/source.json" \
    --root "$WORK/store" --identity "$WORK/identity" --label rejected-degraded \
    --json > "$WORK/degraded.json" 2> "$WORK/degraded.log"; then
    echo "SECURITY FAILURE: degraded 2-node cluster bypassed backup policy" >&2
    exit 1
fi
grep -F 'Galera backup refused' "$WORK/degraded.log"
test "$("$CLI" storage list --root "$WORK/store" --identity "$WORK/identity" --json | jq length)" = 1
docker start "$N3" >/dev/null
for port in 13316 13317 13318; do ready "$port" 3; done
wait_rows 40
# Stable start position after Rust full dump (no concurrent writes).
mysql 13317 'FLUSH BINARY LOGS'
STATUS=$(mysql 13317 'SHOW MASTER STATUS')
BINFILE=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $1}')
START=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $2}')
test -n "$BINFILE" && test "$START" -ge 4
# Commit on other Galera members, then prove node2's binlog has writesets.
for n in $(seq 41 60); do
    port=13316
    if [ $((n%2)) = 0 ]; then port=13318; fi
    mysql "$port" "INSERT INTO mb_galera_live.records VALUES ($n,'pitr-$n')"
done
wait_rows 60
STATUS=$(mysql 13317 'SHOW MASTER STATUS')
STOPFILE=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $1}')
STOP=$(printf '%s\n' "$STATUS" | awk 'NR==1 {print $2}')
test "$BINFILE" = "$STOPFILE" && test "$STOP" -gt "$START"
mysql 13316 "INSERT INTO mb_galera_live.records VALUES (999,'excluded')"
wait_rows 61
mysql 13317 'FLUSH BINARY LOGS'
docker stop "$N3" "$N1" "$N2" >/dev/null
docker cp "$N2:/var/lib/mysql/$BINFILE" "$WORK/binlog/$BINFILE"
test -s "$WORK/binlog/$BINFILE"
"$CLI" archive create --source "$WORK/binlog" \
    --output "$WORK/galera-binlog.tar.gz.age" --recipient "$RECIPIENT" \
    --prefix galera-binlog
"$CLI" archive verify --archive "$WORK/galera-binlog.tar.gz.age" \
    --identity "$WORK/identity"
"$CLI" archive extract --archive "$WORK/galera-binlog.tar.gz.age" \
    --identity "$WORK/identity" --destination "$WORK/recovered-binlog"
# Restore to isolated, NON-Galera MariaDB. Never touch a production member.
docker run -d --name "$TARGET" --network "$NET" \
    -e MARIADB_ROOT_PASSWORD=galera_ci_password \
    -p 127.0.0.1:13319:3306 mariadb:11.4 >/dev/null
for _ in $(seq 1 90); do
    if mysql 13319 'SELECT 1' >/dev/null 2>&1; then break; fi
    sleep 2
done
mysql 13319 'SELECT 1' >/dev/null
jq -n --arg creds "$WORK/credentials.cnf" \
    '{engine:"mysql",host:"127.0.0.1",port:13319,user:"root",database:"mb_galera_live",credentials_file:$creds}' \
    > "$WORK/target.json"
"$CLI" plugins database-restore-new --profile-json "$WORK/target.json" \
    --root "$WORK/store" --identity "$WORK/identity" --id "$BASE_ID" \
    --confirm-target mysql:127.0.0.1:13319:mb_galera_live --json
test "$(mysql 13319 'SELECT COUNT(*) FROM mb_galera_live.records')" = 40
mariadb-binlog --start-position="$START" --stop-position="$STOP" \
    "$WORK/recovered-binlog/galera-binlog/$BINFILE" |
    MYSQL_PWD=galera_ci_password mariadb --protocol=tcp -h 127.0.0.1 \
        -P 13319 -u root
test "$(mysql 13319 'SELECT COUNT(*) FROM mb_galera_live.records')" = 60
test "$(mysql 13319 'SELECT SUM(id) FROM mb_galera_live.records')" = 1830
test "$(mysql 13319 'SELECT COUNT(*) FROM mb_galera_live.records WHERE id=999')" = 0
if "$CLI" plugins database-restore-new --profile-json "$WORK/target.json" \
    --root "$WORK/store" --identity "$WORK/identity" --id "$BASE_ID" \
    --confirm-target mysql:127.0.0.1:13319:mb_galera_live; then
    echo "SECURITY FAILURE: existing recovery DB was overwritten" >&2
    exit 1
fi
test "$(mysql 13319 'SELECT COUNT(*) FROM mb_galera_live.records')" = 60
echo "PASS real Galera 3 nodes / quorum2 / rejoin3 / Rust encrypted FULL / replicated cross-node binlog PITR / 60 rows sum1830 / excluded ID999"
