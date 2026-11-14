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
