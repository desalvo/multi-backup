# Multi Backup — Rust r038 operations guide (EN)

**Status:** development preview on draft PR #10, **not a published RC or a certified production release**. The previously distributed r018 Python/Tk build remains available. Python-created archive data migration is optional by explicit project decision; new Rust functionality and recoverability are the acceptance criteria. Never deploy destructive features against your only copy before the outstanding [RC gates](RC_CERTIFICATION_MATRIX.md) are passed.

Related reference: [IT operational procedures](R038_OPERATIONS_IT.md), [chunk format](CHUNK_STORAGE_V1.md), [plugins](PLUGIN_MATRIX.md), [holds](BACKUP_PROTECTION.md), [maintenance](MAINTENANCE_POLICY.md), [security](SECURITY.md), [development status](DEVELOPMENT_STATUS.md).

## 1. Supported development-preview features

| Area | Rust r038 code | Unqualified aspects |
|---|---|---|
| Native CLI and desktop UI | Full/incremental controls, backup chain inspection, verification, purge and confirmed GC | Physical installer usability, graphical completeness, final CI and security signoff |
| Encrypted local store | Full/incremental logical snapshots, complete file reference inventories, content-addressed dedup, Zstandard, age, HMAC and 8+2 Reed–Solomon parity | Multi-terabyte workloads, power loss, metadata/ACL/xattr fidelity |
| Generic plugin executor | POSIX→POSIX, plus Linux read-only mounted NFS/SMB/SSHFS/GPFS CES inputs → POSIX encrypted store | Remote source consistency, generic remote destinations and live mount acceptance |
| Account plugins | Dedicated GitHub/Dropbox preview paths, create-new target workflows with independent destination authorization | Full cross-user and multi-item restore, permissions/metadata and authenticated real-service recovery |
| Schedules | Rust one-shot runner and polling daemon, configurable backup mode or verification; persistent state | GUI scheduler persistence, OS service packaging, alerts and durable multi-host coordination |
| Chain maintenance | Safe descendant reparenting/promoting to FULL, manifest-only purge; separate preview/confirmed local GC | Atomic multi-manifest crash transaction, mixed-stripe repacking, remote GC, WORM |

The registry has **32 plugin IDs and 53 declared backup roles**, with **5 preview-executable filesystem backup pairs**; **0/682 generic source/destination combinations are production-certified**. Do not interpret a listed provider as a working adapter.

## 2. Preparing encrypted test storage

