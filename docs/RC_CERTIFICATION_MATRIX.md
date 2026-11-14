# Multi Backup r021 — certification and release acceptance matrix

**Audit date:** 2026-11-14. **Source:** PR #9 merged to main in commit
`b7e757d9378d2503b57fa43267577a3a55f60fd2` after full green feature-head CI 43640869549.
**Disposition: NOT CERTIFIED. No stable release permitted.**

This file is a machine-auditable checklist of required evidence; a green
compilation, local test suite, or prototype Docker image does not certify the
complete backup appliance. Preserve the working r018 Python/Tk release assets while the merged Rust code remains experimental.

| Gate | Requirement for certification | Verified evidence/status |
| --- | --- | --- |
| Production gate | Python/Rust tests, Ruff/Bandit, dependency audits | PASS on feature head 860d390a, run 43640869549 (19/19 required verification jobs, 0 failed) |
| Storage v1 | Dedup, age, parity, fault injection, verify and restore | Code/test added; not formally qualified |
| Portable ZIP64 | Integrity-checked multi-volume export/import and retry | Code/test added; needs large-volume qualification |
| Logical browse | GitHub, LFS, CI, metadata, Dropbox under ONE backup | Rust manifest and responsive web prototype; provider E2E pending |
| Complete native Rust | No Python runtime in installer or provider actions | Rust preview built in prior revisions; complete parity not proven |
| Desktop platform CI | Mac ARM/Intel, Windows, EL8/9/10, Ubuntu, Debian 12/13 AMD64/ARM64 | PASS on feature head 860d390a, run 43640869549; local installed-app and live-provider qualification still pending |
| Production app signature | Apple Developer ID + notarization, Windows Authenticode, signed RPM repo | Credentials/hardware validation unavailable |
| Registry | Multi-arch Docker Hub with SBOM/provenance, architecture runtime tests | OCI build works, push unverified/not configured |
| Kubernetes | Helm/Kustomize render, PVC, TLS, NGINX/HAProxy, wizard, upgrades, backup persistence | Config added; deployment and upgrade acceptance pending |
| Identity/auth | Local + LDAPS + OIDC SSO, roles, account lockout, TLS | Argon2id local prototype; LDAP/OIDC absent |
| SMTP(S) and audit | TLS certificate check, immutable audit, retention, retries | Minimal JSONL audit; SMTP absent |
| Desktop mount | Read-only FUSE/WinFsp mount with on-demand chunk reads, unmount and symlink controls | Not implemented |
| Legacy data conversion (optional) | Python-made archive migration is no longer an RC gate by user decision | Optional conversion paths retained; not a blocker for native functional certification |
| Packaging | One readable logical backup; original branded images; manuals/photos | Original artwork source, PDF/manual regeneration; review pending |
| Live provider acceptance | Real GitHub/Dropbox backup & restore, rate limit, interruption, 5 snapshots | Not run; private credentials unavailable |
| Durability | Independent full restore, no data loss during crash, parity corruption, key loss drills | Needs long-running test/certification |
| Security | Threat modeling, SAST/SCA, secret handling, penetration test, restore sandbox | Initial gates; independent assessment pending |

## Publication interlocks

- Any PR: tests, previews and OCI build only; **never** registry push.
- Early on main + `[full-ci]`: release only after **all** native Rust
  previews, server, security, and publication jobs pass.
- Stable tag: requires `ENABLE_STABLE_RUST=true` explicitly **in addition**
  to the signing and native gate. Do not configure it prematurely.
- Docker stable additionally requires `ENABLE_STABLE_CONTAINER=true`.
- Docker Hub needs GitHub Actions `DOCKERHUB_USERNAME=desalvo` (variable),
  `DOCKERHUB_TOKEN` (secret). Do not include tokens in source, logs or chat.

## Acceptance evidence to attach

Record exact Git commit SHA and GitHub Actions run/job URLs, OS versions,
SHA-256 hashes of installers, notarization/ticket and Authenticode outcomes,
a verified roundtrip restore digest, corruption/recovery results, package
manifest SBOM and provenance. Complete the checklist above only when each
external requirement is actually performed and independently observed.

