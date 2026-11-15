# Multi Backup r039 — authentic MariaDB 11.4 Galera 4 acceptance

## Verified result

**PASS — GitHub Actions 43673606744, source commit 37e5de3943b76f453fe5448d251f64db7d80afa5.**

`.github/workflows/database-galera-integration.yml` builds an isolated
MariaDB 11.4 image with a genuine Galera 4 wsrep provider and rsync-based
SST, then runs `scripts/test-mariadb-galera-pitr.sh` with three nodes on
a private Docker network.

The end-to-end checks validate three Synced nodes in a Primary component,
writes originating at each member, shutdown of node 3 while the other two
maintain quorum and serve ten more writes, and recovery/rejoin of node 3
without losing any of the forty records.

A native Rust `plugins database-backup` operation captures a consistent
FULL logical snapshot from Galera node 2. A `galera_expected_nodes=3`
profile verifies Primary/Synced/ready status and exact membership before
and after dumping. A forced two-node Primary membership must reject another
backup without publishing a new encrypted manifest.

For PITR, twenty additional transactions are written exclusively to the
OTHER two Galera nodes, then the collection from node 2's
`log_slave_updates` ROW binary log is encrypted and verified by native
Multi Backup. Replaying its selected start/end positions on an **isolated
non-Galera restore server** after the Rust full backup yields exactly
60 rows, ID sum 1830; the post-boundary row ID 999 must be absent.
An attempted second restore into the same database is rejected.

## Security / remaining qualification

This is a genuine controlled integration acceptance, not production
certification across physical Galera members, network partitions, concurrent
DDL, full-quorum failures, authenticated SST, multiple MariaDB releases,
multi-binlog GTID continuity, continuously collected logs, or failover of
the binlog source itself. This CI script temporarily stages plaintext binlog
files before encryption; a production continuous-log ingestion adapter must
avoid this and prove complete source-to-recovery epoch consistency.

The Galera policy is opt-in and scoped to MariaDB source backups. A
Galera-selected recovery destination is rejected: use a separately created
standalone recovery server. Check `docs/DATABASE_PITR_R039_IT.md` and EN
for non-cluster PITR acceptance and remaining production blockers.
