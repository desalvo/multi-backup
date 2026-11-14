# Explicit backup protection — native Rust r036

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

Deleting an unprotected logical backup removes its **manifest only**.
Existing ciphertext chunks, stripe metadata and parity are kept intact:
they may be shared with protected and other active snapshots. A future GC
must reconcile all live manifests/protection markers and parity references
under the same exclusive lock, then apply the configured quarantine/grace
window before removing any orphan. Do **not** run a filesystem-wide manual
cleanup by age or filename.

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
even when its retention window has expired. The earlier 30-day retention
and daily garbage-collection **policies are planning objects**; the autonomous
scheduler and full safe deletion/GC execution have **not** been completed
by this patch.

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
`retention-preview`. The full recurring scheduler and garbage collector
are not yet operational.