### Critical note

A *single logical backup* contains all applicable repositories, LFS,
CI artifacts, metadata and selected Dropbox content. Chunk and stripe
objects are storage internals, **never separately presented as user backups**.

Do not treat signing/build compliance as equivalent to full backup
recoverability or an external security certification.


## r022 newly implemented and pending external qualification

- Read-only health verification of every encrypted chunk plus parity checks.
- ZIP64 volumes of encrypted content-addressed objects, with encrypted catalog and SHA-256 validation before atomic import.
- Non-destructive streaming import of multiple legacy age/tar archives into a **single** logical backup.
- Multi-selection restore and responsive file-manager-like web browse.

These features require full post-merge qualification before marking them production certified. Reverse conversion, production native signing, live mount, OIDC/LDAPS and SMTP remain explicit release blockers.


## r023 outstanding integration certification

The native Rust storage pane, relocated store, SMTP(S), LDAPS and OIDC
backends have been implemented as development code. Their presence is
not certification. Validate latest CI, OIDC issuer/JWKS rotation/nonce,
LDAP directory group membership and TLS, SMTP actual delivery and
certificate rejection, external identity authorization and audit
before enabling remote production access.

Live filesystem mounts, in-memory age session interoperability and
direct provider-to-chunk ingestion remain absent; stable promotion is blocked.


### r024 experimental mount

WebDAV read-only mount endpoint implemented as development code, with streaming verified encrypted-chunk access and separate Basic-auth token. **Unverified against OS clients; security and restore acceptance remains open.** Native FUSE/WinFsp mount is still absent. Do not enable auth.dav or treat it as a production mount until verified.


## r025 conversion acceptance

The code path for v1 -> legacy archive export (streaming) and legacy -> v1 import is implemented and has a round-trip test. **Certification pending** on actual GitHub, Dropbox, LFS and large datasets. Do not delete original archives until independent restore and checksum validation pass.


## r027 -- Debian and private architecture qualification

Native Debian 12 Bookworm and Debian 13 Trixie x AMD64/ARM64 must each
pass cargo compile, independent .deb dependency resolution, installation,
native CLI version, GUI self-test, byte-identical extraction and SHA-256.
Their release outputs must include all four .deb files.

Private design confidentiality: verify a password-chosen AES-256 R6 PDF
is locally generated without intermediate plaintext files, rejects incorrect
passwords, is never disclosed in GitHub Actions, and only encrypted bytes
are optionally added. The public repository must contain **no private
diagram source or live topology**. This is a process safeguard, not
an international security certification or resistance guarantee.


## r028 — evidence and remaining security work

- CI run 43625695818 at a10a40e8: Production Gate, OCI compilation, Mac ARM64/Intel DMGs, Windows x64 installer, Ubuntu GUI, Debian 12/13 x AMD64/ARM64 and six Rocky/Alma RPM previews all passed. No official release publishing took place from the draft PR.
- New security delta: refuse public-origin insecure startup, reject copied gate/recovery keys, verify authentication boundary in a **running** OCI container and require this check in release policy. New CI is required before marking runtime gate verified.
- Remaining for production: reverse-proxy removal of untrusted gateway headers, enforced private mTLS/CNI network policy, WORM Object Lock, 2-of-3 independent recovery approvals, signed audit, external provider restores and independent penetration test. Unit and container acceptance do not substitute these.


## r029 — universal plugin acceptance extension

New scope adds 18 backup input roles and 7 output roles, reversed for restore; see `docs/PLUGIN_MATRIX.md`. **None** is yet certified as a unified bidirectional generic plugin pair. Existing GitHub/Dropbox/POSIX direct Rust preview operations remain outside the generic dispatch. Every plugin must prove source consistency, inventory and metadata fidelity, verified encrypted transport, retry/resume, manifest-last atomicity, create-new restore with explicit conflict handling, least-privilege tokens and real test-service recovery. VM/CT, RBD, database/PITR and Kubernetes/CSI drivers require application-aware snapshot consistency and restore acceptance; not interchangeable with POSIX copy. Static catalog is a contract, not authorization to operate remote services or release a stable build.

