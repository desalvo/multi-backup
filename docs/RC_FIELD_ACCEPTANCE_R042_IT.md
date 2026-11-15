# Multi Backup r042 — field qualification and RC gate ledger (IT)

**Data:** 15 novembre 2026. **Stato:** Rust-native in preview, non RC firmata. Il numero di plugin registrati non indica quanti siano certificati.

## Evidenze controllate

| Area | Evidenza | Esito e perimetro |
| --- | --- | --- |
| Full Production Gate r041 | https://github.com/desalvo/multi-backup/actions/runs/43762169224 | PASS esatto 286bd656, 33 job verdi e 2 pubblicazioni saltate |
| GUI Rust | r041 Production CI, GUI Xvfb, Mac ARM/Intel, Windows e Linux | PASS build/smoke; UI reale accessibilità e signed install non certificati |
| Agent Rust | r041 Production CI + PG16 43750382563 | PASS compilazione pacchetti, Ed25519/nonce SQL/placement; push/pull E2E con disconnessioni e limiti hard ancora aperti |
| Rust HA/server | r041 Production CI job real Kubernetes multi-node kind, PG TLS | PASS due pod, SQL e TLS in cluster; fencing fisico dopo perdita lease, GC multi-writer e operatività esterna ancora aperti |
| POSIX | 43747178037 | PASS FULL/INC, parity, restore in CI; metadata, crash-large-scale open |
| NFS autentico | 43766606434 | PASS server NFS reale, read-only attested mount, Rust encrypted FULL+INC, verify, restore, negatives |
| Samba/CIFS SMB3 | 43766606434 | PASS server Samba autenticato, vero kernel CIFS, Rust encrypted FULL+INC, verify, restore locale; SMB direct RW restore planned |
| GlusterFS autentico | 43766606434 | PASS server e client FUSE reali, Rust FULL+INC, restore isolato; heal/quorum/ACL e snapshot cluster da verificare |
| OpenAFS | 43747477666 | PASS cella MIT Kerberos + vos volume roundtrip; altre modalità agenti/privilegi ancora aperte |
| PostgreSQL/MariaDB/Galera | 43747178038 / 43747178143 / 43747178077 | PASS test isolati logici e PITR; non equivale a produzione unattended |
| Lustre | 43762169090 | PASS kernel Lustre QEMU, MDT/OST e striping; Rust app-aware snapshot/metadata restore aperto |
| Oracle Free | 43762169130 | PASS RMAN PDB backup piece -> age Rust backup/restore -> RMAN validate; no Rust Oracle-native full physical restore |
| GPFS IBM CES | kit PR #15 merged 7bc0f756 | PENDING real IBM Developer Edition field environment, RHEL VMs, proof authentic CES, private CI |
| CephFS | Rust mounted plugin preview | PENDING real CephFS cluster, metadata and snapshot fidelity |
| Other catalogued cloud, SAN, VM, object/WORM plugins | docs/PLUGIN_MATRIX.md | Mostly plan/preview; NOT certified or silently enabled |

## RC STOP conditions

1. Current PR #14 full CI (exact SHA) must finish PASS, along with all real provider jobs it intends to certify; do not transfer PASS from older code without scope.
2. Real GitHub/Dropbox new-format backup/restore and interruption tests; fail-safe resume, large files and metadata fidelity.
3. Live GPFS CES field tests on operator-provided IBM cluster with correct EULA, real snapshot/ACL/xattr/ownership and isolated recovery. CephFS and remote filesystem durability separately.
4. HA physical write fencing, failover in-flight backup, shared-vs-node-local storage enforcement and distributed GC safety.
5. GUI real machine and OS installer acceptance on Mac/Windows/Linux, accessibility, signed/notarized packages, stable updates and rollback.
6. Agent host physical limits, bad key/replay, network faults, remote SSH and push/pull under concurrency. Independent security review.

No public self-hosted GPFS runner: private coordinator sample scripts/gpfs-lab/PRIVATE_WORKFLOW_TEMPLATE.yml. Developer edition IBM software and credentials never appear in repo or public artifacts. Full docs: docs/GPFS_DEVELOPER_LAB_R042_IT.md.

No automatic stable tag, Docker release, broad provider Ready promotion, or merge of unfinished Rust code.
