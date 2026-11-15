# Multi Backup plugin architecture — Rust native contract v1

The **source** and **destination** of each backup are plugins. A restore reverses
the direction of the same two plugins. Four separate capabilities are
advertised for each plugin: `backup_input`, `backup_output`,
`restore_input`, `restore_output`. A plugin's *registration* is not a
claim of implementation, live-account interoperability or release readiness.

## Rust contract and discovery

The authoritative, static, auditable registry is `rust/src/plugins.rs`.
The existing legacy Python registry (`backends.available()`) and native
`github backup` / `dropbox backup` commands remain functional and unchanged;
they are **not** dynamically executed by the new planner. Plugin IDs are stable
and independent of OS. No arbitrary local DLL, script, shell command or
downloaded plugin package is ever discovered/executed.

```bash
multi-backup-native plugins list --json
multi-backup-native plugins list --role input --mode backup --json
multi-backup-native plugins list --role output --mode restore
multi-backup-native plugins plan --input github --output posix --mode backup --json
multi-backup-native plugins plan --input posix --output github --mode restore --json
```

`plugins plan` is **read-only**. It rejects unknown IDs, unsupported
directional roles and invalid modes; `executable=false` until fully qualified
production adapters are certified. `preview_executable=true` now identifies
an independently gated **real Rust development executor**, not a production
Ready capability. `preview` means native code exists but is not certified. `planned` means
only the interface specification/catalogue exists. `unsupported` is an
explicitly forbidden role. `ready` is reserved for live-tested, qualified
native adapters (none currently declared).

## Responsibilities of the two plugin families

**Input plugin** (backup source / restore destination):
- Produce an authoritative point-in-time source inventory and stable object
  IDs, normalized paths and source-specific consistency token.
- Read source data via bounded streams to the authenticated encryption core;
  never stage entire plaintext backups in a world-readable directory.
- Describe all recoverable metadata (owner, mode, ACL, xattrs, sparse extents,
  symlinks, storage identifiers, database roles, VM/cluster configuration).
- Restore only after validated destination identity/role, explicit approvals
  for destructive actions, conflict policy, object integrity verification and
  a safety checkpoint; default is **create-new**, not overwrite.

**Output plugin** (backup destination / restore source):
- Accept **encrypted** chunks/objects and authenticated manifests only; the
  output layer must never receive age private keys or plaintext credentials
  for a source provider.
- Use a durable two-phase upload: begin → stream/verify chunks →
  publish signed/encrypted logical manifest **last**. Aborted uploads remain
  uncommitted and invisible as complete backups.
- Expose immutable/append-only/versioning capabilities, object checksum,
  retries, constrained parallelism and resume tokens. A remote deletion must
  never be implied by backup retention or failed resume.
- Restore by enumerating verified manifests/chunks to the local decrypting
  core; one user backup remains **one logical backup**, not multiple objects
  shown as independent backups.

The generic engine is in partial development: five preview filesystem source→POSIX pairs execute through the common Rust runtime; the majority of catalog combinations remain declarative only. Every adapter must respect encryption, permissions, vault, audit, retention and two-phase commit semantics.

## Security / platform boundaries

- Credentials are per-plugin, least privilege, stored in OS keyring/secret
  store; no secrets in manifests, Git, logs, plugin metadata, CLI arguments,
  or private handoff. Always explicit account/path/tenant scope.
- POSIX/NFS/SMB/CephFS metadata and snapshot semantics differ. NFS supports
  v3 and later, but consistency may require a storage-side snapshot.
- RBD is **Ceph RADOS Block Device** and operates on image snapshots/diffs,
  not pathname traversal.
- Proxmox plugin includes VE configs, VM, CT, volumes and PBS inventory/restore.
  Kubernetes includes OpenShift/OKD, API objects, CRDs, CSI snapshots and
  guarded etcd scope. VMware/oVirt require hypervisor-specific snapshot APIs.
- MySQL/MariaDB and PostgreSQL require application-consistent backup plus
  binlog/WAL and point-in-time restore gates; raw copied data directories
  alone are not sufficient evidence of recoverability.
- Never assume cloud API scopes include passwords/secret values: record
  missing non-exportable material for manual recreation during recovery.
- No write-capable plugin automatically executes deletions, overwrites,
  VM starts/stops, Kubernetes replacement or database import. Preflight and
  operator-approved restore plan required.
- Preserve provenance and public manifests for safe external contributions;
  independently signed/sandboxed plugin distribution is a future gate.

## Required acceptance before flipping a provider from Planned to Ready