## r030 — SAN and anti-ransomware plugin acceptance

**Registry ≠ implementation.** The expanded 21-source/11-destination matrix
covers iSCSI, Fibre Channel and NVMe-oF as both backup input and output,
with an anti-ransomware ciphertext vault as backup output / restore input.
No generic SAN adapter or actual WORM writer/reader has passed live tests.
Required evidence: frozen/quiesced snapshot read, correct initiator/target
and namespace identity, multipath consistency, no mounted writable
production target, crash injection, end-to-end byte/metadata restore to a
new isolated target, append-only ingest and vault retention protected
against compromised operator credentials, read-only recovery account,
independent approval and audit. The anti-ransomware vault must refuse
deletion/truncation/shortened retention despite stolen ingestion credentials.

## r031 — GlusterFS and SAN LVM profiles (2026-11-14)

- Added `glusterfs` as both input and output of backups, with inverse
  restore roles. Distinct plugin ID; requires mounted Gluster client,
  topology and consistency verification and preserves GFID/ACL/xattr.
- iSCSI and FC each support independently configurable optional LVM layouts:
  `local`, `cluster-local`, `shared-exclusive`, `ha-exclusive`,
  `cluster-shared`. The raw-LUN mode remains available by omission.
- New read-only Rust `plugins plan --input-lvm/--output-lvm` validates
  the profile independently per endpoint and emits snapshot, multipath
  WWID/PV/VG/LV, cluster quorum, fencing/locking and isolated-new-target
  acceptance requirements. Misapplied profiles fail closed.
- Added Rust unit tests for all 5 LVM profiles on both transports,
  inverse restore mapping, rejection on non-SAN plugins, clustered
  locking/fencing guardrails and GlusterFS both directions.
- Updated scope: **23 plugin IDs, 22 backup sources and 12 destinations**,
  34 backup-role contracts plus 34 inverse restore contracts.
  Catalogue coverage: 34/34 (100%); unified certified role adapters:
  0/34 (0%); certified generic provider-pair combinations: 0/264 (0%).
- Do NOT claim that catalogued SAN LVM, GlusterFS or WORM operations are
  implemented, or that unsafe live-device writes are supported.

## r032 — AFS and enterprise storage/content plugins (2026-11-14)

- Added **AFS / OpenAFS** as two-way plugin, with AFS cell/volume/VLDB,
  Kerberos token, backup/readonly snapshots, full/incremental `vos`
  chain, ACL/mountpoint preservation and restore to **new** volume preflight.
- Added Lustre, IBM Storage Scale / GPFS, HDFS, Alfresco, ownCloud and
  Nextcloud as *planned* input/output plugin roles; extended existing
  Google Drive and OneDrive inputs to also have output roles.
- Added access-gate metadata to the Rust contract: licensed external
  client for GPFS, OAuth scope review for Google Drive/OneDrive, and
  externally installed mounted/API clients for other integrations.
  No proprietary runtime, kernel module, OpenAFS code, or credentials
  have been packaged. Vendor license and API rights require acceptance.
- Plan preflight adds provider-aware AFS volume/ACL/dump-chain rules,
  HDFS TLS/Kerberos/snapshot, Lustre/GPFS snapshot and storage
  entitlement, remote document version/permission checks.
- Added Rust unit tests for all new bidirectional roles, AFS restore
  isolation, licensing/OAuth gates and fail-closed pairing.
- New inventory: **30 plugin IDs, 29 backup inputs, 21 backup outputs**
  and exact inverse restore roles (21 inputs/29 outputs); **50 declared
  backup roles and 50 inverse**, zero generic pairs certified of 609
  possibilities. Catalogue definition 50/50 (100%) is *not* product
  completion. Existing older overall RC percentage must be rebaselined.
- No unqualified vendor adapter is enabled; latest CI must verify this
  commit before declaring the new code/test matrix green.

## r034 — SSHFS and open CES GPFS client support (2026-11-14)

- Added bidirectional `sshfs` provider, distinct from `ssh` (SCP/SFTP
  transfer). Client is an independently installed, patched GNU GPL-2.0
  SSHFS/FUSE package, not bundled into Multi Backup.
