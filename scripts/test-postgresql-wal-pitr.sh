#!/usr/bin/env bash
# Real PostgreSQL 16 WAL PITR on isolated Docker primary and recovery cluster.
# Evidence: Multi Backup native encrypted physical base + WAL archive roundtrip,
# named restore point, post-target exclusion, fail-closed overwrite.
set -euo pipefail
umask 077
WORK=$(mktemp -d)
PRIMARY="mb-pg-pitr-src-$$"
RECOVERY="mb-pg-pitr-dst-$$"
trap 'docker rm -f "$PRIMARY" "$RECOVERY" >/dev/null 2>&1 || true; sudo rm -rf "$WORK"' EXIT
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
for binary in docker pg_basebackup psql pg_isready jq age-keygen; do command -v "$binary"; done
mkdir -p "$WORK/wal" "$WORK/sealed"
sudo chown -R 999:999 "$WORK/wal"
age-keygen -o "$WORK/key" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/key" | head -1)
test -n "$RECIPIENT"

docker run -d --name "$PRIMARY" -e POSTGRES_PASSWORD=postgres_ci_password \
    -p 15432:5432 -v "$WORK/wal:/archive_wal" postgres:16 \
    -c wal_level=replica -c archive_mode=on \
    -c "archive_command=test ! -f /archive_wal/%f && cp %p /archive_wal/%f" \
    -c archive_timeout=10s > /dev/null
ready() {
    local port="$1"
    for _ in $(seq 1 90); do
        if PGPASSWORD=postgres_ci_password pg_isready -h 127.0.0.1 -p "$port" -U postgres >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    docker logs "$PRIMARY" | tail -100
    return 1
}
pg() {
    local port="$1"
    shift
    PGPASSWORD=postgres_ci_password psql -X -A -t -v ON_ERROR_STOP=1 \
        -h 127.0.0.1 -p "$port" -U postgres -d postgres -c "$1"
}
ready 15432
# Official Docker init provides host database access, but not host
# replication protocol access. Add a narrowly scoped, SCRAM-protected
# postgres replication rule for this disposable private CI network.
docker exec -u postgres "$PRIMARY" sh -c "printf '%s\n' 'host replication postgres 172.17.0.1/32 scram-sha-256' >> /var/lib/postgresql/data/pg_hba.conf"
test "$(pg 15432 'SELECT pg_reload_conf()')" = t
pg 15432 "CREATE TABLE pitr_record (id integer PRIMARY KEY, payload text NOT NULL)"
pg 15432 "INSERT INTO pitr_record SELECT i, 'initial-' || i FROM generate_series(1,10) i"
# pg_basebackup is the engine's REAL physical-consistency producer. It
# generates private plaintext staging: this is a CI lab, NOT an unattended
# no-plaintext production adapter.
PGPASSWORD=postgres_ci_password pg_basebackup -h 127.0.0.1 -p 15432 \
    -U postgres --format=plain --wal-method=stream --checkpoint=fast \
    -D "$WORK/base"
pg 15432 "INSERT INTO pitr_record SELECT i, 'before-marker-' || i FROM generate_series(11,30) i"
pg 15432 "SELECT pg_create_restore_point('mb_pitr_safe_point')"
pg 15432 "INSERT INTO pitr_record VALUES (999, 'after-marker-must-not-recover')"
WAL_FILE=$(pg 15432 "SELECT pg_walfile_name(pg_switch_wal())")
test -n "$WAL_FILE"
for _ in $(seq 1 60); do
    if sudo sudo test -s "$WORK/wal/$WAL_FILE"; then break; fi
    sleep 2
done
test -s "$WORK/wal/$WAL_FILE"
test "$(pg 15432 'SELECT count(*) FROM pitr_record')" = 31
docker stop "$PRIMARY" >/dev/null
# Preserve strict volserver ownership during capture: work from a private
# privileged snapshot instead of relaxing the primary WAL archive permissions.
sudo cp -a "$WORK/wal" "$WORK/wal-private-snapshot"
sudo chown -R "$(id -u):$(id -g)" "$WORK/wal-private-snapshot"

# Encrypt real pg_basebackup data and archived WAL using native Multi Backup.
# Restoring requires both independently verified ciphertext archives.
"$CLI" archive create --source "$WORK/base" --output "$WORK/sealed/pgbase.tar.gz.age" \
    --recipient "$RECIPIENT" --prefix pgbase
"$CLI" archive create --source "$WORK/wal-private-snapshot" --output "$WORK/sealed/pgwal.tar.gz.age" \
    --recipient "$RECIPIENT" --prefix pgwal
"$CLI" archive verify --archive "$WORK/sealed/pgbase.tar.gz.age" --identity "$WORK/key"
"$CLI" archive verify --archive "$WORK/sealed/pgwal.tar.gz.age" --identity "$WORK/key"
# A wrong key must not restore any valid PostgreSQL data.
age-keygen -o "$WORK/wrong-key" 2>/dev/null
if "$CLI" archive extract --archive "$WORK/sealed/pgwal.tar.gz.age" \
    --identity "$WORK/wrong-key" --destination "$WORK/untrusted"; then
    echo "SECURITY FAILURE: wrong-key WAL restore succeeded" >&2
    exit 1
fi
test ! -e "$WORK/untrusted"
"$CLI" archive extract --archive "$WORK/sealed/pgbase.tar.gz.age" \
    --identity "$WORK/key" --destination "$WORK/pg-recovery"
"$CLI" archive extract --archive "$WORK/sealed/pgwal.tar.gz.age" \
    --identity "$WORK/key" --destination "$WORK/wal-recovery"
if "$CLI" archive extract --archive "$WORK/sealed/pgbase.tar.gz.age" \
    --identity "$WORK/key" --destination "$WORK/pg-recovery"; then
    echo "SECURITY FAILURE: existing physical restore overwritten" >&2
    exit 1
fi
test -s "$WORK/pg-recovery/pgbase/global/pg_control"
test -s "$WORK/wal-recovery/pgwal/$WAL_FILE"
touch "$WORK/pg-recovery/pgbase/recovery.signal"
sudo chown -R 999:999 "$WORK/pg-recovery" "$WORK/wal-recovery"
docker run -d --name "$RECOVERY" -e POSTGRES_PASSWORD=postgres_ci_password \
    -p 15433:5432 \
    -v "$WORK/pg-recovery/pgbase:/var/lib/postgresql/data" \
    -v "$WORK/wal-recovery/pgwal:/recover_wal:ro" \
    postgres:16 \
    -c "restore_command=cp /recover_wal/%f %p" \
    -c recovery_target_name=mb_pitr_safe_point \
    -c recovery_target_action=promote \
    -c archive_mode=off > /dev/null
ready 15433
# Recovery must have promoted at the named target. Only 30 rows survive.
for _ in $(seq 1 30); do
    if test "$(pg 15433 'SELECT pg_is_in_recovery()')" = f; then break; fi
    sleep 1
done
test "$(pg 15433 'SELECT pg_is_in_recovery()')" = f
ROWS=$(pg 15433 'SELECT count(*) FROM pitr_record')
SUM=$(pg 15433 'SELECT sum(id) FROM pitr_record')
test "$ROWS" = 30
test "$SUM" = 465
test "$(pg 15433 'SELECT count(*) FROM pitr_record WHERE id=999')" = 0
echo "PASS PostgreSQL 16 REAL WAL PITR: initial 10 + pre-target 20 recovered; post-target transaction excluded; encrypted base and WAL verified"