Provider-specific mock + fault injection; invalid/replayed credentials;
checkpoint/restart; remote throttling and dropped sessions; large file/hash
verification; backup and restore to **new** target; metadata/snapshot
consistency; multi-OS packager validation; real service API access using a
test account; negative tests for overwrite/delete and cross-tenant isolation.
S3/CEPH WORM requires independent, external verification that an ingest
principal cannot rewrite a committed object or shorten retention. No vendor
adapter should reach Ready based on a compile-only green CI.

## SAN block transports and anti-ransomware vault (r030)

The canonical native catalog additionally declares `iscsi`, `fc`, and
`nvmeof` as **both** backup input and output, with symmetric restore
roles. These are storage *transports* to a LUN/namespace and not substitutes
for the `rbd` Ceph volume driver or POSIX filesystem copy. Their
`safety=snapshot_block_device` metadata identifies non-negotiable
preflight requirements: stable WWID/NGUID and LUN/NSID identity, multipath
reconciliation, a verifiable snapshot boundary or application quiesce,
bounded block-stream validation and safe detach/cleanup. Restore and backup
output write operations are always blocked until the operator has a new,
empty, isolated target with independent approval. Current stage: **Planned**.

`anti-ransomware-vault` is **backup output only** and **restore input
only**, exposing `safety=immutable_recovery_vault`. It accepts ciphertext
plus authenticated manifest, with independent append-only ingest and a
separate read-only recovery principal. WORM/Object Lock, enforced retention,
external audit and multi-party approval are mandatory before Ready. The
current Rust content-addressed store does not by itself meet those conditions:
**this is an interface and acceptance contract, not a working WORM service**.

Counts: **21 input + 11 output = 32 backup-role definitions** across 22
plugin IDs, reversed for restore. See `docs/PLUGIN_MATRIX.md` for status
and the 0% generic-adapter certification baseline.

## r031 — GlusterFS and LVM over iSCSI / FC

New provider `glusterfs` is registered for backup input/output and the
inverse restore roles. Use a mounted Gluster client, never brick-path
manipulation. Snapshot consistency and preservation of volume metadata,
GFID, extended attributes and ACL require explicit validation against
replica/disperse configurations; heal/split-brain and durable commit must
be tested before enabling writes.

The `iscsi` and `fc` plugins now advertise five *optional* typed
`lvm_modes` in JSON: `local`, `cluster-local`,
`shared-exclusive`, `ha-exclusive`, `cluster-shared`.
Raw LUN is represented by an omitted profile. CLI `plugins plan` accepts
`--input-lvm` and `--output-lvm` independently, and refuses to apply
LVM modes to non-iSCSI/FC plugins. A shared or clustered LVM profile adds
read-only preflight requirements for WWID, PV/VG/LV identity, device
ownership, snapshot consistency, single-writer fencing or distributed
locking, and quorum. An output/restore target must be a *new empty isolated*
LUN/VG. The planner does NOT acquire locks or perform any block operation.

Compatibility note: Proxmox supports LVM over iSCSI and FC, including
shared storage under the correct cluster lock model, but LVM snapshots,
VM volume chains and cluster-aware filesystems differ. A successful
capability plan does not establish crash/application consistency.

## r032 — Lustre, Storage Scale/GPFS, HDFS, AFS, Alfresco and cloud repositories

The registry now includes 30 plugin IDs and 29 backup source / 21 backup
destination roles (reversed for restore). `google-drive` and `onedrive`
are now also *planned* destinations. New two-way plugins: `lustre`,
`gpfs`, `hdfs`, `afs`, `alfresco`, `owncloud`, `nextcloud`.
All are static, never dynamically imported from arbitrary plugin folders.

New `DataKind` values `distributed_filesystem` and
`document_repository` distinguish namespace/volume consistency and
document metadata. Declarative `access_gate` is either
`external_protocol_or_mounted_client`, `licensed_external_client` or
`oauth_scope_review`. A gate lists prerequisites in `plugins plan`;
it never downloads a third-party driver, requests OAuth consent or
activates a mount automatically.

- IBM Storage Scale licensing: operator-provided and separately installed
  authorized client; backup/transfer entitlements must be verified.
- OpenAFS: external IPL-licensed client, AFS cell/VLDB volume identity,
  backup/RO snapshots, chained full/incremental dumps, separate ACL
  preservation and create-new-volume recovery.
- Lustre: external mounted client, layout/stripe and snapshot consistency.
- HDFS: SWebHDFS, Kerberos and secure redirect policy.
- Alfresco: REST/CMIS documents, versions, model, permissions and links.
- OwnCloud/Nextcloud: independent DAV/OCS compatibility and revision tests.
- Google Drive/OneDrive: OAuth tenant/scope consent and create-new encrypted
  object output; no overwrite or silently upgraded OAuth scope.