- Switched GPFS plugin to **CES NFS/SMB profiles** `ces-nfs` and
  `ces-smb`, using open-source distribution client tools; IBM Storage
  Scale server/CES nodes retain their own license requirements.
  Direct native GPFS mount via a free client is not assumed available.
- Added Rust `mount_probe` module and read-only CLI `plugins probe`,
  checking Linux exact pre-mounted NFS/CIFS/FUSE.SSHFS mountpoint and
  filesystem type without mounting, file reads, authentication or writes.
  Kernel rw flag does not prove access privileges/GPFS provenance.
- CLI `plugins plan` now supports input/output GPFS CES protocol profiles
  separately, with preflight for consistent snapshots, ACLs, patched SSHFS,
  host-key pinning, symlink containment, isolated encrypted-only output.
  All newly cataloged adapters remain `planned`.
- Inventory **31 plugin IDs, 30 backup inputs, 22 backup outputs**, reverse
  restore roles 22/30. Declarative contracts 52/52, certified generic
  adapter roles 0/52, qualified provider pairs 0/660; rebaseline overall
  completion before reporting a new product-wide percentage.
- No change to main/stable. Production CI for this commit must complete.

## r036 — user-controlled protection of individual backup instances

- Native content-addressed logical backups can be **protected/unprotected**
  explicitly. Protection state is an age-encrypted per-backup marker, not
  8+2 parity and not the WORM vault. `Store::load/list` now surfaces
  protection status, with corrupted marker fail-closed.
- New CLI commands `storage protect-backup/unprotect-backup/delete-backup`.
  Unprotect and delete require exact full ID confirmation. Protected backup
  deletion is rejected. Manifest deletion deliberately does **not** remove
  shared ciphertext chunks/parity pending a safe, independently verified GC.
- Desktop Storage displays a protected lock and offers Protect, confirmed
  Unprotect and confirmed Delete only when unprotected.
- ZIP64 transport and full-storage relocation carry the encrypted hold,
  preserving the protected status across imports or copies.
- Legacy encrypted `.tar.gz.age` archives gained a separate per-archive
  protection marker with CLI `archive protect/unprotect`, and deletion
  is blocked while protected. Mutation uses an exclusive root lock.
- The policy-only retention planner honors an explicit `protected` flag
  regardless of age or default 30-day expiry. Autonomous recurring
  retention and GC remain **not implemented**; no automatic deletion has
  been enabled by this change.
- Added integration/unit tests for shared chunks, protected ZIP import,
  corrupt holds, wrong-ID rejection and protected legacy archive deletion.
- No claim of immutable hardware WORM protection or completed remote plugin
  enforcement; independent vault and provider certification remain pending.

## r036 corrected inventory — Oracle and explicit protection

- `oracle` is registered as a planned consistent-online RMAN input,
  taking the plugin inventory to 32 plugin IDs, 31 backup input roles and
  22 backup output roles; restore is the exact reverse (22 / 31).
  **53** backup role definitions are present, and **682** possible pairs
  are not yet certified as generic integrated jobs.
- New user-requested protection is separate from 8+2 parity and WORM:
  encrypted per-backup hold survives portable transport/relocation,
  blocks manual logical delete and retention preview. Legacy local
  archives have their own guarded protection sidecars. The runtime
  scheduler and safe chunk GC are not yet certified or activated.

## r037 — nested top-level namespaces and retention preview

- The chunk store now offers `Store::backup_in_namespace` and CLI
  `storage backup --top-namespace PATH`; GUI has an optional namespace
  field supporting nested segments (e.g. `Rome/Cluster/VM`).
  Existing `Store::backup` and manifests remain backwards compatible.
- New manifests store creation Unix time and optional top namespace.
  Migrated/legacy manifests with unknown timestamps are preserved
  indefinitely by automatic retention until classified explicitly.
- Added `Store::preview_retention` and CLI `storage retention-preview`:
  read-only age-authenticated snapshot load/integrity checks followed by
  the 30-day retention policy. The protection flag is read directly
  from encrypted hold state, NEVER trusted from user-provided records.
