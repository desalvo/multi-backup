# Multi Backup Rust migration — format and GUI parity gates

Status: **begun, NOT COMPLETE**. This record prevents accidentally claiming
Rust-native functionality where Python is still the working engine.

The Rust crate in rust/ implements and tests the candidate-selection policy
for GitHub Actions. The rest of the application is currently Python/Tk:
GitHub and Dropbox adapters, resumable streaming archives, age crypto,
state checkpoints, manual and automated restore, GUI and native packaging.

Migration gates that must pass before replacing working code:

1. Record the r014/r015 serialized JSON progress and snapshot/checkpoint
   format as compatibility fixtures, retaining account/repository/CI manifests,
   encrypted age recipients, SHA-256 sidecars and vault migration/recovery.
2. Implement CLI options, progress messages, deduplication, resumable journals
   and retention/cleanup policies in Rust. Run old vs new against the same
   fixture sets; compare outputs, deterministic where formats allow.
3. Implement GitHub/Dropbox read-only download adapters, authenticated
   retries, pagination and cancellation. Preserve the conservative
   opt-in-only remote CI artifact cleanup.
4. Reproduce the approved photographic dark-violet dashboard, rounded glass
   antialiased buttons, sidebar, navigation, keyboard interactions, settings,
   archives, Touch ID/Keychain and restore functions in a truly native Rust
   desktop application using an appropriate licensed rendering framework.
5. Certify restoration of *pre-migration* archives with the Rust executable
   using independently created encrypted fixtures, and vice versa.
6. Build, sign, notarize and install-check Mac ARM64/Intel, Windows x64,
   and Rocky/AlmaLinux 8–10; include clean upgrades and rollback tests.
7. Only then remove Python/PyInstaller from distribution.

Do not migrate user private identities or discard an old working vault until
round-trip encryption/decryption and restore parity are proven. Never promise
that a Rust executable exists in a released package until its artifacts
are actually built and tested.

## r018 executable Rust interoperability milestone

The production Rust crate now builds as `rlib` and `cdylib` and exposes two side-effect-free stable C ABI functions: `multi_backup_progress_rgb(f64)->u32` and `multi_backup_completion_percent(u64,u64)->u32`. The Python GUI's real progress colour mapping attempts the compiled, bundled Rust-native kernel first via the constrained `rust_bridge.py` loader. Production Gate builds/releases a native library and runs parity tests against all 101 integral hues, including red/yellow/green endpoints. The Mac and Windows PyInstaller builds embed this compiled core; Linux RPMs retain the Python fallback. No remote credential, vault, or archive path is passed to the Rust kernel. This is a real, bounded native integration milestone, not a completed application rewrite. Keep all numbered parity gates above open until individually qualified.

The active GitHub CI backup scheduler now calls native Rust `multi_backup_ci_target` and `multi_backup_ci_should_schedule` where present, verified against the Python backfill algorithm over full combinations of candidate counts, limits, pending/complete slots and worker capacities. The native library is bundled in both CLI and GUI on Mac and Windows. No change is made to checkpoint or encrypted archive formats, and Linux remains on the equivalent safe Python policy until native EL8-10 build qualification. The other Rust migration gates remain open.

## r019 — Rust-native encrypted archive interoperability (2026-11-14)

- Added `rust/src/archive.rs` and independent `multi-backup-native` Rust CLI. It can create gzip/tar archives encrypted directly with age, verify optional SHA-256 sidecars, list and fully authenticate members, and restore with strict normalized paths, member/size limits, duplicate rejection, link/special-type rejection and an atomic empty destination. It never writes an unencrypted whole archive. Existing Python archives remain the schema; archive bytes differ due gzip/tar metadata but the contents and structure must agree.
- Added a genuinely encrypted Rust↔Python interoperability test (real age keypair, Python-generated archive read/restored in Rust, Rust-generated archive verified/restored by Python), plus unsafe traversal and checksum mutation negative tests. Production Gate now installs `age` and fails unless both round-trips work. The Rust CLI is included as a helper in Mac and Windows native packages; Linux packaging of the helper remains open.
- **NOT yet a full migration.** The Python/Tk GUI, GitHub/Dropbox transfer clients, checkpoint journals and vault remain the production path; cryptography is performed by the existing `age` executable invoked from Rust. Do not disable Python, repoint default backup/restore or claim stable until every workflow's functional and security parity is certified on real providers and all native OS installers.

