# Multi Backup — Rust migration acceptance inventory (r019)

Goal: a fully compiled, independently usable Rust app that preserves all 0.1.0 encrypted archives, checkpoint identities, protected vaults and provider operations. A green incremental CI is not complete migration.

| Functional area | Rust state | Still required for full release |
|---|---|---|
| Progress gradient, CI backfill admission | Operational native ABI | Keep parity tests |
| Streaming age-encrypted tar/gzip archive operations | Native create, list, verify, atomic safe extraction; routed for installed Mac/Windows browse/new extraction | Live large-file and all-platform recovery |
| Protected vault v1 (scrypt/AES-256-GCM) | Native create, authenticate, export/import recovery, password change | Full Keychain/Touch ID/damaged-vault recovery and rollout certification |
| Dropbox 4-MiB content hash | Native streaming kernel actively used in packaged plugin | Provider transport still Python |
| Backup cycle checkpoints | Native read-only schema-1 audit | Implement mutation, restart/resume, atomic journal, locking |
| GitHub backup plugin | Only scheduler kernel migrated | Git mirror, GH API metadata, CI runs/assets, retention and cleanup policy |
| Dropbox backup plugin | Native read-only TLS listing/download, revision validation, encrypted object and manifest writer, explicit Start/Resume implemented in Rust CLI | Real-provider E2E and upgrade/migration qualification before default switch |
| GitHub/Dropbox remote restoration | Not migrated | New repo/file creation, reviewable E2E with real accounts |
| Native GUI and graphical About | Still Python/Tk, with Rust helper integration | Fully Rust compiled GUI retaining approved photographic mock/accessibility |
| Installer delivery | Rust helper embedded in Mac/Windows PyInstaller bundles | Rust-only Mac/Windows/Rocky/Alma distributions, release signing/notarization |
| Complete app compatibility | Partial | Independent cross-version backups/restores and secure upgrade/rollback |

There is deliberately no percentage: functional areas differ enormously in scope, and a numeric estimate would be misleading.

## Rust-native CLI usage

The executable is named multi-backup-native, or multi-backup-native.exe on Windows. It uses the installed age/age-keygen helpers and requires no Python for the following operations.

- multi-backup-native --version
- multi-backup-native archive verify --archive backup.tar.gz.age --identity identity.txt
- multi-backup-native archive list --archive backup.tar.gz.age --identity identity.txt --json
- multi-backup-native archive extract --archive backup.tar.gz.age --identity identity.txt --destination ./fresh-restore
- multi-backup-native archive create --source ./source --output ./new.tar.gz.age --recipient age1PUBLICKEY
- multi-backup-native checkpoint audit --file .state/active.json --json
- multi-backup-native dropbox hash --file ./downloaded.bin
- multi-backup-native vault verify --file default.agevault
- multi-backup-native vault create --file new.agevault
- multi-backup-native vault export --file default.agevault --destination recovery.agevault
- multi-backup-native vault import --file recovery.agevault --destination imported.agevault
- multi-backup-native vault change-password --file default.agevault

Passwords are prompted without echo by default. Automation may explicitly pass --password-stdin with one line for verify/create and two lines for export/import/change-password. Never include passwords in command-line arguments.

Rust archive operations reject symlinks, unsafe names, malformed archives, duplicate entries and overwrites. They fully authenticate age streams, check SHA-256 when a sidecar is present, and never write unencrypted full archive staging data.

## Release guard

The current r018 early-release on main remains the functioning production-compatible baseline. The r019 development branch is a qualified partial migration, not a Rust-only release. Python must not be removed from delivered builds until the full set of gates passes. See docs/RUST_MIGRATION.md and docs/DEVELOPMENT_STATUS.md for additional detail.
