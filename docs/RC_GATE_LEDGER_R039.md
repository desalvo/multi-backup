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
| New authentic PG16 agent auth and two-replica SQL leasing tests | PENDING workflow run | .github/workflows/agent-cluster-live-pg.yml |

The new PG16 workflow uses an isolated localhost-only SQL fixture and a test-only constructor bypassing production verified-TLS parsing. It certifies SQL state transitions only, not agent HTTPS payload transfer, actual Kubernetes replica pods or physical host/process fencing.

## Remaining release blockers

Live GitHub/LFS/CI and Dropbox cross-account backup/restore with metadata, throttling, resume and failure injection; remaining planned provider catalog (S3, WebDAV writes, WORM, RBD, iSCSI/FC/NVMe-oF, VM/Proxmox/VMware/oVirt, Kubernetes/CSI, Oracle, HDFS, cloud drives, Alfresco/ownCloud/Nextcloud); genuine two+ Kubernetes pods with shared storage and physical old-writer shutdown; TLS and agent push/pull network failure/cert tests; ACL/xattr identity fidelity, volume scale and GC/parity power-loss recovery; installed Mac/Windows/Linux signed/notarized native apps with rollback and biometric verification; independent security review. Catalog membership never establishes a working/certified adapter. Historical Python archive conversion optional, not release blocker. No automatic publication or merge.
