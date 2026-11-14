# Explicit backup protection — r036 baseline with r038 operations

An explicitly protected backup **must never be deleted** by an application
operator, retention policy, scheduler or garbage collector until an authorized
operator explicitly removes that protection. This is independent of the
configured backup expiry (default 30 days), protected/immutable WORM storage
and the existing 8+2 parity feature. Do not confuse `storage protect` (parity)
with `storage protect-backup` (user-requested deletion hold).

## Native content-addressed store

Encryption-backed protection metadata is stored under
`.multi-backup-v1/protected/<backup-id>.json.age`, where the backup ID is a
validated SHA-style 64-character hexadecimal identifier. The marker records
version, ID and `protected=true`; it must decrypt successfully with the
operator's age identity and match the manifest ID. A corrupted or symlink
marker **fails closed**. The code serializes protection, unprotection and
logical deletion using an exclusive storage lock.

```bash
multi-backup-native storage list --root STORE --identity KEY --json
multi-backup-native storage protect-backup --root STORE --identity KEY --id FULL_BACKUP_ID
multi-backup-native storage unprotect-backup --root STORE --identity KEY --id FULL_BACKUP_ID --confirm-id FULL_BACKUP_ID
multi-backup-native storage delete-backup --root STORE --identity KEY --id FULL_BACKUP_ID --confirm-id FULL_BACKUP_ID
```

Once protected, deletion is refused even if a matching confirmation ID is
supplied. Explicit unprotection is a separate, confirmed operation; it does
not delete the backup. Desktop Storage shows a **PROTETTO** lock label and
offers separate actions to protect, request unprotection with confirmation
and delete only when unprotected.

Deleting a backup now first **rebases descendants or promotes the next increment to FULL**, then removes the chosen encrypted manifest. Protected or unhealthy children block this change. Purge NEVER deletes chunks, stripe metadata or parity. The r038 local GC is a separate preview/confirmed operation, with grace (48h default, 24h minimum) and authenticated reference scan. Mixed live/dead parity stripes are kept intact until crash-safe repacking is qualified. Never delete ciphertext manually based on age or filename.

ZIP64 export/import and storage relocation include the encrypted hold
marker so a protected backup remains protected after transport or copy.
Older manifests and archives without a hold are still readable.

## Existing legacy tar.gz.age archives

Rust CLI also supports per-archive protection and explicit unlock:

```bash
multi-backup-native archive protect --root BACKUP_ROOT --archive BACKUP_ROOT/metadata/ARCHIVE.tar.gz.age
multi-backup-native archive unprotect --root BACKUP_ROOT --archive BACKUP_ROOT/metadata/ARCHIVE.tar.gz.age --confirm ARCHIVE.tar.gz.age
multi-backup-native archive delete --root BACKUP_ROOT --archive BACKUP_ROOT/metadata/ARCHIVE.tar.gz.age --confirm ARCHIVE.tar.gz.age
```

The legacy marker is a local `ARCHIVE.tar.gz.age.protected.json` sidecar
and is **not** a cryptographic WORM lock. All local management operations
share one root-specific exclusive lock; a corrupt or maliciously symlinked
marker blocks deletion. The archive must be within the known catalog and
cannot be deleted while an active checkpoint references it.

## Retention, recovery and realistic guarantees

`rust/src/backup_policy.rs` defines `protected` as a retention override:
an explicitly protected backup is always returned in the preservation set,
even when its retention window has expired. The 30-day automatic retention purge remains policy-only. A native Rust one-shot/polling scheduler and *manually confirmed local GC* are implemented in r038 as development previews, not a production-certified unattended cleanup system.

These controls are application-level protections: users with filesystem
administrative privileges or a compromised storage backend may tamper with
local files and markers. True independent anti-ransomware resilience requires
the separate WORM/Object Lock vault, an append-only principal with no delete
rights, enforced immutable retention and independently tested recovery
authorization. No such production qualification is claimed yet.

Protection for all future remote destination plugins must be enforced in
each plugin's own retention/delete API as well as the common engine. Never
describe the current chunk-store and legacy-archive implementation as
comprehensive protection for unimplemented cloud/NAS/hypervisor adapters.

## Acceptance scenarios

1. Protect a verified logical backup, confirm it appears protected in list/GUI.
2. Attempt manual delete with correct confirmation: **blocked**.
3. Attempt retention preview on an expired protected entry: **preserved**.
4. Unprotect with wrong ID: **blocked**. Correct ID: marker removed, backup remains.
5. After removal of protection, confirm and delete the unprotected logical
   manifest; verify a second backup sharing chunks still restores correctly.
6. ZIP export/import of protected backup retains the hold and blocks deletion.
7. Corrupt marker blocks list/deletion; local archive guard blocks protected
   legacy deletion until explicit exact-filename unprotection.
8. Validate protection in the Desktop UI; preserve fail-closed behavior in
   server/other provider dispatch before certifying any universal adapter.

CI test: `cargo test --manifest-path rust/Cargo.toml --all-targets`,
`cargo test --manifest-path rust-server/Cargo.toml --all-targets`,
and `cargo check --manifest-path rust-desktop/Cargo.toml`. Green compile
is not independent multi-provider recovery certification.

## r037 — nested namespaces and retention preview tied to real encrypted holds

New native backups may be created within an optional nested top namespace
without changing paths of existing backups:

```bash
multi-backup-native storage backup --root STORE --identity KEY --label "Nightly" \
  --sources-json sources.json --top-namespace "Rome/Cluster/VM"
multi-backup-native storage retention-preview --root STORE --identity KEY \
  --days 30 --keep-last 1 --json
```

The manifest records its `top_namespace` and UTC creation timestamp.
Existing manifests without these fields remain readable; unknown legacy
timestamps are **never auto-expired**. The retention preview reads
`Store::list`, verifies each backup's encrypted chunks and protection
status, and passes the *authoritative encrypted hold flag* to the
30-day policy. A protected backup always remains in the preserve set,
including when it is older than 30 days. No deletion is performed by
`retention-preview`. Rust r038 includes an experimental scheduler and separately confirmed local GC; automated retention-deletion and remote GC are NOT enabled.


## r038 — protezione del successore durante il rebase

A protected child prevents the purge of a parent if that action would rewrite the child's authenticated lineage. A separate confirmed `unprotect-backup` is required; it must never run implicitly. `storage protect` configures 8+2 parity; `storage protect-backup` sets a no-delete hold; neither provides independent hardware WORM. The physical GC only removes unreferenced ciphertext, and partially live parity stripes are retained. See [IT procedures](R038_OPERATIONS_IT.md) and [EN procedures](R038_OPERATIONS_EN.md).
