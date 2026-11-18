# Multi Backup r218 — Apple iCloud Drive B-In / B-Out (macOS)

Status: **PREVIEW**, not READY. This adapter uses the signed-in user's **local macOS File Provider iCloud Drive**, not a generic Apple cloud API. Apple CloudKit accesses only registered application containers; it cannot enumerate arbitrary personal iCloud Drive files. No Apple ID secrets, iCloud.com scraping or 2FA bypass.

## Working supported local operations
- B-In: coordinate one downloaded ordinary file using macOS NSFileCoordinator, create a private staging copy, and encrypt it to a NEW age archive using the native Rust CLI. No directory recursion or complete metadata/ACL fidelity.
- B-Out: authenticate/decrypt the source age archive, then coordinate a create-only copy into an existing iCloud Drive folder. Local SHA-256 readback verifies bytes, but does not prove Apple cloud replication.
- Recovery: pull an encrypted .age archive into a NEW local file, and verify/restore using the native Rust CLI.
- NOT supported: headless Linux/Kubernetes Apple account access, full iCloud Drive remote API, durable sync receipt, remote version pinning, simultaneous cloud snapshots, remote deletion, remote recursive folder traversal, unattended refresh tokens, or iCloud Drive's private web endpoints.

## Use on an enrolled signed-in Mac

Enable iCloud Drive in System Settings and ensure source files are downloaded locally. Grant the signed application filesystem permission in System Settings.

Compile the helper:
    swiftc -O scripts/macos/multi-backup-icloud.swift -o scripts/macos/multi-backup-icloud
    scripts/macos/multi-backup-icloud --self-test

Back up one file to a new encrypted archive:
    bash scripts/macos/multi-backup-icloud.sh backup 'Documents/quarterly.txt' '/private/backup/quarterly.tar.gz.age' 'age1YOURPUBLICKEY'

Upload the previously authenticated age archive to a new iCloud Drive file:
    bash scripts/macos/multi-backup-icloud.sh push '/private/backup/quarterly.tar.gz.age' 'MultiBackup/quarterly.tar.gz.age' '/private/keys/age-identity.txt'

Pull the encrypted archive and verify with native Rust:
    bash scripts/macos/multi-backup-icloud.sh pull-encrypted 'MultiBackup/quarterly.tar.gz.age' '/private/recovered/quarterly.tar.gz.age'
    multi-backup-native archive list --archive '/private/recovered/quarterly.tar.gz.age' --identity '/private/keys/age-identity.txt'

All paths given for iCloud are relative to ~/Library/Mobile Documents/com~apple~CloudDocs. The 'MultiBackup' target folder must exist. Keep the age private key outside iCloud.

## Testing and READY gate
- GitHub macOS CI compiles the real Swift bridge, runs path traversal refusals and checks Rust provider stage and dispatcher denial. Linux CI cannot validate macOS File Provider.
- Before global READY: dedicated disposable Apple ID on real Mac; signed native app entitlement/permitted file access; online/offline source hydrate and new ciphertext upload; independent cross-device Apple cloud visibility; wrong identity, existing destination, symlink and cancelled sync negatives; age restore verified.
- This bridge is not considered a server-side Linux plugin and must remain PREVIEW until live iCloud checks pass.