Use the compiled \`multi-backup-native\` binary plus installed \`age\` / \`age-keygen\` helpers. Store the private age identity outside the backup medium in a restricted folder on an encrypted disk. The Rust content-addressed store currently takes an external identity path for some operations; not all GUI storage paths use the unlocked vault directly.

\`\`\`bash
mkdir -p "$HOME/.config/multi-backup/keys" "$HOME/mb-r038-test"
chmod 700 "$HOME/.config/multi-backup/keys"
age-keygen -o "$HOME/.config/multi-backup/keys/r038.txt"
multi-backup-native --version
multi-backup-native storage init --root "$HOME/mb-r038-test/store" \
  --recipient 'age1REPLACE_WITH_REAL_PUBLIC_RECIPIENT'
\`\`\`

Protect and test a separate recovery copy of the age identity. The public recipient **cannot** regenerate a lost private key.

## 3. Manual FULL and INCREMENTAL backups

A \`sources.json\` file contains an array of existing directories:

\`\`\`json
[
  {"namespace":"documents","directory":"/home/example/Documents"},
  {"namespace":"metadata","directory":"/home/example/Metadata"}
]
\`\`\`

\`\`\`bash
# Reference FULL; record its returned 64-character ID, F0
multi-backup-native storage backup --root STORE --identity AGE_IDENTITY \
  --sources-json sources.json --label "Full 1" --mode full \
  --top-namespace "Test/User"

# First increment; record its new ID, I1
multi-backup-native storage backup --root STORE --identity AGE_IDENTITY \
  --sources-json sources.json --label "Increment 1" --mode incremental \
  --parent F0 --top-namespace "Test/User"

# Subsequent increments refer to the prior increment (e.g. --parent I1)
\`\`\`

Each incremental saves a **complete inventory of the current tree as encrypted chunk references** and reuses unchanged ciphertext chunks. No old source files are physically copied into the next increment. The same top-level namespace is mandatory along a lineage. Removed files remain restorable from previous snapshots that are still retained.

For the native generic plugin:

\`\`\`bash
multi-backup-native plugins backup --input posix --output posix \
  --source /source --root STORE --identity AGE_IDENTITY \
  --label "Plugin full" --namespace files --mode full
\`\`\`

Only on Linux, an independently prepared **read-only mountpoint** can be a preview source:

\`\`\`bash
multi-backup-native plugins backup --input nfs --output posix \
  --source /mnt/nfs-snapshot --source-snapshot-attested \
  --root STORE --identity AGE_IDENTITY --label "NFS full"
\`\`\`

Other preview mounted source IDs: \`smb\`, \`sshfs\`, \`gpfs\`. GPFS CES additionally requires \`--input-gpfs-access ces-nfs|ces-smb\`. The exact OS mount type and read-only flag are checked. **A user attestation is not independent proof of a consistent snapshot, correct ACL/xattr capture or application-consistent database state.** No remote mounts are created or written.

## 4. Backup information, sizes and verification

\`\`\`bash
multi-backup-native storage list --root STORE --identity AGE_IDENTITY --json
multi-backup-native storage info --root STORE --identity AGE_IDENTITY --id I1 --json
# Default: verify the selected snapshot AND all increments back to the reference FULL
multi-backup-native storage verify-chain --root STORE --identity AGE_IDENTITY --id I1
# Only check the selected point (explicitly opt out of full chain)
multi-backup-native storage verify-chain --root STORE --identity AGE_IDENTITY --id I1 --no-chain
\`\`\`

\`storage info\` includes the complete ordered chain plus, for **each point**: type, reference full, previous ID, file count, uncompressed bytes, Zstandard-compressed bytes **before age encryption**, compressed-and-deduplicated bytes for **unique chunks within that snapshot**, new compressed chunk bytes relative to the predecessor and added/modified/deleted file counts. These are **not total physical disk usage**, which additionally includes age headers, encrypted manifests, other snapshots and parity. Do not sum per-snapshot deduplicated sizes to estimate shared storage.

Full-chain integrity checks age authentication, HMAC/SHA content and parity. Run isolated restore drills too; green checks alone do not prove remote recovery.

## 5. Restore and cross-account operation

\`\`\`bash
# Destination must not exist. Restore one selected snapshot, complete tree:
multi-backup-native storage restore --root STORE --identity AGE_IDENTITY \
  --id I1 --destination FRESH_DIRECTORY

# Restore only a file or folder prefix recorded in the manifest:
multi-backup-native storage restore --root STORE --identity AGE_IDENTITY \
  --id I1 --selection "Test/User/documents/reports" \
  --destination ANOTHER_FRESH_DIRECTORY
\`\`\`

For providers with user accounts, the **origin account** is provenance, not permission to write to the destination. The destination must have independently authorized credentials. Current remote commands support a **new GitHub repository** or **new Dropbox file** only, not complete arbitrary cross-account metadata migration and not a multi-repository selective restore.

\`\`\`bash
# Authenticate gh as an identity allowed to create under the target owner.
multi-backup-native restore github --archive REPO.tar.gz.age \
  --identity AGE_IDENTITY --target target-owner/new-repository \
  --source-account source-owner --target-account target-owner \
  --visibility private --confirm CREATE-NEW

# Use a WRITE token for the destination Dropbox account, separate from source READ.
export MULTI_BACKUP_DROPBOX_WRITE_TOKEN='TARGET_WRITE_TOKEN'
multi-backup-native restore dropbox --archive FILE.tar.gz.age \
  --identity AGE_IDENTITY --path /folder/restored-file.ext \
  --source-account SOURCE_ACCOUNT --target-account TARGET_ACCOUNT_ID \
  --confirm CREATE-NEW
unset MULTI_BACKUP_DROPBOX_WRITE_TOKEN
\`\`\`

GitHub refuses existing target repositories and requires \`git\`, \`git-lfs\`, \`gh\`, appropriate target ownership and \`gh auth status\`. Dropbox checks the account ID/email associated with the **target write token** through its own API before upload, prohibits overwrites and checks the returned content hash. Drive/OneDrive/Nextcloud and other account-based plugins still have only planned contracts. Cross-account end-to-end tests, multi-item selection, metadata and permission replication are **not yet certified**.

## 6. Chain-aware logical deletion (purge)

For \`FULL F0 → INC I1 → INC I2\`:

- Delete I1: I2 is reparented to F0, retaining the exact I2 file inventory.
- Delete F0: I1 becomes a self-contained FULL, and I2 continues from I1.
- Delete last increment: no ancestor needs rewriting.

\`\`\`bash
multi-backup-native storage delete-backup --root STORE \
  --identity AGE_IDENTITY --id DELETE_ID --confirm-id DELETE_ID
\`\`\`

The program refuses to purge a protected snapshot, rewrite a protected child or proceed with unverifiable dependencies. An explicit, separate authenticated unprotect action is required before modifying any such descendant:

\`\`\`bash
multi-backup-native storage unprotect-backup --root STORE \
  --identity AGE_IDENTITY --id CHILD_ID --confirm-id CHILD_ID
\`\`\`

The purge rewrites authenticated encrypted manifests as necessary and removes the target logical manifest. **It does NOT delete any chunk or parity object and cannot immediately free capacity.** Multi-manifest power-loss transactions are not yet production-certified. If interrupted, stop, inspect the surviving chain, re-verify, and only then resume corrective operations. Never run physical cleanup based solely on a successful purge message.

## 7. Separate, explicit garbage collection

\`\`\`bash
multi-backup-native storage gc-preview --root STORE \
  --identity AGE_IDENTITY --grace-hours 48 --json

# After reviewing the preview and verifying an independent recovery copy:
multi-backup-native storage gc-collect --root STORE \
  --identity AGE_IDENTITY --grace-hours 48 \
  --confirm ERASE-ORPHANS --json
\`\`\`

GC has a **48-hour default** orphan grace, minimum **24 hours**. It takes the store lock, scans and verifies all remaining manifests, links, ciphertext and parity, and can remove only eligible orphan objects and **fully orphaned parity stripes**. A parity stripe with both live and dead member chunks is kept intact, including dead members, until a future crash-safe 8+2 stripe repacker is qualified. Therefore reclaimable space may be **smaller than theoretical unused space**. The GC report's \`reclaimable_bytes\` represents eligible physical ciphertext/parity objects, not logical uncompressed sizes. Remote GC, automatic retention-triggered deletion and legal-grade WORM enforcement are **not** qualified.

## 8. Manual and scheduled backups and integrity checks

A Rust job chooses FULL, INCREMENTAL or VERIFY. Configure \`jobs.json\` (permissions private):

\`\`\`json
{
  "version": 1,
  "jobs": [
    {
      "id": "daily-increment",
      "enabled": true,
      "interval_seconds": 86400,
      "action": "backup",
      "backup_mode": "incremental",
      "root": "/backup/store",
      "identity": "/secure/identity.txt",
      "source": "/data/Documents",
      "namespace": "documents",
      "top_namespace": "Team",
      "input_plugin": "posix",
      "output_plugin": "posix",
      "label": "Daily documents",
      "parent_id": "EXISTING_VERIFIED_FULL_ID"
    },
    {
      "id": "weekly-verification",
      "enabled": true,
      "interval_seconds": 604800,
      "action": "verify",
      "root": "/backup/store",
      "identity": "/secure/identity.txt",
      "backup_id": "SNAPSHOT_TO_CHECK",
      "verify_full_chain": true
    }
  ]
}
\`\`\`

An incremental job must start from a valid parent and then remembers \`last_backup_id\` in **that job's own persistent state**. A different verification job **does not automatically inherit** that ID: provide an explicit \`backup_id\` (until cross-job references are implemented). The default verification checks the *whole* chain; \`verify_full_chain:false\` opts out.

\`\`\`bash
multi-backup-native schedule run --config jobs.json --state /private/schedule-state.json
multi-backup-native schedule run --config jobs.json --state /private/schedule-state.json --force
multi-backup-native schedule daemon --config jobs.json \
  --state /private/schedule-state.json --poll-seconds 60
\`\`\`

Intervals are seconds, not full cron expressions (minimum 60 seconds). The one-shot runner can be launched by cron/launchd/Windows Task Scheduler. The daemon uses periodic polling; it is **not yet shipped as a production-certified OS service**. Failed jobs do not advance their success timestamp. The scheduler rejects duplicate IDs and uses a lock against concurrent runners. Configuration GUI, notification delivery, tested restart/rollback and account-based schedules remain incomplete.

## 9. Mandatory RC certification checks

1. Latest PR-head Production Gate and platform build matrix GREEN, including real \`age\` encrypted backup, incremental rebase, GC and plugin executable tests.
2. Byte-for-byte FULL/INC and selective restore before/after purge and GC on separate media, including corruption and power-loss injection.
3. Real authenticated GitHub and Dropbox cross-account target creation, least-privilege scopes, LFS/metadata/CI artifacts, partial item workflows, rate-limit/retry/resume and identity isolation.
4. Safe parity-group repacking, remote GC, WORM/legal hold semantics and independent recovery of encrypted keys and data.
5. Installed signed/notarized native application QA, accessibility, update/rollback and independent security review.

**This is a description of development-preview behavior and the required operational checks, not a guarantee of production readiness.**
