# Multi Backup — functional inventory r038 (Rust preview)

**Date:** 2026-11-14. **Source:** draft PR #10, development branch.
**Status: NOT CERTIFIED for stable production or a signed RC.**
This inventory distinguishes source-code implementation, CI acceptance, real
provider usability, documentation and missing security qualifications.

## Interpretation

- **Implemented:** code path and local test already exist; may still require external acceptance.
- **Preview:** operational development code exists but integration, environment, failure recovery, complete UI or qualification is outstanding.
- **Planned:** only architecture/catalog/contract, no full adapter.
- **Blocked:** release cannot be promoted until this evidence is attached.

| Status | Functional area | Implemented capability and remaining evidence |
| --- | --- | --- |
| Implemented | Rust-native CLI and GUI binaries | Source compiled in past CI matrix on macOS/Windows/Linux; current r038 must pass its own CI |
| Preview | Encrypted local store | age + zstd + HMAC-SHA256 deduplicated chunk objects; full restore of test fixtures; large-scale endurance and real recovery pending |
| Preview | 8+2 parity and recovery | Reed-Solomon over ciphertext and encrypted shard metadata; two-shard failure tests; power-failure fault injection pending |
| Preview | Full and incremental | Explicit manual mode; incremental parent history; manifests contain complete file inventories and immutable shared chunk refs |
| Preview | Chain verification | Full-chain-by-default health check, optional selected-only; scheduled checks run with user-chosen intervals |
| Preview | Size reporting | Files and logical bytes, pre-dedup and post-dedup zstd sizes, newly referenced compressed bytes, ordered full/incremental history |
| Preview | Selective local restore | Authenticated fresh-directory restore of whole logical tree, file or selected folder path |
| Preview | Deletion and rebase | Child increment reparenting or promotion to full; refuse protected children; shared chunks never deleted by logical purge |
| Preview | GC and automatic 8+2 parity repack | Separate confirmed GC; mixed-stripe repack writes/validates new parity BEFORE publishing successor index; 48-hour grace on replacement; orphan preparatory parity cleanup; crash/power-loss certification pending |
| Preview | Durable Rust scheduler | Native one-shot and daemon, full/incremental modes or full-chain check jobs; configuration via JSON/CLI, GUI management and dynamic latest-backup tracking incomplete |
| Preview | Five generic backup pairs | POSIX→POSIX and Linux read-only NFS/SMB/SSHFS/GPFS CES→POSIX encrypted store; mounted snapshot identity/ACL acceptance pending |
| Preview | GitHub dedicated engine | Rust Git mirrors, metadata/CI/LFS inventory; live provider backup, full recovery and throttling still to be independently certified |
| Preview | Dropbox dedicated engine | Rust read-only incremental file/revision source and new-file write recovery; token scopes and real-account E2E pending |
| Preview | Cross-account restore | Source/destination identities independent; GitHub new-owner repo creation, Dropbox target write-token account check; arbitrary partial cross-account recovery incomplete |
| Preview | Encryption key vault | age identity management and protected encrypted vault in native GUI/CLI; platform biometric, restore after recovery, signing still pending |
| Preview | Portable export/import | ZIP64 parts, integrity and age migration/export code; large/partial volume acceptance and rollback pending |
| Preview | Desktop user experience | Approved photographic GUI, hamburger, About, progress animation, Start/Resume and storage page; functional desktop quality/accessibility acceptance incomplete |
| Preview | Server read-only WebDAV | Optional HTTPS/gateway experimental code; live OS mounting/security hardening not certified |
| Planned | Remaining generic plugin pairs | 32 registered IDs, 53 directional backup role declarations, 682 potential pairs; **0/682 production certified** |
| Planned | Remote cloud-to-cloud and DB/VM/SAN | S3/WebDAV/cloud/database/hypervisor/SAN generic writers and consistent restore adapters, many catalog-only |
| Planned | Remote GC and immutable WORM | Strong authorization/independent write-protection, vendor Object Lock, remote safe purge and audit |
| Planned | Native GUI schedule management | Editable/persisted jobs, per-plugin UI scheduling, dynamic newest incremental for verification |
| Planned | Crash-atomic multi-manifest purge | Multi-child rebase journal with replay/rollback, security and corruption fault injection |
| Blocked | Signed stable or public RC | Green current SHA across full CI, live provider E2E, app installation, signing/notarization, verified update/rollback and independent security review |

