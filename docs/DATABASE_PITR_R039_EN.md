# Multi Backup r039 — real PostgreSQL WAL / MariaDB binlog PITR acceptance

## Scope

`.github/workflows/database-pitr-integration.yml` launches two real,
isolated database engines on Linux GitHub Actions runners. Both scenarios use
the actual Multi Backup Rust native archive/restore and age integrity checks,
with vendor database tools responsible for physical base production and log
replay. **These are integration acceptance tests, not certification that
fully autonomous native PITR management is complete.**

PostgreSQL 16 uses a physical `pg_basebackup -Fp -X stream` source, a
separate archived WAL directory, a named restore point, a later excluded
transaction and an independent recovery server. Rust encrypts/verifies both
base and WAL archives and restores them to previously nonexistent paths.
The recovery server uses `recovery.signal`, `restore_command` and
`recovery_target_name`, and the test requires promotion and precisely
30 recovered records (sum of IDs 465), excluding ID 999. A wrong age
key and an existing restore directory must be rejected. The temporary
plaintext physical base staging used **only in the CI harness** is NOT
a production no-plaintext physical PostgreSQL plugin.

MariaDB 11.4 enables durable ROW binary logs. After a quiescent Rust logical
FULL snapshot, `FLUSH BINARY LOGS` and `SHOW MASTER STATUS` define the
start position. Twenty additional transactions are committed, the stop
position is recorded, and a deliberately excluded later commit is made.
The encrypted binlog is verified and restored by Rust to a private staging
directory. MariaDB's native `mariadb-binlog` replay applies only
the selected position range on a freshly initialized recovery server after
the Rust FULL restore. The test requires exactly 30 recovered rows, sum 465,
exclusion of the later row, and refusal to overwrite the target.

### Verified evidence and remaining qualification

- MariaDB binlog PITR: job PASS on run 43672237760 (SHA 218852c3).
- PostgreSQL PITR: initial runner failure due to missing SCRAM-protected
  replication HBA entry; subsequent CI must prove recovery end-to-end.
- Native streaming PG physical base, WAL/binlog retention/orchestration,
  continuous recovery, multi-file log gaps, GTID, concurrency, network
  failures, credentials and multiple database versions still require work.
- The authentic OpenAFS/Kerberos lab remains a separate CI job.

Run `bash scripts/test-postgresql-wal-pitr.sh` and
`bash scripts/test-mariadb-binlog-pitr.sh` on an isolated Linux test host
with Docker, age, jq, database clients and the Rust debug CLI available.
Do not point these tests at real customer databases.
