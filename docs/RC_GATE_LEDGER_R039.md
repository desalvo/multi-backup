# Multi Backup r039: current RC qualification ledger (15 November 2026)

Source at field run: PR #11 d80675fcf80e960d446df19a2de0190b52b67e0d; Rust package production release NOT certified.

| Real service acceptance | Result | GitHub Actions run |
|---|---|---|
| Real Kerberos/OpenAFS full+incremental encrypted new-volume restore and safety negatives | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747477666 |
| POSIX full/incremental, integrity and parity repair | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747178037 |
| SSH/SCP/SFTP and SSHFS FUSE service tests | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747178182 |
| PostgreSQL/MariaDB full logical backup and restore | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747178038 |
| PostgreSQL WAL, MariaDB binlog PITR acceptance | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747178143 |
| Galera three-node quorum/rejoin and isolated PITR | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747178077 |
| Rust focused validation | PASS | https://github.com/desalvo/multi-backup/actions/runs/43747173775 |
| Exact PR #11 production CI | RUNNING at audit | https://github.com/desalvo/multi-backup/actions/runs/43747178170 |
| Authentic PG16 agent authentication and two-replica SQL placement/fencing | PASS 3/3 | https://github.com/desalvo/multi-backup/actions/runs/43750382563 |

The new PG16 workflow uses an isolated localhost-only SQL fixture and a test-only constructor bypassing production verified-TLS parsing. The earliest sequential tests passed at source a782425a (run 43749567747). A new 8-worker parallel test exposed a real SQL race at source aadbb559 (run 43750032081): max_jobs=1 admitted two simultaneous claims. The PostgreSQL transaction now locks the replica row in a SEPARATE statement before evaluating a fresh READ COMMITTED capacity snapshot (`rust-server/src/cluster.rs`, source 1d71bbb6998f115097a28b25ddbbdca1e22a7070). Both independent live runs 43750377598 and 43750382563 PASS all 3 live tests, including an 8-worker competing claim and simultaneous duplicate Ed25519 nonce requests. No changes to OpenAFS source were required. It certifies SQL state transitions only, not agent HTTPS payload transfer, actual Kubernetes replica pods or physical host/process fencing.

## Remaining release blockers

Live GitHub/LFS/CI and Dropbox cross-account backup/restore with metadata, throttling, resume and failure injection; remaining planned provider catalog (S3, WebDAV writes, WORM, RBD, iSCSI/FC/NVMe-oF, VM/Proxmox/VMware/oVirt, Kubernetes/CSI, Oracle, HDFS, cloud drives, Alfresco/ownCloud/Nextcloud); genuine two+ Kubernetes pods with shared storage and physical old-writer shutdown; TLS and agent push/pull network failure/cert tests; ACL/xattr identity fidelity, volume scale and GC/parity power-loss recovery; installed Mac/Windows/Linux signed/notarized native apps with rollback and biometric verification; independent security review. Catalog membership never establishes a working/certified adapter. Historical Python archive conversion optional, not release blocker. No automatic publication or merge.



## SQL concurrency qualification closed — source 1d71bbb (15 November 2026)
- Real PostgreSQL 16 acceptance run 43750032081 revealed that a single-statement SELECT FOR UPDATE inside the claim CTE could read a stale READ COMMITTED snapshot and overbook a replica (capacity=1, concurrent accepted=2). We added an actual 8-worker contention test plus eight concurrent identical nonce commits.
- Rust server fixes the race with one transaction and two SQL statements: lock the owning replica row first, then get a fresh capacity snapshot to claim a queued job; commit both atomically. No scheduling bypass and no weaker storage/lease authorization.
- Genuine PG16 tests PASS 3/3 on exact SHA 1d71bbb6998f115097a28b25ddbbdca1e22a7070 in runs https://github.com/desalvo/multi-backup/actions/runs/43750377598 and https://github.com/desalvo/multi-backup/actions/runs/43750382563. Rust focused, POSIX, SSH/SSHFS, database/SQL PITR, Galera, static AFS runner also PASS on this exact source. Main production CI for exact source https://github.com/desalvo/multi-backup/actions/runs/43750382603 still PENDING at audit; signed RC remains blocked by real provider credentials, Kubernetes physical writer fencing, security and signing.
