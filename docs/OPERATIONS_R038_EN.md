# Multi Backup 0.1.0-r038 — Rust operations manual (preview)

**Updated:** 14 November 2026. **Scope:** Rust CLI/desktop, draft PR #10. **NOT a certified RC or stable release.**

This is the current operational handbook for new Rust-format backups. Older Python/Tk r018 screenshots and PDF manuals remain historical. Migrating existing Python-created archives is optional, and no old data or key is automatically removed. The full feature inventory and evidence gaps are documented in [RC_CERTIFICATION_MATRIX.md](RC_CERTIFICATION_MATRIX.md).

## 1. Prerequisites

Use the compiled \`multi-backup-native\` CLI and a private filesystem with enough capacity. The packaged \`age\`/\`age-keygen\` helpers must be available on PATH. Store the age private key outside both the backup source and ciphertext store. Never include credentials in backup metadata, CLI arguments, source code or public documentation. Review verification results and test a fresh-directory restore before considering a backup useful.

This Rust v1 store is still experimental; Reed-Solomon 8+2 parity is NOT an independent off-site backup or immutable/WORM recovery vault.

## 2. Initialize encrypted storage

\`\`\`bash
age-keygen -o /private/keys/backup-age.txt
# Replace with the actual public recipient printed by age-keygen.
multi-backup-native storage init --root /private/backup-store \
  --recipient age1REPLACE_WITH_PUBLIC_RECIPIENT --chunk-mib 4
\`\`\`

The store uses authenticated age metadata, zstd-compressed age-encrypted chunks, a secret-keyed HMAC-SHA256 chunk ID, and encrypted 8+2 parity. Do not manually manipulate \`.multi-backup-v1/\`.

## 3. Full and incremental backups

Create a \`sources.json\` file that identifies existing source directories:

\`\`\`json
[{"namespace":"documents","directory":"/data/documents"}]
\`\`\`

\`\`\`bash
multi-backup-native storage backup --root /private/backup-store \
  --identity /private/keys/backup-age.txt --sources-json sources.json \
  --label "Monthly full" --mode full
# After recording the actual FULL_ID:
multi-backup-native storage backup --root /private/backup-store \
  --identity /private/keys/backup-age.txt --sources-json sources.json \
  --label "Daily incremental" --mode incremental --parent FULL_ID
\`\`\`

Each incremental includes a complete authenticated logical inventory, while unchanged ciphertext chunks are reused; a \`parent_id\` links to the previous backup for chronology, audit and chain verification.

## 4. Size statistics

\`\`\`bash
multi-backup-native storage info --root /private/backup-store \
  --identity /private/keys/backup-age.txt --id BACKUP_ID --json
\`\`\`

The report includes the file count, uncompressed logical byte count, sum of all referenced **zstd-compressed** bytes before deduplication, unique referenced compressed bytes after deduplication, newly referenced compressed bytes absent from the direct predecessor, added/modified/deleted file counts, and the full ordered history back to its reference full. Compression figures **exclude age encryption, manifest and parity overhead** and must not be confused with disk space reclaimable by GC.

## 5. Verification and selective restore

\`\`\`bash
# Default: verify the complete chain up to its full reference.
multi-backup-native storage verify-chain --root /private/backup-store \
  --identity /private/keys/backup-age.txt --id BACKUP_ID
# Optional: check only the selected snapshot.
multi-backup-native storage verify-chain --root /private/backup-store \
  --identity /private/keys/backup-age.txt --id BACKUP_ID --no-chain
# Full restore into a target directory that DOES NOT YET EXIST:
multi-backup-native storage restore --root /private/backup-store \
  --identity /private/keys/backup-age.txt --id BACKUP_ID \
  --destination /restores/fresh-full
# Restore only one logical path:
multi-backup-native storage restore --root /private/backup-store \
  --identity /private/keys/backup-age.txt --id BACKUP_ID \
  --selection documents/invoices --destination /restores/fresh-selection
\`\`\`

Verification checks age authentication, SHA256/HMAC, chunk content and parity. Restore never silently overwrites an existing target. Compare the recovered files with known-good source data before retiring old backups.

## 6. Safe purge and dependent rebasing

\`\`\`bash
multi-backup-native storage delete-backup --root /private/backup-store \
  --identity /private/keys/backup-age.txt \
  --id OLD_ID --confirm-id OLD_ID
\`\`\`

A purge modifies only encrypted logical manifests. Deleting F0→I1→I2's I1 reparents I2 to F0; deleting F0 promotes I1 to a new full, preserving the ability to restore I1/I2. These complete manifests already contain the necessary chunk references, so source data is not re-read and chunk ciphertext is not rewritten. A protected backup or protected dependent blocks automatic manifest rewriting until explicitly unprotected. A corrupt/inconsistent chain must block deletion. **Purge alone does not free physical space.**

## 7. Automatic 8+2 parity repack and separate garbage collection

\`\`\`bash
# Read-only preview: see orphan chunks, mixed stripes and potential bytes.
multi-backup-native storage gc-preview --root /private/backup-store \
  --identity /private/keys/backup-age.txt --grace-hours 48
# Explicit destructive confirmation for ORPHAN ciphertext only:
multi-backup-native storage gc-collect --root /private/backup-store \
  --identity /private/keys/backup-age.txt \
  --confirm ERASE-ORPHANS --grace-hours 48
\`\`\`

During **gc-collect**, mixed live/dead Reed-Solomon 8+2 stripes are automatically repacked: the engine generates new parity for the surviving ciphertext chunks, independently verifies both encrypted parity shards, and commits the encrypted successor stripe index LAST. Until that atomic commit, the old stripe remains authoritative; after commit the old index and ciphertext remain on disk for recovery until the successor has passed the grace period. A later GC removes obsolete indexes/parity and now-unreferenced chunks. The default grace period is 48 hours, minimum 24. Orphan prepared parity shards with no index can be removed after their own grace, supporting interrupted precommit cleanup.

**Release caveat:** this is development-preview code. Multi-generation repacks, mid-commit power failures, OS storage durability, rebase transactions, large volumes and remote GC still require independent acceptance. Dry run never repacks and does not free space. Do not manually delete \`stripes/\` or \`parity/\`. Not every byte can be reclaimed immediately in the first pass.

## 8. Scheduled backups and checks

Rust offers \`schedule run\` (manual or invoked by cron/launchd/Task Scheduler) and \`schedule daemon\` (process polling). Jobs choose \`backup_mode: "full"|"incremental"\` or \`action: "verify"\`; \`verify_full_chain\` defaults to **true**.

\`\`\`json
{
  "version": 1,
  "jobs": [
    {
      "id": "daily", "enabled": true, "interval_seconds": 86400,
      "action": "backup", "backup_mode": "full",
      "root": "/private/backup-store", "identity": "/private/keys/backup-age.txt",
      "source": "/data/documents", "input_plugin": "posix",
      "output_plugin": "posix", "namespace": "documents", "label": "Full backup"
    },
    {
      "id": "weekly-check", "enabled": true, "interval_seconds": 604800,
      "action": "verify", "root": "/private/backup-store",
      "identity": "/private/keys/backup-age.txt",
      "backup_id": "REPLACE_WITH_REAL_ID", "verify_full_chain": true
    }
  ]
}
\`\`\`

\`\`\`bash
multi-backup-native schedule run --config /private/jobs.json \
  --state /private/scheduler-state.json --force
multi-backup-native schedule daemon --config /private/jobs.json \
  --state /private/scheduler-state.json --poll-seconds 60
\`\`\`

A failure does not advance a job's last success timestamp. Verification currently needs an explicit backup ID and is not dynamically linked to other jobs. The native GUI does not yet persist the schedule configuration, and GC is not scheduled automatically. Use one daemon/runner with the filesystem lock and protect the private state files.

## 9. Operational plugins and cross-account restore

32 plugin IDs are registered; only **five backup pairs** have a preliminary generic Rust executor: POSIX→encrypted POSIX and read-only pre-mounted NFS/SMB/SSHFS/GPFS CES→encrypted POSIX on Linux. For remote mounted inputs require matching read-only kernel mount evidence and \`--source-snapshot-attested\`; the operator's flag is NOT independent proof of a consistent backend snapshot. GPFS CES additionally requires \`--input-gpfs-access ces-nfs|ces-smb\`. None of the 682 theoretically possible generic pairs is yet production certified.

For cross-account recovery, **source ownership is provenance, not authorization**. Target access must be authenticated independently:

\`\`\`bash
# "gh" CLI must already be authenticated to an account able to create under newowner.
multi-backup-native restore github --archive /backup/repository.tar.gz.age \
  --identity /private/keys/backup-age.txt --source-account oldowner \
  --target-account newowner --target newowner/recovered-repo \
  --visibility private --confirm CREATE-NEW

# Dropbox's WRITE token belongs to the DESTINATION account.
multi-backup-native restore dropbox --archive /backup/single-file.tar.gz.age \
  --identity /private/keys/backup-age.txt \
  --source-account old@example.com --target-account new@example.com \
  --path /restored/new-file.ext --confirm CREATE-NEW
\`\`\`

Set \`MULTI_BACKUP_DROPBOX_WRITE_TOKEN\` through a secure OS credential service or ephemeral private process environment; avoid shell-history exposure. The destination account is checked with Dropbox \`get_current_account\` before upload. Current native GitHub recovery creates a **new full repository** from an encrypted Git mirror and Dropbox recovery uploads **one file** per command. Generic partial remote restores, cross-provider writes, origin metadata/ACL and all provider-to-provider combinations remain unimplemented or not externally qualified.

## 10. Development status and RC blockers

Implemented in Rust preview: compiled CLI/GUI, age+zstd/HMAC chunk store, 8+2 parity, full/incremental points, selective local restore, chain metrics/checks, protected purge/rebase, separate GC with automatic parity repack, local scheduler and preliminary account-scoped remote flows.

**Not yet certified:** real-account GitHub/Dropbox full recoveries including LFS/CI metadata, partial cross-account restore, crash/power-failure-safe transaction drills, multigeneration parity repack under fault injection, native GUI scheduling, production GC/WORM, signed/notarized installers, real OS updates/rollback, independent security assessment. The current PR #10 must pass its own CI; the earlier green r037 run does not qualify r038.

See [CHUNK_STORAGE_V1.md](CHUNK_STORAGE_V1.md), [MAINTENANCE_POLICY.md](MAINTENANCE_POLICY.md), [PLUGIN_API.md](PLUGIN_API.md), [RC_CERTIFICATION_MATRIX.md](RC_CERTIFICATION_MATRIX.md), and [DEVELOPMENT_STATUS.md](DEVELOPMENT_STATUS.md).
