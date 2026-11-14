# Multi Backup plugin architecture — Rust native contract v1

The **source** and **destination** of each backup are plugins. A restore reverses
the direction of the same two plugins. Four separate capabilities are
advertised for each plugin: \`backup_input\`, \`backup_output\`,
\`restore_input\`, \`restore_output\`. A plugin's *registration* is not a
claim of implementation, live-account interoperability or release readiness.

## Rust contract and discovery

The authoritative, static, auditable registry is \`rust/src/plugins.rs\`.
The existing legacy Python registry (\`backends.available()\`) and native
\`github backup\` / \`dropbox backup\` commands remain functional and unchanged;
they are **not** dynamically executed by the new planner. Plugin IDs are stable
and independent of OS. No arbitrary local DLL, script, shell command or
downloaded plugin package is ever discovered/executed.

\`\`\`bash
multi-backup-native plugins list --json
multi-backup-native plugins list --role input --mode backup --json
multi-backup-native plugins list --role output --mode restore
multi-backup-native plugins plan --input github --output posix --mode backup --json
multi-backup-native plugins plan --input posix --output github --mode restore --json
\`\`\`

\`plugins plan\` is **read-only**. It rejects unknown IDs, unsupported
directional roles and invalid modes; \`executable=false\` until both generic
adapters are certified. \`preview\` means a *separate dedicated native
operation* exists, not that the unified connector is wired. \`planned\` means
only the interface specification/catalogue exists. \`unsupported\` is an
explicitly forbidden role. \`ready\` is reserved for live-tested, qualified
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
