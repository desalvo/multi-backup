# Backup schedule, retention, capacity and namespaces — policy status

This is the **Rust preview implementation inventory**. The following
defaults are defined in `rust/src/backup_policy.rs`, and must eventually
be editable in the desktop/server settings with durable saved state.

| Policy | Default | Implemented today |
| --- | --- | --- |
| Backup schedule | Disabled until explicitly enabled; configurable per-job interval | Native `schedule run` (manual/cron/launchd/Task Scheduler) and Rust `schedule daemon` added in r038; live provider/service qualification pending |
| Backup retention | 30 days; keep at least one verified backup per namespace | `expiry_preview` and authenticated `storage retention-preview`; deletion job not wired |
| Explicit protection | Off until user protects a backup | Protected logical chunk-store and local legacy archives, UI/CLI and confirmation, E2E unit tests |
| Garbage collection | Execution manual with explicit confirmation; 48-hour orphan grace | Native local POSIX mark-and-sweep preview/run implemented in r038, preserving live parity and protection; mixed-stripe repack and remote GC not yet qualified |
| Namespace | Optional nested path, e.g. `Projects/Lab/Cluster` | Manifest field + CLI/GUI for new native chunk-store backups; filesystem index in-memory prototype |
| Destination capacity | Used/available/total for a specific destination | `mounted_capacity` for POSIX/mounted NFS/CIFS/GPFS/SSHFS/etc.; cloud-specific provider APIs not wired |
| Consistency | DB live; VM live with quiesce/snapshot; CT live where supported | Read-only `plan_consistency` only; hypervisor/database workflows not yet run |
| Oracle | Native RMAN online (ARCHIVELOG) | Input plugin declared; licensed/authorized RMAN external runtime not yet integrated |

## Backup protection and expiry

The protected state is authoritative for native chunk-store deletion and
retention preview, not just a checkbox on the UI. A protected backup cannot
be deleted through the implemented application APIs. Unprotecting requires
an independent explicit confirmation; the prior WORM retention lock (if
configured externally) must remain in force even after unprotection.

No retention worker may delete a manifest without an *in-lock* check of
the latest protection marker, age/SHA integrity and a server-side immutable
retention/hold policy. The protected marker must be included in any portable
copy. Chunk GC needs an authenticated reference graph for **all** still-live
manifests, plus parity stripes and any active upload/restore checkpoints.
A 30-day age test against the chunks themselves is NOT a valid GC algorithm.

## Data consistency before the app can mark a provider Ready

- MySQL/MariaDB and PostgreSQL: engine-supported online checkpoint and
  transaction/binlog/WAL continuity, verified point-in-time restore.
- Oracle: RMAN ARCHIVELOG backup pieces, redo, SPFILE and controlfile;
  paid features off unless licensed; restored SCN acceptance test.
- VMs: live storage snapshot and app/guest quiescing where possible.
  A crash-consistent snapshot is distinct from application consistency;
  opted-in graceful stop/restart fallback must restore initial power state
  even on failed backup.
- CT: live snapshot preferred, suspend/resume next when possible,
  stop/restart only as explicitly permitted, always with restart-on-error.

## Namespace and indexing architecture

A new logical backup can be placed in nested directories with
`--top-namespace` without breaking the older single-root format.
The `NamespaceIndex` prototype precomputes folder summaries and paginates
children, but does not yet persist an encrypted searchable index for very
large namespace trees. That remains a required performance gate.

## Remaining release gates

The UI for changing schedules/retention/GC must persist settings, trigger
a real durable scheduler, survive crashes/restarts and audit any deletion.
A full GC implementation requires cross-provider reference ownership,
append-only vault integrity, rate limiting, quarantine and independent
recovery drills. Never report these policy-only pieces as automated
production cleanup. See `docs/BACKUP_PROTECTION.md`.


## r038 — full/incremental chain purge and garbage collection contract

A logical **FULL** represents all files at a reference point. An **INCREMENTAL**
is a complete snapshot inventory of encrypted chunk references, but reuses
all unchanged content-addressed chunks and adds only changed chunks.
The `parent_id` chain is authenticated and is used for provenance,
integrity audits and user-visible full/incremental statistics. Full and
incremental modes may be selected manually or independently per schedule.

**Cancellazione / purge:** the explicitly confirmed `storage delete-backup`
now automatically invokes `Store::delete_backup_rebase`: direct children of
a deleted incremental are reparented to the deleted increment's predecessor;
direct children of a deleted full become new full reference points. Complete
per-backup file inventories and their chunk references remain intact. More
remote/incremental children are discovered before any mutation. A protected
child or unhealthy dependency blocks the operation; the user must remove
protection explicitly, never implicitly. If a child is already full, its
file references need not be copied. The physical chunk count is UNCHANGED
by purge.

**Garbage collection:** `storage gc-preview` is read-only and reports
eligible ciphertext and the bytes potentially reclaimable; `storage
gc-collect --confirm ERASE-ORPHANS` is a distinct, explicitly approved
operation. A common store lock protects the authenticated reference graph.
Before any deletion the engine verifies every live backup/lineage, including
parity. Default grace 48h, minimum 24h. Fully unreferenced parity stripes
and eligible orphan ciphertext chunks may be removed after grace. If a parity
stripe has **both live and orphan members**, its full old membership is
retained; a separate crash-recoverable **stripe repacker** is still required
to reclaim *all* otherwise redundant data. No live chunk, protected hold or
source snapshot is deliberately deleted. Remote object GC, automatic policy
purge and physical WORM enforcement are still outside this preview.

**Verification:** new Rust `storage verify-chain` checks the entire
incremental chain back to its reference full by default; `--no-chain`
limits verification to the selected point. Scheduling uses
`verify_full_chain: true` unless explicitly overridden. A corrupt link
or unverified chain blocks GC. `storage info` reports file count,
uncompressed bytes, zstd-compressed bytes, post-dedup zstd bytes, new
compressed chunk bytes, added/modified and removed files, and the full
ordered history of increments with reference full ID.

This is a locally implemented Rust development path, **not** yet a certified
unattended production retention/GC service. Acceptance tests include chain
promotion, protected-dependency refusal, no physical deletion at purge,
grace-period gating, orphan parity deletion, and preservation of mixed
live/dead parity protection.