### Additional native components (r019)

- Read-only Rust `checkpoint audit` parses the existing schema-1 active.json, checks exact cycle format, configuration fingerprint structure and SHA-256/size of all saved step outputs, rejects symlinks and never modifies resume state. The real Python Journal fixture is audited before and after simulated corruption.
- Rust `dropbox hash` implements the documented SHA-256 of concatenated 4-MiB block SHA-256 digests; it is actually used by the Dropbox Python plugin through a restricted, packaged C ABI when present. Python fallback and unit tests compare empty, small, full-block and boundary+1 files. Dropbox network transport remains Python.

### Read-only Rust vault interoperability (r019)

- The independent Rust CLI can authenticate the existing encrypted identity vault schema v1 with scrypt N=32768/r=8/p=1 and AES-256-GCM, verifying recipient-bound AAD and the age private identity syntax in RAM. It never prints, writes, caches or exports the secret; derived keys/private identity use memory zeroization. Passwords are read from the terminal without echo or from an explicitly requested stdin pipe, never argv.
- Real parity test: Python seals a newly generated age identity into an ephemeral vault, Rust unlocks it with the correct password and rejects a bad password. The existing production Python vault remains unchanged and recovery import/export, Touch ID, migration and key rotation still require a separately audited Rust implementation.

### Complete compatible vault management surface in native Rust (r019)

- Expanded the native vault CLI to create new protected age identities without plaintext staging, verify schema-1 vaults, export independently password-encrypted recovery copies, import recovery copies to new vault locations, and change passwords atomically with encrypted rollback. Source and destination overwrite protections, 0600 file permissions, scrypt/AES-GCM AAD and zeroized key/private data are retained. Password input is via non-echo terminal or explicitly opted-in stdin, never argv.
- The real age/Python parity test now exercises Rust-created vaults read by Python, Rust recovery export/import read by Python and a Rust password change that invalidates the old password without changing the recovered private age identity.
- This covers password-based vault file operations only. Platform-specific macOS Touch ID/Keychain, Windows biometrics, migration of existing insecure plaintext identities and in-place recovery of damaged vaults remain guarded by their current Python/Swift implementations until native parity is certified.

### Production native archive route on installed apps (r019)

- The existing Python archive browsing and local extraction API now delegates to the bundled Rust executable in installed macOS/Windows applications only. It uses an explicit trusted local helper path (never arbitrary PATH execution), returns the same list-member JSON, and raises errors on native failures rather than silently returning to a weaker path. New, absent restore destinations are published atomically by Rust; compatibility callers using an existing destination or nonstandard size limits retain Python behavior. Source checkout tests deliberately retain the independent Python parser to avoid circular parity tests.
- The production GUI, plugin transfer implementations, complete checkpoint authoring and native Rust UI remain open; integrating a validated Rust helper into the Python GUI is a transitional migration, not Rust-only delivery.

## r020 checkpoint writer (native Rust)

Rust `journal` now supports start/restart/resume, signed SHA-256 step records and finalization with an exclusive lock and atomic JSON persistence. The exact canonical Python JSON configuration hash (including Unicode escapes) is reproduced, so archived Python v1 checkpoint sessions are not silently renamed or invalidated. A cross-runtime regression starts/records in Rust and validates the same checkpoint with the deployed Python Journal, then completes in Rust. The implementation is internal development until the provider transaction paths and crash/power-loss recovery are E2E certified.

### r020 independent native Dropbox backup workflow