Licensing compatibility for *redistributed binaries* needs an independent
review before packaging any vendor code. Multi Backup itself remains
EUPL-1.2 and is deliberately not shipping vendor runtimes in this commit.
All providers are `planned`, not implemented or approved for production.

## r034 — GPFS via CES (NFS/SMB), SSHFS as an independent plugin

GPFS uses operator-mounted open-protocol exports, not IBM native GPFS
client code. `GpfsAccess` supports `ces-nfs` or `ces-smb`; CLI
`plugins plan` accepts independent optional `--input-gpfs-access` and
`--output-gpfs-access`. The licensed server exposes the share, while the
client may use OS-maintained NFS/CIFS components without GPFS Client licensing.
Check the actual IBM licensing agreement and CES server entitlement.

SSHFS is **distinct from SSH/SFTP/SCP**. This plugin declares input/output
via an already mounted, updated SSHFS/FUSE client supplied separately by
the operating system. SSHFS is GPL-2.0; Multi Backup does not distribute or
statically link this package. Enforce SSH host key pinning and do not accept
insecure directport options or symlink traversal; remote snapshot consistency
requires server-side support.

`plugins probe --plugin gpfs|sshfs --mount DIR [--access ces-nfs|ces-smb]`
is a new Linux read-only mount evidence endpoint; it checks
`/proc/self/mountinfo` and exact filesystem type, redacting remote source
details. The returned `rw_mount_option` is only a kernel flag, never proof
of actual remote write permissions, verified identity or successful backup.
It launches no client or mount command, accesses no user data or remote
credentials, and does not certify a generic adapter.

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

## r036 — Oracle live RMAN and user-protected backup semantics

The source catalog adds `oracle` as a **planned backup input** and inverse
restore destination. The driver must invoke the *operator's locally
licensed* Oracle RMAN online backup for an ARCHIVELOG database; capture
a recoverable SCN, archived redo, datafile/backup pieces and controlfile/
SPFILE while the service stays available. No copying live Oracle files
as a substitute for an application-consistent operation, and no
redistribution of Oracle binaries. Extra-cost RMAN compression,
encryption or standby options must remain disabled without the required
entitlements; Multi Backup uses its own encryption layer.

**Every output adapter must implement explicit backup protection
independently of retention:** a user-protected logical backup is not
deletable, even after 30 days, until the authenticated user explicitly
removes the hold. Generic output adapters must read that state before
manual deletion, policy sweep, retention shortening, or chunk GC.
The local chunk-store and legacy archive path now have protection
enforcement; remote providers still need their own tested enforcement.
WORM/Object Lock on an independent vault is a separate and stronger
safety property. See `docs/BACKUP_PROTECTION.md`.

## r038 — operational filesystem adapters (development preview)

The compiled `rust/src/plugin_executor.rs` has the first complete generic
execution path: **POSIX → POSIX** encrypted native backup and full/selective
restore into an explicit fresh directory. The destination uses verified
age-encrypted content-addressed chunks and an encrypted logical manifest.
No path overlap with the source, storage or private age identity is permitted.

On Linux, `nfs`, `smb`, `sshfs`, and `gpfs` (CES NFS/SMB) can read from
an already mounted **read-only** source only when (1) the exact mountpoint
has the expected kernel filesystem type, and (2) the caller explicitly
supplies `--source-snapshot-attested`. GPFS additionally requires
`--input-gpfs-access ces-nfs|ces-smb`. Mount evidence plus user attestation
cannot independently prove a storage-side snapshot, consistent application
state, ACL/xattr retention or actual IBM backend identity. These adapters
are not production Ready, and no remote mount or write is ever attempted.

```bash
multi-backup-native plugins plan --input nfs --output posix --mode backup --json
multi-backup-native plugins backup --input posix --output posix --source /src \
  --root /encrypted-store --identity /key/identity.txt --label first --json
multi-backup-native plugins restore --input posix --output posix \
  --root /encrypted-store --identity /key/identity.txt \
  --id BACKUP_ID --destination /fresh-restore
```

`rust/tests/plugin_dispatch_roundtrip.rs` is a blocking native functional
test in the Production Gate and macOS/Windows/Debian packaging jobs. Catalog:
32 plugin IDs, 53 backup roles, **5 preview executable backup pairs**, and
0 fully certified generic pairs (out of 682 possible); all other combinations
fail closed. GitHub/Dropbox still have dedicated preview engines outside this
generic dispatch and require live-service E2E. Legacy Python archive migration
is explicitly optional and no longer a promotion gate.


