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
directional roles and invalid modes; `executable=false` until both generic
adapters are certified. `preview` means a *separate dedicated native
operation* exists, not that the unified connector is wired. `planned` means
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

The common engine eventually composes any eligible input with any eligible
output; plugins are allowed to specialize metadata but cannot circumvent the
encryption, audit, retention, vault or two-phase commit rules. Runtime
orchestration is **not implemented by this catalog commit**.

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