- Test checks nested source paths, traversal rejection, future 30-day
  expiry preview still preserves a protected backup, and restored data
  under the nested namespace. Rust focused CI `43638279897` passed
  all 3 jobs for commit `901608a5`; full CI on that revision is pending.
- Automatic scheduler execution and real mark-and-sweep GC are NOT
  enabled; protected shared chunks may not be reclaimed by an unsafe
  age-based cleanup. Output plugin-level capacity is partially supported
  for pre-mounted filesystems; cloud API quotas/namespace index
  persistence still require integration and live verification.


## r038 — merge qualification evidence (2026-11-14)

PR #9 was merged to `main` as `b7e757d9378d2503b57fa43267577a3a55f60fd2`. The premerge feature head `860d390a` passed all **19 required verification jobs** in [CI #43640869549](https://github.com/desalvo/multi-backup/actions/runs/43640869549); **0 failed**, **2 publication jobs skipped** by PR safety policy. Artifact groups include Rust-native macOS ARM64 and Intel DMG, Windows x64 installer/ZIP, Ubuntu desktop, Debian 12/13 AMD64/ARM64, Rocky/Alma EL8/9/10, GUI/PDF previews and compatibility macOS images. Tests cover compiled CLI encrypted archive byte-for-byte recovery, sidecar tampering, wrong key, protected-backup deletion and namespace roundtrip. No update, physical installation, external credential or live provider restore acceptance was inferred from this result.

**Disposition remains NOT CERTIFIED for stable production.** No stable signing/notarization, real GitHub/Dropbox/LFS/CI restore drill, large-scale performance, WORM recovery, or independent security audit is evidenced; the runtime scheduler and safe mark-and-sweep garbage collector have not been implemented/certified. Never treat this document's older historical FAIL status lines as the latest run; the most recent premerge CI above supersedes those historical CI states, while outstanding functional blockers still apply. The merge commit has no `[full-ci]` publication trigger, preserving published r018 assets. Follow-up main CI and all required external acceptance must still be recorded before any RC promotion.


## r038 release policy override — functional Rust first

The user confirmed Python-produced archives are not in critical active use and **old archive format migration is not a release blocker**. Keep optional conversion source and existing files, without automatic deletion. Require instead native encrypted backup/verify/full and selective restore, real live GitHub/Dropbox account acceptance (LFS, CI metadata, throttling and resumable jobs), genuine native GUI functionality, protected retention/GC, installed upgrade/rollback and production signing. Rust preview packaged binaries exist but are not yet a certified RC.

A narrow common Rust plugin execution path is now coded: POSIX→POSIX and Linux read-only mounted NFS/SMB/SSHFS/GPFS CES→POSIX encrypted store, followed by POSIX→POSIX verified restore. Remote snapshot consistency/ACL/xattr fidelity are **not proven** by a mount probe or attestation flag. These are previews, not production Ready adapters. PR #10 CI must succeed before merge.


## r038 — incremental rebase and ciphertext GC safety gate

New Rust functionality on draft PR #10 must pass:
1. Full F0 -> I1 -> I2: delete I1, reparent I2 to F0 while preserving byte-
   identical restore and complete chain verify.
2. Delete F0 with I1/I2 surviving: I1 becomes a full, I2 remains a linked
   incremental, with no plaintext copy or data loss.
3. Protected successors: refuse their implicit metadata rewrite, require
   explicit unprotect; corrupted ancestor/child likewise blocks purge.
4. Purge never removes physical chunks or parity. GC preview never mutates.
5. GC verifies ALL remaining manifests+parity under lock, observes 48h default
   grace, deletes only fully dead parity groups and nonreferenced ciphertext,
   and leaves mixed live/dead groups untouched for a future safe repack.
6. Restore after GC must be byte-identical; multi-OS real age roundtrip,
   error injection, interruption/recovery, mixed stripe compaction and
   independent recovery certification remain release gates.

The current local GC is intentionally conservative. **Maximal disk
reclamation for partially live 8+2 parity stripes is NOT yet implemented.**
Unattended remote retention/GC and filesystem WORM are unqualified.
Do not advertise the r038 draft as a certified or released RC.