## r038 — source/target user separation and incremental contract

- An account-scoped restore must store **source account as provenance**, but independently authorize the destination with target-specific credentials and least privilege. Never reuse source API secrets in another user's account by implication.
- Rust dedicated preview paths: GitHub new repository under an explicitly matched target owner; Dropbox add-only upload of one file using an independently verified target write-token account ID/email. Remote arbitrary selection, multi-account metadata/ACL transfer and full-account restore remain open.
- Backup plugins must eventually implement both **FULL** and **INCREMENTAL** from CLI/manual schedules, with signed/authenticated lineage, reference full and complete per-snapshot metrics. Verification of incrementals defaults to the full chain, with explicit opt-out.
- Deletion of any full or increment must preserve all surviving restore points: rebase descendants, promote successors to FULL if necessary, never physically delete live chunks during purge, and leave actual ciphertext reclamation to a separately authorized GC. Protected children block automatic lineage changes.
- Local GC retains partially live parity 8+2 stripes for redundancy; crash-safe repacking and remote GC remain unqualified. Details: [IT operations](R038_OPERATIONS_IT.md), [EN operations](R038_OPERATIONS_EN.md) and [RC matrix](RC_CERTIFICATION_MATRIX.md).

## r038 — Cross-account e rebase/GC condivisi

Per ogni plugin autenticato l'identità dell'account sorgente è sola provenienza: il restore verso altro utente richiede autorizzazione indipendente dell'account di destinazione. GitHub crea un nuovo repository su un proprietario target autorizzato; Dropbox verifica l'account del token WRITE per il nuovo file. I restore remoti parziali generici e le combinazioni cloud-to-cloud NON sono certificati.

La destinazione POSIX Rust supporta FULL/INCREMENTAL, purge con rebase, verifica catena completa e GC separata con repack automatico 8+2. Non implica che gli altri plugin catalogati siano adapter eseguibili. Leggere [FEATURE_INVENTORY_R038.md](FEATURE_INVENTORY_R038.md) e [OPERATIONS_R038_EN.md](OPERATIONS_R038_EN.md).

## r039 — Common source transport & cluster contract

Every plugin input can declare server_local, ssh_agentless, managed_agent_pull or managed_agent_push plus ExecutionLimits. This declares a source preparation/dispatch mode, NOT an executable adapter. For AFS a volume-specific vos dump local or pinned SSH plan and managed-agent preview exist; the generic server replica worker currently executes only server_local POSIX/NFS/SMB/SSHFS/GPFS CES filesystem input.

Budget fields: max_concurrent_jobs, threads_per_job, max_transfer_bytes_per_sec, max_host_cpu_percent, max_host_memory_percent, min_free_memory_bytes. Pull and autonomous push must apply stricter server/agent limits. Single-volume AFS dumps cannot be split into threads; concurrent independent volumes may execute on different eligible nodes. Node_local storage is assigned only to its registered owner replica; truly shared storage may be scheduled across eligible replicas with PG-backed leases. Writes after lost lease are not yet fenced and thus not certified. Procedure in [cluster r039 EN](CLUSTER_ORCHESTRATION_R039_EN.md).

## r039+ direct provider byte streams and exact mounted filesystem adapters

For trusted native source adapters, Store::backup_stream(identity,label,top_namespace,logical_path,source,kind,parent,finish) reads a provider Read stream into encrypted age/zstd deduplicated chunks and 8+2 parity. The finalizer MUST confirm successful upstream command/provider completion BEFORE an encrypted manifest can be published; if producer fails only unreferenced encrypted chunks remain subject to separate guarded GC. No plaintext dump staging is required.

The generic executor currently supports exactly 8 filesystem input plugins to POSIX and 6 restore destinations from POSIX, never all catalog entries. For Linux mounted sources, the path must be an exact RO mount with supported type and operator snapshot attestation; for mounted output, destination must be a non-existing immediate child of an exact matching RW mount. The type and permissions of the mount do not prove source snapshot consistency, ACL/xattr/stripe fidelity or shared storage ownership.

AFS uses a separate typed vos volume adapter with existing-token local/pinned SSH dump and age stream backend, and a confirmed new-volume vos restore chain; it MUST NOT be called via a generic POSIX filesystem source. See [AFS and mounted provider operators IT](PLUGIN_EXECUTORS_R039_IT.md) and [EN](PLUGIN_EXECUTORS_R039_EN.md).