A Rust-only network backup implementation now uses pinned Dropbox HTTPS endpoints, TLS validation, bounded retry/pagination and the existing MULTI_BACKUP_DROPBOX_TOKEN environment flow. It downloads only listed immutable file revisions; validates size/ID/rev/Dropbox 4-MiB content hash before encrypting each object; reuses existing ciphertext only if SHA-256 sidecars validate; preserves the Python-compatible objects/XX/SHA.tar.gz.age path, encrypted manifest schema and an explicit Start/Resume session with immutable manifest filename. No Dropbox remote delete/move/write endpoints are present. The native executable supports 'dropbox backup' and 'dropbox resume' without Python. CI can exercise offline integrity/session tests, but real-account E2E and cross-platform packaged network trust remain required before replacing the default production plugin.

### r020 experimental native GitHub backup

Added Rust `github backup` and `github resume` with the authenticated `gh` and `git` executables, explicit journal naming, repository discovery with pagination, safe owner/repo slugs, bare mirror and git fsck, encrypted per-repository snapshots, read-only metadata and successful Actions CI run backfill. Remote cleanup is disabled by design; failure of required CI data blocks a falsely successful backup. Native provider output is a new source-compatible encrypted archive layout, but an exact replacement for all r018 GitHub metadata fields and retention rules is **not yet certified**. The r018 engine stays the default and can recover every existing archive.

### r020 native Rust GUI prototype, not a Tk wrapper

A separate Rust-only desktop project now exists under `rust-desktop/` using egui/eframe. It loads the existing project photographic banner and logo, provides the dark/violet layout, hamburger, About, operational provider selection, Start/Resume, local archive inventory, age verification/new-destination extraction, protected vault creation/unlock/recovery/password change and live job status. It calls compiled `multi_backup_core` functions directly, not Python. The CI will build the standalone Linux executable and actually open/close a window in an Xvfb software-GL environment. It is an **experimental preview**, not yet approved to replace the r018 pixel-accurate Tk interface: remote restore, automatic update, provider event progress and extensive accessibility tests remain pending.

## r020 — Native Rust-only preview packaging, three operating systems

- Native Rust desktop binaries now have separate runnable preview packaging without Python/Tk/PyInstaller. The Rust entrypoint resolves helpers in a trusted app-local bin folder for Finder/Explorer launches.
- macOS: the new packager builds Apple Silicon and Intel app bundles, native age/gh/git-lfs helpers, icon and Info.plist, ad-hoc signs, creates a single-app DMG, mounts and tests it. Preview is not Developer ID notarized and must not replace stable.
- Windows: the preview packager creates a Rust executable, CLI and native helpers as a ZIP and a per-user Inno Setup installer plus SHA-256 checksums; not production signed.
- Ubuntu 24.04: CI creates a tar.gz with GUI, CLI and native helpers and smoke-tests the packaged GUI with Xvfb/software GL.
- None of the Rust preview packaging jobs publish releases. Actual platform installs, Rocky/Alma RPMs, provider E2E, security audits and upgrade/rollback still require qualification.

### r020 private vault identity in memory — no plaintext identity file

The native GUI now keeps a Zeroizing<age-identity> allocation shared through Arc only for the current application session after a successful vault password unlock. Vault identity paths are never written to plaintext temporary files for local verify/restore: Rust sends the decrypted private identity into age's supported `-i -` stdin pipe, with the encrypted archive a separate named input, and fully authenticates the returned stream. The CLI also supports `archive list/verify/extract --vault PROTECTED.agevault --password-stdin`; integration tests decrypt and restore a real Python-generated encrypted archive directly from a protected vault, and refuse an incorrect password. This covers local restore; remote-provider identity reuse and Touch ID enrollment are still separate acceptance items.

### r020 remote restore with an in-memory protected identity

GitHub fresh-repo mirror restore and Dropbox add-only file restore share the same Rust age reader. Both can now consume the private identity from an already unlocked protected vault via a private stdin pipe without creating a plaintext identity file, or use the external identity-file legacy path. The Rust CLI accepts `restore github|dropbox --vault PROTECTED.agevault` with interactive password or explicit stdin, and the native desktop reuses its zeroizing in-memory session key for all local/remote restores. Explicit remote creation confirmation and existing-data protection remain mandatory.
