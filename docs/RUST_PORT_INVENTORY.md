# Multi Backup — Rust-native functional acceptance inventory (r038)

Goal: a compiled, independently usable Rust app with fully functional new backup/restore, native GUI and executable provider plugins. Python-created archive data migration is optional and is no longer a release blocker by explicit user decision. A green CI alone is not complete functional qualification.

| Functional area | Rust state | Still required for full release |
|---|---|---|
| Progress gradient, CI backfill admission | Operational native ABI | Keep parity tests |
| Streaming age-encrypted tar/gzip archive operations | Native create/list/verify/atomic extraction plus audited local delete with typed confirmation; Mac/Windows browse/restore routing | Live large-file and all-platform recovery |
| Protected vault v1 (scrypt/AES-256-GCM) | Native create/authenticate/recovery/password change, session-only in-memory age key, Mac Touch ID opt-in via compiled LocalAuthentication helper | Physical Touch ID acceptance, Windows Hello, migration from damaged vault and rollout certification |
| Dropbox 4-MiB content hash | Native streaming kernel and preliminary read-only Rust client | Real-provider E2E and generic adapter integration |
| Backup cycle checkpoints | Native schema-1 audit, exclusive lock, Start/Resume, durable journal write/commit and finalization with Python-compatible hashes | Crash-injection and production provider checkpoint recovery E2E |
| GitHub backup plugin | Native Rust repository discovery, Git mirror including LFS, metadata exports and Actions backfill, verified ciphertext reuse, audit-controlled retention of 5 distinct snapshots | Complete Python metadata/CI cleanup policy parity, external live-account E2E and repeated restore certification |
| Dropbox backup plugin | Native read-only TLS listing/download, revision validation, encrypted object and manifest writer, explicit Start/Resume implemented in Rust CLI | Real-provider E2E and upgrade/migration qualification before default switch |
| GitHub/Dropbox remote restoration | Rust CLI explicit CREATE-NEW, fresh GitHub repo with LFS and Dropbox add-only upload sessions, GUI modal, protected vault identity passed in memory | Real-provider restore E2E, interruption/rollback and independent parity audits |
| Native GUI and graphical About | Standalone egui/Rust desktop prototype with photographic dashboard, hamburger, About, archive/restore/vault and direct native provider actions; CI graphical smoke in Xvfb | Complete pixel parity, accessibility, full feature coverage and signed macOS/Windows/Linux installation |
| Installer delivery | True Rust-only DMGs (ARM64/Intel), Windows Inno Setup/ZIP, Ubuntu 24 tar and target-built Rocky/AlmaLinux EL8/9/10 RPM previews in Production-Gate-dependent CI jobs | Confirm all six RPMs, final Mac/Windows installed app parity, Developer ID notarization and stable signing |
| Native update management | Rust-only official release discovery and HTTPS+SHA-256 verified nonexecuting staging in GUI/CLI | Signed stable promotion, rollback and actual native update assets |
| Startup vault enforcement | Native Rust GUI locates legacy per-user protected identity and requires password/Touch ID before other controls; backup requires an unlocked vault | Physical biometrics and recovery of damaged or obsolete identity migrations |
| Native functional scope | Partial, old Python archive migration OPTIONAL | Live GitHub/Dropbox backup/restore, UI/GC/scheduler reliability, signed installer and update rollback |

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

Last distributed version is r018 Python/Tk. The native Rust r038 code in draft PR #10 is a development preview, NOT a qualified RC. Rust-only release requires functional new-format recoverability, live provider accounts, installer signing, scheduler/GC reliability and safe update/rollback. Historical Python archive conversion is optional. Current procedures: [IT](R038_OPERATIONS_IT.md) · [EN](R038_OPERATIONS_EN.md).


## r038 functional plugin progress

The Rust generic adapter now actually runs POSIX→POSIX encrypted backups and verified selective/full POSIX restores. Linux sources from *existing read-only mounted snapshots* (NFS, SMB/CIFS, SSHFS and GPFS CES) can also feed the encrypted POSIX store, subject to exact mount probe, read-only flag and explicit operator assertion of snapshot consistency. The snapshot assertion is not an independent snapshot/ACL check, so the remote paths remain unqualified development previews. All other 32-entry catalog combinations remain planned unless explicitly implemented. 5 preview backup pairs, 0/682 certified generic pairs; full GitHub/Dropbox provider and native GUI feature completion remain open.

Retain legacy format converter optionally, but stop requiring old Python archive wire-format roundtrip for RC promotion. Qualify Rust-created data roundtrip, encrypted integrity, source identity/key protection, crash recovery, new/isolated target restore, security and signed updates instead. Historical r018/r019 notes below are not the current release policy.


## r038 — functional additions

| Function | Implemented in development | Remains open |
|---|---|---|
| Full/incremental snapshots | Complete encrypted file-reference inventory, parent lineage and deduplicated chunks | Crash/fault/large-data tests |
| Backup details and verification | File count, uncompressed/zstd/zstd-deduplicated sizes, changed/deleted items; full-chain verify default | Real backup/restore certification |
| Safe purge | Descendant reparent or child promotion to full, no physical chunk deletion | Multi-manifest power-loss transaction |
| GC | Separate preview and confirmed local purge of orphan ciphertext, parity checks, grace | Crash-safe mixed-stripe repack, remote GC, WORM |
| Scheduler | Rust one-shot/daemon and persistent job state | OS service/GUI persistence, alerts |
| Cross-account restore | GitHub target-owner control, Dropbox independent target-write-token verification | Full/partial multi-item provider restores and permission fidelity |