## Release-relevant behavior

**Purge** only updates/drops authenticated manifests; physical age ciphertext is
never deleted in this operation. Protected instances and descendants are
fail-closed. **GC** separately identifies live object references, verifies
all surviving backups and removes only grace-eligible orphan content.
**Automatic parity repack** belongs to GC: it prepares and verifies a new
8+2 stripe from live members of a partially obsolete stripe, commits a
new encrypted index last, and retains the old stripe until the successor
passes the grace period. The first GC may therefore reclaim little or no
space; a later one completes retirement. This remains development preview.

**Size fields** are zstd-compressed bytes before age/parity overhead; physical
GC reclaim counts actual disk ciphertext and parity. User-facing graphs must
not confuse those two kinds of quantities.

**Cross-account remote restore** may target another GitHub owner/Dropbox
account with *independently authorized destination credentials*. A source
account identifier never grants write access and must not carry secrets.
Today GitHub creates one new full mirror repository and Dropbox restores a
single encrypted file at a time; general arbitrary cross-user selection is
not yet a certified generic operation.

**Python archival migration is optional**, by user decision. Its incomplete
byte-format parity is not a Rust release blocker; however Rust functional
backup/restore, privacy, data durability, runtime and installer acceptance
remain blocking.

## Operational references

- [Manuale operativo Rust (italiano)](OPERATIONS_R038_IT.md)
- [Rust operations manual (English)](OPERATIONS_R038_EN.md)
- [Storage and parity design](CHUNK_STORAGE_V1.md)
- [Maintenance, retention and GC](MAINTENANCE_POLICY.md)
- [Provider and authentication contracts](PLUGIN_API.md)
- [Plugin matrix](PLUGIN_MATRIX.md)
- [Protected backups and holds](BACKUP_PROTECTION.md)
- [Security boundaries](SECURITY.md)
- [RC acceptance criteria](RC_CERTIFICATION_MATRIX.md)

**Documentation rule:** every behavior change must update this inventory,
the affected procedure in both languages, the corresponding engineering
reference, the development status/handoff, and the current acceptance tests.
The Production Gate executes check-documentation-consistency.py to detect
missing coverage, but that check does not certify functional correctness.

## r039 overlay — additional Preview / Blocked features

| Status | Feature | Actual progress / missing proof |
| --- | --- | --- |
| Preview | AFS local/SSH volume dump | Validated vos volume dump plans and stdout streaming; keytab, localauth and restore on real cells outstanding |
| Preview | Authenticated native agent | Rust CLI push/pull, age encrypted spool, PG registry, admin approval, token/Ed25519 and nonce replay check; actual agent/server E2E outstanding |
| Preview | Resource-limited parallel volumes | Max job workers, threads field, bytes/s throttling, cooperative CPU/RAM limits, stricter server vs agent policy. Hard limits and SSH remote probe outstanding |
| Preview | PostgreSQL multi-replica orchestration | TLS verify-full, shared registered storage inventory, node-local affinity, SKIP LOCKED lease/heartbeat, opt-in filesystem worker; physical write fencing after lease loss outstanding |
| Preview | Unified replica browser sessions | PostgreSQL shared login/logout; pre-login OIDC state and cross-replica local restore proxy remain pending |
| Preview | Agent native installers | macOS arm64/x86_64 PKG, Windows x64 EXE, Debian 12/13 amd64/arm64 DEB, Rocky/Alma EL8/9/10 x86_64 RPM CI jobs, with services disabled until approval |
| Blocked | r039 RC readiness | Real Kubernetes 2+ replica failover/fencing, real AFS full/incremental recoveries, signed installers, load/memory benchmarks and security audit required |

The r038 base inventory below is historical for these r039 extensions. The source of truth is [CLUSTER_ORCHESTRATION_R039_IT.md](CLUSTER_ORCHESTRATION_R039_IT.md) and [CLUSTER_ORCHESTRATION_R039_EN.md](CLUSTER_ORCHESTRATION_R039_EN.md).
