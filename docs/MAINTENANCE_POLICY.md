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
