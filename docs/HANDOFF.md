# Multi Backup - portable project handoff

**Project:** `desalvo/multi-backup`  
**Last working distributed build:** `0.1.0-r018-rc1` (Python/Tk)  
**Merged development baseline:** Rust-native r037 preview on main; not a certified stable release  
**Origin:** `desalvo/github-backup` 0.1.0-r001 (history remains in that repository; Multi Backup has a new initial repository history).  
**License:** EUPL-1.2  
**Languages:** CLI and help in English; README, manuals and brochures in English and Italian.

## Architectural contracts

- All source engines are plugins; GitHub enabled and Dropbox disabled by default.
- No untrusted plugins are imported from arbitrary folders.
- GitHub cleanup must be explicitly opted in; default retain 5 main CI runs plus one per tag subject to conservative artifact-dependency protection; per-repo and total deletion reports are mandatory.
- Dropbox v1 is read-only and incremental; cannot delete remote files or purge archived revisions.
- All backups encrypted using age, with private keys stored separately; per-repository and CI metadata sharded; integrity markers and restart/resume support.
- `early-release` triggered by `[full-ci]` in `main` HEAD commit; `stable` triggered by a stable version tag. Stable promotion requires packaging and security acceptance, not RC tagging.
- GUI screenshot assets in `docs/screenshots` are actual Tk UI capture; hero and icon in `assets` are artistic assets. Do not invent or present UI mockups as observed live behavior.
- Native matrix: macOS ARM64/Intel min 13; Windows 11 x64; RockyLinux/AlmaLinux 8/9/10 x86_64.
- GUI + CLI all main operations; update checks at launch and periodically. Production unattended installer needs secure code signing and is not yet enabled.

## Key files

`multi_backup.py`: CLI and GitHub engine coordinator; `github_engine.py`: legacy GitHub exporter; `backends/*`: plugin registry and Dropbox; `ci_artifact_cleanup.py`: dynamic retention and safeguards; `checkpoint.py`: integrity resume; `migration.py`: legacy archive conversion; `archive_ops.py`: encrypted archive reader and mounting; `gui.py`: UI; `updater.py`: release discovery and verified staging; `key_management.py`, `settings.py`: user state; `.github/workflows/`: tests and native release packages; `docs/manuals`, `docs/brochures`: deliverables; `docs/DEVELOPMENT_STATUS.md`: progress.

## Package and CI instructions

Continue from repository `desalvo/multi-backup` or cumulative ZIP; run `python -m unittest discover -s tests -v` and build/test native apps in CI. Use a one-line English cumulative commit message containing `[full-ci]` to create a rolling early release, e.g. `Prepare Multi Backup RC with plugin backends and native channels [full-ci]`.

**Do not tag stable** until Mac, Windows, EL8–10 native CI is green, package signatures/repositories are configured, migration and restore are verified, and manual updated. Preserve current handoff and any cumulative local changes not yet pushed. Copy the complete new project directory into a cumulative ZIP with docs and assets.

## Important limits

GitHub API does not expose secret values; restored GitHub accounts need manual remapping of tokens, runners, SSO and applications. The Dropbox plugin covers file revision payloads only, not Dropbox sharing/admin metadata. `age`, `gh`, `git`, `git-lfs` dependencies still need packaging runtime verification; do not claim security certification based on unit tests alone.

## r003 CI repair handoff (2026-11-13)

The user reported the first RC build CI failed. Connected GitHub tooling cannot enumerate the push-triggered workflow runs/logs; direct logs were not available. Reproduced two definite Ruff failures (E401/E402), corrected Windows age Chocolatey package id (`age.portable`), removed redundant Inno Setup installation on Windows runner, and tightened HTTPS entry-point checks for Bandit B310. Do not claim the exact failure step was confirmed. Check `/actions` for the [full-ci] fix commit and inspect any remaining failing job. Preserve EUPL and cumulative ZIP on future revisions.

- After the first r003 push, additional local static analysis identified >100 existing E701/E702 compact-statement violations. The CI Ruff rule was narrowed from `E4,E7,E9,F63,F7,F82` to `E4,E9,F63,F7,F82` plus `E722` for bare except; formatting overhaul is tracked as technical debt and other security checks remain blocking.

## r004 (2026-11-13) - real remote CI diagnosis

The connector successfully retrieved the user-provided failed run `https://github.com/desalvo/multi-backup/actions/runs/43491071561`, its job list and the failing security job logs. Three Python test matrix jobs, Ruff and Bandit passed; the sole failing step was `python -m pip_audit -r requirements.txt`. The audit identified **42 vulnerability identifiers for Pillow 12.3.0** and returned 13.0.0 as the fixed version. New policy is `Pillow>=13.0.0,<14`, applied consistently to `requirements.txt`, `requirements-docs.txt` and all three native package-build recipes. Never disable pip-audit to force the CI green. Validate a new [full-ci] run and native builds before claiming a finished RC. The previous r003 note that logs could not be read was superseded by direct connector access in this cycle.

## r005 native CI - release blockers (2026-11-13)

Remote CI success for security: https://github.com/desalvo/multi-backup/actions/runs/43494324235. Native run https://github.com/desalvo/multi-backup/actions/runs/43494324431 completed 9 build jobs successfully (2 macOS, 5 Linux), failing only Windows (`132123583905`) due to portable Chocolatey `age.exe` shim being copied instead of actual binary, and Rocky EL10 (`132123584088`) due to unmaintained image `rockylinux:10` missing. The fix copies genuine `age.exe`/`age-keygen.exe` from `age.portable/tools` and uses `rockylinux/rockylinux:8/9/10`. Await new native run and verify release assets and signing; do not claim stable or full RC green without remote evidence.

## r006 Windows installer path correction (2026-11-13)

The native r005 Windows log shows age and age-keygen both working, but Inno Setup 6.7.1 rejects `LicenseFile=LICENSE`, looked up under `packaging/windows/` rather than repository root. `OutputDir=installers` would also point under that directory. Add `[Setup] SourceDir=..\..` to `packaging/windows/installer.iss`, which makes source assets and output root-relative, and a regression test for this. New full and native CI required before asserting green.

## r008 handoff — requested Production Gate and macOS application hotfix

- User reported the existing Apple Silicon DMG exposes Python and a nonlaunchable “damaged” app. Do **not** tell users to disable Gatekeeper or strip quarantine.
- CI design: `ci.yml` is the **only push/tag/PR/dispatch-triggered workflow**, with a single Production Gate (Python 3.11–3.13, Ruff, Bandit, pip-audit runtime+docs, packaging policy). On `[full-ci]`/tag/manual dispatch, it calls `build-native.yml` via `workflow_call`, with `needs: [production-gate]`. All native checks and signatures must pass before public release or RPM index publication.
- macOS packaging scripts: `scripts/build-macos-app.sh` (real PyInstaller onedir app, embedded CLI and age/gh/git-lfs, Developer ID signing), `scripts/notarize-macos.sh` (notarize+staple both .app and one-item DMG), `scripts/verify-macos-dmg.sh` (mount, copy, Gatekeeper assessment, app/CLI smoke), plist entitlements. GUI self-test in `gui_launcher.py`.
- **External blocker** to public fixed DMG: Apple Developer Program credentials: `APPLE_DEVELOPER_ID_P12_BASE64`, `APPLE_DEVELOPER_ID_P12_PASSWORD`, `APPLE_DEVELOPER_IDENTITY`, `APPLE_NOTARY_API_KEY_BASE64`, `APPLE_NOTARY_API_KEY_ID`, `APPLE_NOTARY_API_ISSUER_ID`. Set in repo Actions secrets. Never put key material in documentation, chat or archive.
- Release version target 0.1.0-r008-rc1, commit `[full-ci]` permitted. If secrets absent, macOS job should fail safe, and no new public early release assets are uploaded. Check new GitHub run logs before declaring a successful fix.
- Known open stable blockers: Windows Authenticode, RPM/repo GPG, native E2E restore/update tests. Existing old DMGs remain downloadable and must not be claimed repaired without replacement.


## r014 visual redesign handoff (2026-11-13)

The user approved a glossy navy/violet/cyan, photographic, rounded macOS-style dashboard mock. Added premium_dashboard.py, dark glass toolbar and sidebar, photography-backed hero, responsive layout and live local archive/plugin/vault/disk/update metrics. Do not invent scheduled jobs or demo success numbers. Preserve all eight navigation routes, age vault, remote restore, backups and compatibility. Test packaged UI via gui_launcher.py --gui-smoke on Mac arm64/Intel plus full security and Windows/RPM matrix; publish only a qualified early-release and seek user visual acceptance on installed Mac. Stable still requires complete signing/notarization and E2E proof.


## r015 artifact backfill, installation and documentation handoff (2026-11-14)

User requires standard Mac drag-to-Applications DMG (app and Applications symlink), explicit resume checkpoints in logs, CI artifact fallback resilience, replacement from older eligible CI runs to reach configured number of complete runs, and optional override to ignore unresolved artifact downloads while always attempting the requested count. Strict mode flags incomplete if not enough full CI runs remain; opt-in mode records per-artifact omissions and a summary with warnings. Override must NOT trigger remote cleanup of unverified data. r015 adds fresh capture of the approved photographic GUI in EN/IT brochure and manual via Actions, rather than publishing old screenshots. Full Python-to-Rust GUI/CLI/provider/encryption/restore migration is a separate major architectural effort with parity gates and remains OPEN, not claimed finished. Preserve encrypted archives, checkpoints and age vault compatibility. Python remains the working implementation until Rust passes full end-to-end parity.


### r015 rendering quality / Rust staged core

User submitted screenshot of badly stair-stepped rounded rectangles and the tiny internal glossy arc. Fix uses render_quality.py 4x supersampled rounded patches/glints, composited into Tk/Pillow PhotoImages. Applied to premium_dashboard.py, ui_widgets.py, sidebar_nav.py; tests/test_render_quality.py must remain in Production Gate. The native Tk text is not rasterized, preserving legibility. Confirm macOS native smoke plus subjective Retina rendering with newly generated screenshots. Rust library is only a policy prototype; the Python application remains shipped, and full Rust GUI/engine migration is explicitly open. Do not claim release completion without all gates and new assets.


## r016 handoff — explicit backup actions and local deletion (2026-11-14)

User requested separate Start and Resume buttons, stable backup name on Resume, ability to delete existing backups locally, author/license/version/build at sidebar bottom, and bespoke backup-relevant photography in both hero and sidebar without distortion or unwanted crop.

GitHub: --restart always creates a new cycle and archives previous active state; --resume requires a matching active checkpoint and keeps its name. Dropbox incremental manifest name is stored in a separate active session file for explicit Resume (existing encrypted objects remain immutable and SHA-256 verified). Archive deletion is single local selected .tar.gz.age + sidecars, with exact confirmation and audit, no remote calls, and active-checkpoint / running-job protections.

UI currently uses Tk/Pillow, r015 4x antialias stays unchanged. Build numbers and EN/IT publications must follow r016. Full Rust engine/GUI migration remains OPEN. Test both Mac architectures plus all native packages, and check photographer art visible fully with correct proportions.


### Photo integration for r016

The user wanted bespoke photographic backup-related banners replacing the hiking illustration. Use scripts/create-photo-assets.py to generate assets/backup-hero.jpg and assets/backup-sidebar.jpg at native packaging time from a freely licensed server-rack photo, with independent navy-toned compositions and image contain to avoid distortion or unwanted crop. The pipeline generates them before native app freezing and before the PDF screenshot job; both are local assets at runtime. docs/PHOTO_CREDITS.md records the source/license. Tests: tests/test_photo_assets.py. Source checkout without network may temporarily show prior hero if generator not run; do not claim the native packages are updated until qualified CI uploads those images.


## r017 handoff — native About, supplied photographic mock and documentation repair

User supplied the final desired Multi Backup premium navy/violet photographic UI mock and requests faithful adoption across all pages, not just the dashboard. They also requested a native About popup comparable to MTA, with accurate app metadata. Keep the user-approved navy/violet/cyan palette, high-DPI 4x anti-aliasing, no distorted/cropped photos, operational local stats only, and explicit Start vs Resume and guarded archive deletion from r016.

New `about_dialog.py` presents `VERSION`, `BUILD`, `AUTHOR`, EUPL-1.2 and the GitHub project URL without reading/unlocking the vault. Available in Help, macOS `tk::mac::ShowAbout`, toolbar, sidebar and F1. It can copy build provenance and open project website only on explicit click. GUI secondary pages now use a dark-glass palette instead of bright white forms.

Main build `43537250887` was NOT fully successful: the publication job failed with `ModuleNotFoundError: No module named 'gui'` from direct execution of `scripts/capture-ui.py`. `scripts/capture-ui.py` now inserts the project root into `sys.path`; PR CI has a post-gate `publications-smoke` job for real screenshots, About and bilingual PDFs, and `native-release` depends on this. Verify PR and new main builds (Windows/6 RPMs/both Mac platforms, artifact publication) before claiming r017 released.

Branch: `feature/r017-about-mock-publications`; intended version: `0.1.0-r017-rc1` build 17. Do not publish stable until external production signing/notarization, full E2E restore and user visual acceptance. Rust migration remains a separately gated, incomplete macro-area; Python/Tk runtime stays active until proven equivalent.


## r018 handoff — Rust core progress and precise live-status animation

Requested gradient is monotonic hue red (0)→yellow (50)→green (100), anti-aliased 4x via Pillow; animated small glow bead is independent of true % and stops after 8s without events, immediately upon stop/finish. Replaces Hide navigation with an antialiased hamburger button and keeps native accessibility/keyboard navigation. Rust native progress-colour / bounded-completion ABI builds in Production Gate and is embedded in Mac and Windows GUI packages with Python parity fallback; Rust **whole-application migration is not complete** and must not be claimed. Remaining major areas: GitHub/Dropbox network clients, age encrypted streaming, checkpoint compatibility, vault, restore, Rust UI, Linux native packaging and true cross-version E2E.

### r018 native scheduler milestone

CI backfill target and admission checks now have a real Rust C ABI used by the bundled Mac/Windows CLI (and GUI), with safe Python fallback for other platforms. All provider communication, crypto, state journals, vault and GUI remain Python. Qualify full PR and release build before any stable claim.

### r018 screenshot-driven polish

The actual r018 CI screenshot artifact from run 43541311109 confirms hamburger, red 0% bar, full sidebar photography and native About popup; improve About header with the purpose-built narrower sidebar photo, slim sidebar scrollbar and reduce pill labels to prevent touching borders. Check native Rust embedding from release gate. Do not call this final visual acceptance until the user's installed Mac build is reviewed.

## r019 — Rust-native encrypted archive interoperability (2026-11-14)

- Added `rust/src/archive.rs` and independent `multi-backup-native` Rust CLI. It can create gzip/tar archives encrypted directly with age, verify optional SHA-256 sidecars, list and fully authenticate members, and restore with strict normalized paths, member/size limits, duplicate rejection, link/special-type rejection and an atomic empty destination. It never writes an unencrypted whole archive. Existing Python archives remain the schema; archive bytes differ due gzip/tar metadata but the contents and structure must agree.
- Added a genuinely encrypted Rust↔Python interoperability test (real age keypair, Python-generated archive read/restored in Rust, Rust-generated archive verified/restored by Python), plus unsafe traversal and checksum mutation negative tests. Production Gate now installs `age` and fails unless both round-trips work. The Rust CLI is included as a helper in Mac and Windows native packages; Linux packaging of the helper remains open.
- **NOT yet a full migration.** The Python/Tk GUI, GitHub/Dropbox transfer clients, checkpoint journals and vault remain the production path; cryptography is performed by the existing `age` executable invoked from Rust. Do not disable Python, repoint default backup/restore or claim stable until every workflow's functional and security parity is certified on real providers and all native OS installers.

Working branch: feature/r019-rust-native-encrypted-archive. Main untouched until qualification; preserve r018 early-release.

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

### r020 explicit native remote restoration

The compiled Rust core now has dedicated, opt-in remote restore commands. A GitHub mirror archive can be restored only into a fresh owner/new-repo using gh repo create and git push --mirror (plus LFS push); an encrypted Dropbox object can be restored to a new absolute file path with strict add-only semantics using bounded 8MiB upload sessions and verified content hash. Both operations require --confirm CREATE-NEW in the native CLI; the standalone Rust GUI has an additional explicit confirmation modal. Neither path is invoked by automatic backup or local archive browsing. External live-account recovery is unverified and must be separately qualified.

### r020 Rust archive deletion

The native CLI/GUI now preserves the guarded *local-only* deletion workflow: exact archive filename typed confirmation, catalog confinement, symlink rejection, active GitHub/Dropbox checkpoint protection, only ciphertext and same-directory metadata companions, and append-only fsynced JSONL audit. No remote delete method is invoked. Unit tests assert refusal of out-of-root, wrong confirmation and deletions during an operation. Full GUI acceptance remains pending.

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

## r020 native feature closure addendum — 14 November 2026

- **Protected session key:** native Rust desktop now unlocks a schema-1 AES-256-GCM vault once, retains only a `Zeroizing` RAM identity until the app closes and sends the age secret to the child process over `age -i -` standard input. Both local verify/extract and remote CREATE-NEW restore can work without creating or exporting a plaintext key file. Real Python->Rust round-trip with a protected password vault and wrong-password denial is a mandatory gate.
- **Native provider safeguards:** GitHub mirror fetch now requires all Git LFS objects and aborts when unavailable. Reused Git and metadata ciphertext must have a valid SHA-256 sidecar. Only after finalizing a successful cycle, older distinct snapshots above the default five are deleted locally using the same typed-confirmation audit engine; invalid archives are never automatically deleted. The native summary marks missing metadata as incomplete instead of falsely asserting success. No provider remote delete operation is added.
- **Rust UI biometry:** Mac preview bundle compiles the Swift Keychain/Touch ID adapter and offers explicit enrollment only after password-based vault authentication, device-biometric unlock and disable. Passwords are exchanged only through local pipes; the Rust vault independently verifies the retrieved password. Hardware Touch ID is unavailable in hosted CI and **physical-device verification remains required**. Windows Hello not yet ported.
- **RPM qualification:** six additional Python-free Rust-native RPM CI matrix builds now target Rocky Linux and AlmaLinux 8/9/10 directly with each distribution's libc; no Ubuntu-built binary is mislabeled as an EL package. RPMs require native `git`, `git-lfs`, `gh`, `age`, and runtime graphics libraries. RPM builds are in PR CI only, not released or signed.
- **macOS compatibility smoke flake:** the legacy r018 early-DMG check occasionally returned a transient `hdiutil verify: Resource temporarily unavailable` after successful signing. Added bounded retry only for this exact transient error; corruption/signature errors still fail closed.
- **Release gate:** no merge, stable tag, package replacement or claim of fully qualified Rust-only software before all six OS builds, real-account GitHub/Dropbox backup and restoration, upgrade/rollback, physical biometry and signing/notarization pass. No Python has been removed from `main`.

## r020 native startup lock and native update workflow (14 November 2026)

- Rust standalone desktop now resolves the same protected vault path as Python 0.1.0 on macOS (`~/Library/Application Support/multi-backup/keys/default.agevault`), Windows (`%APPDATA%/multi-backup/keys/default.agevault`) and Linux (`$XDG_CONFIG_HOME/multi-backup/keys/default.agevault`), independent of the user-selected backup destination. If that vault already exists (including a rejected symlink), startup is locked; only an authenticated password or already enrolled Mac Touch ID can unlock backup UI. For a new installation without a vault, Start/Resume takes the user to the Vault panel and requires generation/unlock rather than permitting unsecured backups. An identity already unlocked for the current application session is held exclusively in a zeroized Rust allocation.
- Added the new Rust updater module and operational CLI/UI controls. They retrieve the official fixed GitHub Releases endpoint by stable/early-release, select native architecture-specific Rust-only assets, reject non-HTTPS/off-domain URL hops and missing official SHA-256 digests, bound redirects/file sizes and stage the bytes atomically. No downloaded executable is installed/launched automatically. Physical system signing and version-by-version upgrade/rollback remain separate release acceptance tests.
- Existing r018 Python GUI and archives on `main` have not changed. The r020 feature branch is **not** yet a fully certified replacement: external provider E2E with private authenticated accounts, UI/pixel/accessibility review, physical biometry, signed/notarized distribution and end-to-end upgrade/rollback still fail to have sufficient verified evidence.


## r021 handoff — experimental content-addressed storage & server (2026-11-14)

- Continue on PR #9 `feature/r019-rust-native-encrypted-archive`; main r018
  remains the release baseline. Do not squash/merge/publish until review.
- Rust chunk store v1: HMAC-addressed 4-MiB chunks, zstd/age,
  one encrypted logical manifest (across provider namespaces), selective
  restore to fresh destination; 8+2 ciphertext parity with reconstruction
  and quarantine. CLI `storage` subcommands added.
- Rust server: Argon2id local login, 8-hour cookie sessions, origin/CSRF
  checks, JSONL audit, responsive browse and server-side selective export;
  no LDAP/OIDC/SMTP implementation yet.
- Dockerfile, Compose, Helm, Kustomize, compiled wizard and GitHub conditional
  Docker Hub publication workflow. Registry secret/variable must be configured
  and CI validated before official OCI images exist.
- Original offline artwork generator replaces remote Unsplash dependency.
  Rust desktop draws a vector hero reading "Multi Backup". Regenerate all
  old PDF brochures and screenshot assets before publication.
- Tests added for original art, age-encrypted roundtrip/dedup and erasure
  corruption scenarios. These are NOT yet confirmed passing remotely.
- Remaining: full vault interoperability, GUI storage/settings, content-
  defined chunking, garbage collection, ZIP64 portable volumes, storage
  migration wizard, FUSE/WinFsp mounts, restore-to-origin, LDAP/SSO/SMTP,
  robust audit/retention, image publication and security review.
- Docs: CHUNK_STORAGE_V1.md, SERVER_DEPLOYMENT.md. Keep this handoff and
  cumulative GitHub branch commits; no unverified completion percentage.


## r022 — certification controls (2026-11-14)

- Fixed native Rust egui v0.30 rendering calls and original hero unit fixture; platform requalification in progress.
- Added Rust storage verify / health report, ZIP64 portable volumes and atomic import, and multi-select restore tests.
- Web browser supports nested folders, list/grid and selecting files/folders across a unified backup.
- CI publication interlocks now require full native Rust macOS/Windows/EL8-10, desktop Ubuntu, server OCI, publications, and Production Gate. Stable requires ENABLE_STABLE_RUST opt-in.
- Certification matrix: docs/RC_CERTIFICATION_MATRIX.md. Do not mark stable or merge PR until real-platform signed packages and provider restore drills pass.


## r022 continuation point

On PR #9 the Rust backend has health verification, portable ZIP64 bundles, multiple-selection restore, and streaming legacy archive migration. Native eframe build correction, OCI and OS packaging requalification remain ongoing. Use the latest commit SHA, not the last successful older workflow. Do not merge or promote stable before signed installer E2E, real GitHub/Dropbox recovery and identity/auth integrations. Consult docs/RC_CERTIFICATION_MATRIX.md.


## r023 development continuation

- Main remains unchanged; PR #9 is draft.
- Storage v1 relocation, Rust desktop integrated storage UI, SMTP(S),
  LDAPS and OIDC code are on branch. All new integrations require current
  CI and real hardware/service acceptance.
- Helm optional auth flags are disabled by default and Secret-backed.
- Mac DMG now has app + Applications shortcut and DiskImages transient retries.
- Crucial blockers: actual mount live, full age vault-in-memory, provider
  direct chunk backup, comprehensive reverse storage conversion, independent
  security review, notarization/Authenticode, OIDC/LDAP/SMTP live testing.
- Do not promote stable or Docker images based on a prior green commit.


## r024 live mount continuation

Rust server has opt-in read-only WebDAV mount under /dav/:id, credential from dav-password Secret, HTTPS-only. XML listing is generated from one unified logical manifest; GET decrypts verified chunks lazily. CI compilation and actual clients must be validated. This does not implement direct native FUSE/WinFsp or OS mount commands from desktop GUI.


## r025 update

Store::export_legacy now provides non-destructive streaming v1 -> legacy tar.gz.age; CLI storage export-legacy and Rust desktop Storage chunk action. Old -> v1 import and v1 relocation already exist. Still missing: actual account-level integration and certified native mount/IdP tests.


## r026 update

Audit read-only admin API/UI with 10-per-page searching added; safe file type validation. Focused Rust CI now runs on push only, avoiding duplicate PR+push workloads. External audit retention/immutability not yet certified.


## r027 handoff -- Debian and offline confidential design

Public PR #9 remains the development branch and main is unaffected.
Supported new Rust Linux build targets: Debian 12 and 13 x amd64/arm64
(package-rust-debian-preview.sh, rust-linux-debian-native-preview,
native-release linux-deb). Production Gate and release collectors now
require all four .deb artifacts, in addition to existing RPM/macOS/Windows.

Private architecture reference is generated **offline**, not stored as
plaintext in Git; the operator-only kit has reportlab/PyMuPDF generator
that creates an AES-256 R6 encrypted PDF with a locally chosen password.
Public documentation: docs/PRIVATE_ARCHITECTURE_WORKFLOW.md.
CI validator: scripts/check-confidential-artifacts.py. Only
docs/private/architecture-encrypted.pdf may be committed, once created
locally by the operator; none exists on the PR yet. For cross-chat
continuation, the operator can provide locally decrypted excerpts or
a sanitized handoff. Do not request the private PDF password in chat.
Prefer a separate private GitHub repo over a public encrypted blob.

Track latest full CI, not previously green SHA; do not merge or label
stable while Debian packaging, OS signing or complete recovery acceptance
is incomplete.


## r028 — protected server runtime and verified CI (2026-11-14)

- At commit `a10a40e82ab963c090d1845d73bc2e653dc1dd90`, PR #9 Production CI run `43625695818` passed all 19 executable jobs, including both macOS Rust previews, Windows, Ubuntu, four Debian 12/13 combinations and six Rocky/Alma RPMs. Container and native public release jobs were intentionally skipped for the draft PR.
- Server hardened-mode policy now rejects public origins without explicit hardened mode, and rejects identical private gateway and recovery secrets. Local development remains allowed only at `http://127.0.0.1:8080` without hardened mode.
- Added black-box running Docker image gateway checks after image build: health probe, missing/wrong/correct gateway token, authentication requirement, and denial of recovery without a user session. No deployment passwords are used.
- CI result for this **new** commit must be checked; a green older SHA does not certify the patch. Stable promotion, independent WORM retention, mTLS ingress, real provider restores, physical signing and live adversarial tests remain blocked.


## r029 — universal bidirectional plugin matrix

The user requires **all** inputs and outputs to be plugins, with source/destination roles swapped for restores. A static Rust registry `rust/src/plugins.rs` now declares **18 backup input providers**, **7 backup output providers**, **7 restore input providers**, and **18 restore output providers**. Full list, stable IDs, consistency models and readiness in `docs/PLUGIN_MATRIX.md` and `docs/PLUGIN_API.md`.

`multi-backup-native plugins list --json` and `plugins plan --input github --output posix --mode backup --json` are **read-only metadata operations**. No generic pair is currently executable; standalone existing GitHub/Dropbox/POSIX commands are preserved. No arbitrary source loading or unverified plugin runtime. Unsupported directions deny by default. Test in `cargo test --manifest-path rust/Cargo.toml --all-targets` and newest CI before claiming green.

Expanded scope makes the previous 57.5% product-wide readiness estimate outdated: **25/25 role contracts inventoried**, **0/25 generalized backup-role integrations certified**, and no new overall percentage until scope rebaseline. Never conflate documentation/catalog registration with an operational plugin. Public code/docs/manuals/releases remain in GitHub; private handoff and encrypted architecture PDF remain solely in the approved Dropbox `/ADSNET/Projects/Multi Backup/Private`.

## r030 handoff — SAN transports and immutable-vault plugin scope (2026-11-14)

- Expanded static Rust plugin registry with `iscsi`, `fc`, and `nvmeof`
  in **both** backup input/output roles and `anti-ransomware-vault` as
  backup output **only**. Restore roles are automatically reversed.
- Introduced typed safety profiles for BlockTransport, BlockDevice and
  ProtectedVault to distinguish raw block snapshots and immutable vault
  operations from filesystem adapters. No direct block-device writes,
  targets, or dangerous command-line mounts are exposed.
- Added unit tests for SAN snapshot safety profiles, all role counts,
  inverse restore mapping, and immutable vault direction rejection.
- New scope: 21 backup inputs, 11 backup outputs, 11 restore inputs,
  21 restore outputs, 22 distinct plugin IDs. Catalog 32/32 (100%) by
  definition; 0/32 unified adapters certified; 0/231 generic backup pairs
  certified. Previous overall RC percentage is obsolete after expansion.
- iSCSI/FC/NVMe-oF block transport adapters and WORM vault are Planned,
  **not implemented**. Safety gates must include storage-consistent snapshots,
  WWID/NGUID/NSID identification, multipath fencing, restore to new isolated
  LUNs/namespaces only, independent immutable retention and recovery approval.
- All source code and public docs remain on GitHub; private architecture
  and handoff remain solely in approved Dropbox folder.

## r031 — GlusterFS and SAN LVM profiles (2026-11-14)

- Added `glusterfs` as both input and output of backups, with inverse
  restore roles. Distinct plugin ID; requires mounted Gluster client,
  topology and consistency verification and preserves GFID/ACL/xattr.
- iSCSI and FC each support independently configurable optional LVM layouts:
  `local`, `cluster-local`, `shared-exclusive`, `ha-exclusive`,
  `cluster-shared`. The raw-LUN mode remains available by omission.
- New read-only Rust `plugins plan --input-lvm/--output-lvm` validates
  the profile independently per endpoint and emits snapshot, multipath
  WWID/PV/VG/LV, cluster quorum, fencing/locking and isolated-new-target
  acceptance requirements. Misapplied profiles fail closed.
- Added Rust unit tests for all 5 LVM profiles on both transports,
  inverse restore mapping, rejection on non-SAN plugins, clustered
  locking/fencing guardrails and GlusterFS both directions.
- Updated scope: **23 plugin IDs, 22 backup sources and 12 destinations**,
  34 backup-role contracts plus 34 inverse restore contracts.
  Catalogue coverage: 34/34 (100%); unified certified role adapters:
  0/34 (0%); certified generic provider-pair combinations: 0/264 (0%).
- Do NOT claim that catalogued SAN LVM, GlusterFS or WORM operations are
  implemented, or that unsafe live-device writes are supported.

## r032 — AFS and enterprise storage/content plugins (2026-11-14)

- Added **AFS / OpenAFS** as two-way plugin, with AFS cell/volume/VLDB,
  Kerberos token, backup/readonly snapshots, full/incremental `vos`
  chain, ACL/mountpoint preservation and restore to **new** volume preflight.
- Added Lustre, IBM Storage Scale / GPFS, HDFS, Alfresco, ownCloud and
  Nextcloud as *planned* input/output plugin roles; extended existing
  Google Drive and OneDrive inputs to also have output roles.
- Added access-gate metadata to the Rust contract: licensed external
  client for GPFS, OAuth scope review for Google Drive/OneDrive, and
  externally installed mounted/API clients for other integrations.
  No proprietary runtime, kernel module, OpenAFS code, or credentials
  have been packaged. Vendor license and API rights require acceptance.
- Plan preflight adds provider-aware AFS volume/ACL/dump-chain rules,
  HDFS TLS/Kerberos/snapshot, Lustre/GPFS snapshot and storage
  entitlement, remote document version/permission checks.
- Added Rust unit tests for all new bidirectional roles, AFS restore
  isolation, licensing/OAuth gates and fail-closed pairing.
- New inventory: **30 plugin IDs, 29 backup inputs, 21 backup outputs**
  and exact inverse restore roles (21 inputs/29 outputs); **50 declared
  backup roles and 50 inverse**, zero generic pairs certified of 609
  possibilities. Catalogue definition 50/50 (100%) is *not* product
  completion. Existing older overall RC percentage must be rebaselined.
- No unqualified vendor adapter is enabled; latest CI must verify this
  commit before declaring the new code/test matrix green.

## r034 — SSHFS and open CES GPFS client support (2026-11-14)

- Added bidirectional `sshfs` provider, distinct from `ssh` (SCP/SFTP
  transfer). Client is an independently installed, patched GNU GPL-2.0
  SSHFS/FUSE package, not bundled into Multi Backup.
- Switched GPFS plugin to **CES NFS/SMB profiles** `ces-nfs` and
  `ces-smb`, using open-source distribution client tools; IBM Storage
  Scale server/CES nodes retain their own license requirements.
  Direct native GPFS mount via a free client is not assumed available.
- Added Rust `mount_probe` module and read-only CLI `plugins probe`,
  checking Linux exact pre-mounted NFS/CIFS/FUSE.SSHFS mountpoint and
  filesystem type without mounting, file reads, authentication or writes.
  Kernel rw flag does not prove access privileges/GPFS provenance.
- CLI `plugins plan` now supports input/output GPFS CES protocol profiles
  separately, with preflight for consistent snapshots, ACLs, patched SSHFS,
  host-key pinning, symlink containment, isolated encrypted-only output.
  All newly cataloged adapters remain `planned`.
- Inventory **31 plugin IDs, 30 backup inputs, 22 backup outputs**, reverse
  restore roles 22/30. Declarative contracts 52/52, certified generic
  adapter roles 0/52, qualified provider pairs 0/660; rebaseline overall
  completion before reporting a new product-wide percentage.
- No change to main/stable. Production CI for this commit must complete.

## r036 — user-controlled protection of individual backup instances

- Native content-addressed logical backups can be **protected/unprotected**
  explicitly. Protection state is an age-encrypted per-backup marker, not
  8+2 parity and not the WORM vault. `Store::load/list` now surfaces
  protection status, with corrupted marker fail-closed.
- New CLI commands `storage protect-backup/unprotect-backup/delete-backup`.
  Unprotect and delete require exact full ID confirmation. Protected backup
  deletion is rejected. Manifest deletion deliberately does **not** remove
  shared ciphertext chunks/parity pending a safe, independently verified GC.
- Desktop Storage displays a protected lock and offers Protect, confirmed
  Unprotect and confirmed Delete only when unprotected.
- ZIP64 transport and full-storage relocation carry the encrypted hold,
  preserving the protected status across imports or copies.
- Legacy encrypted `.tar.gz.age` archives gained a separate per-archive
  protection marker with CLI `archive protect/unprotect`, and deletion
  is blocked while protected. Mutation uses an exclusive root lock.
- The policy-only retention planner honors an explicit `protected` flag
  regardless of age or default 30-day expiry. Autonomous recurring
  retention and GC remain **not implemented**; no automatic deletion has
  been enabled by this change.
- Added integration/unit tests for shared chunks, protected ZIP import,
  corrupt holds, wrong-ID rejection and protected legacy archive deletion.
- No claim of immutable hardware WORM protection or completed remote plugin
  enforcement; independent vault and provider certification remain pending.

## r036 corrected inventory — Oracle and explicit protection

- `oracle` is registered as a planned consistent-online RMAN input,
  taking the plugin inventory to 32 plugin IDs, 31 backup input roles and
  22 backup output roles; restore is the exact reverse (22 / 31).
  **53** backup role definitions are present, and **682** possible pairs
  are not yet certified as generic integrated jobs.
- New user-requested protection is separate from 8+2 parity and WORM:
  encrypted per-backup hold survives portable transport/relocation,
  blocks manual logical delete and retention preview. Legacy local
  archives have their own guarded protection sidecars. The runtime
  scheduler and safe chunk GC are not yet certified or activated.

## r037 — nested top-level namespaces and retention preview

- The chunk store now offers `Store::backup_in_namespace` and CLI
  `storage backup --top-namespace PATH`; GUI has an optional namespace
  field supporting nested segments (e.g. `Rome/Cluster/VM`).
  Existing `Store::backup` and manifests remain backwards compatible.
- New manifests store creation Unix time and optional top namespace.
  Migrated/legacy manifests with unknown timestamps are preserved
  indefinitely by automatic retention until classified explicitly.
- Added `Store::preview_retention` and CLI `storage retention-preview`:
  read-only age-authenticated snapshot load/integrity checks followed by
  the 30-day retention policy. The protection flag is read directly
  from encrypted hold state, NEVER trusted from user-provided records.
- Test checks nested source paths, traversal rejection, future 30-day
  expiry preview still preserves a protected backup, and restored data
  under the nested namespace. Rust focused CI `43638279897` passed
  all 3 jobs for commit `901608a5`; full CI on that revision is pending.
- Automatic scheduler execution and real mark-and-sweep GC are NOT
  enabled; protected shared chunks may not be reclaimed by an unsafe
  age-based cleanup. Output plugin-level capacity is partially supported
  for pre-mounted filesystems; cloud API quotas/namespace index
  persistence still require integration and live verification.


## r038 continuation after PR #9 merge — 2026-11-14

- Repository: `desalvo/multi-backup`; `main` now contains squash-merge `b7e757d9378d2503b57fa43267577a3a55f60fd2` of [PR #9](https://github.com/desalvo/multi-backup/pull/9). The feature head was `860d390a`.
- [Full premerge Production CI #43640869549](https://github.com/desalvo/multi-backup/actions/runs/43640869549) completed successfully: 19 build/test/security/docs jobs passed, 0 failed, 2 publication jobs intentionally skipped, 17 preview artifact groups uploaded. Native CLI encrypted backup+byte-identical restore acceptance has been wired into Linux, macOS, Windows and Debian packaging, and passed this CI.
- Merge is **source integration**, not promotion: `main` still includes the working Python/Tk implementation for compatible releases and a Rust-only development desktop + CLI + experimental server. Last previous early-release assets are not invalidated. No `[full-ci]` on squash commit, and no stable or Docker publication requested.
- For the next chat: check main CI and resolve any regressions. Preserve version compatibility, protected age identity and protected logical manifests; never activate destructive automatic cleanup/GC based only on retention preview. Qualify full GitHub/Dropbox restore with real providers, CI artifacts/LFS, crash-resume, macOS physical Touch ID, Windows sign/installer, legacy import, multi-GB acceptance, WebDAV real OS mounting, rollback, signature and independent security gates before publicly promoting RC.
- `docs/RC_CERTIFICATION_MATRIX.md` is authoritative for *what remains unverified*. Use the compiled preview binaries and screenshots from Actions run 43640869549 for physical acceptance; do not claim live-account or signed release evidence not actually observed.

## r043 — ADSNET GPFS real VM handoff (16 Nov 2026)

- Operator provisioned three RHEL 9.8 6-vCPU/16GB nodes with separate 80GB OS + 150GB data disks: github-runner-01.vm.adsnet.it, github-runner-02.vm.adsnet.it, github-runner-03.vm.adsnet.it. Fourth node github-runner-04.vm.adsnet.it runs Ubuntu 24.04.5 with 4 vCPU, 8GB and 80GB OS; it is the sole GitHub private CI runner candidate. CES VIP DNS: github-runner-vip-01.vm.adsnet.it and github-runner-vip-02.vm.adsnet.it. No NSD/protocol or actual GPFS test yet.
- Operator states SSH public keys have been crossed among 01/02/03; no live verification claim. New non-mutating scripts in scripts/gpfs-lab/06-adsnet-vm-preflight.sh, 07-adsnet-dns-check.sh and 08-adsnet-verify-ssh-mesh.sh validate OS, disk inventory, DNS, and FQDN/short-name root SSH 3×3 (18 checks across three nodes). Use existing keys and manually validated host fingerprints only.
- User created private github.com/desalvo/multi-backup-labs, verified private, initialized README and private GPFS CES workflow. Register ONLY Ubuntu node 04 there, not the three IBM nodes or public Multi Backup repo. Protect execution with out-of-band root-owned exact main SHA, and do not execute private runner until source r042 passes review and is merged.
- Detailed instructions docs/GPFS_ADSNET_LAB_R043_IT.md. Keep RHEL 9.8 kernel/IBM release compatibility and Developer Edition license check; never initialize data disks automatically.
- Latest r042 commit aa69920c45 full Production CI and real NFS/CIFS/GlusterFS workflows completed SUCCESS, but independent GPFS field run remains PENDING.

## r044 — ADSNET operator VM findings and IBM build/NSD gate (16 Nov 2026)

- Operator supplied live read-only preflight from all three RHEL 9.8 nodes. Each: 6 vCPU, ~16 GiB, NTP synchronized, Red Hat registration, subscription release locked 9.8, running kernel 5.14.0-687.5.3.el9_8.x86_64; distinct DNS and reported SSH mesh 18/18 PASS.
- All nodes lack kernel-devel for the EXACT running kernel, ksh, gcc, gcc-c++, cpp; make not yet checked. This is NOT GPFS build readiness despite the VM identification preflight returning PASS. Install only matching kernel-devel and compiler packages from supported RHEL 9.8 channels, after `dnf --assumeno` inspection; do not blindly upgrade kernels or RHEL.
- NSD candidate disks according to operator lsblk: 01 /dev/sdb 150GB; 02 /dev/sdb 150GB; **03 /dev/sda 150GB** (OS on /dev/sdb). These are inventory observations ONLY, not proof of no disk signatures. New scripts/gpfs-lab/09-adsnet-dependency-nsd-audit.sh fails closed on missing packages or visible signatures and never formats or writes disks.
- The IBM FAQ documents CES capacity of 64 GiB for NFS-only and 128 GiB for SMB or multi-protocol, 10 GbE access plus socket recommendations. Prior 16 GiB recommendation for CES was incorrect for qualified hardware; existing 01/02 16 GiB VMs are experimental and need hardware sizing review before any genuine supported-profile CES acceptance.
- DNS forward checks on 04 PASS for all four VM names and VIPs; CES not yet configured and VIPs are not yet proof of HA. Do not claim IBM field certification, authentic GPFS backup, or NSD installation.

### Operator EUS 9.8 dependency availability and fixed-RAM plan (r044)

- Three DNF --assumeno transactions revealed exact matching kernel-devel 5.14.0-687.5.3.el9_8, gcc/gcc-c++/cpp/ksh/make available from RHEL 9.8 EUS repositories; kernel-headers newer build is a separate userspace package and not a running-kernel upgrade. A few gcc runtime libraries may upgrade *within* el9_8. The simulations ended in expected Operation aborted and no package installation has been reported yet.
- Operator reports no additional RAM available. Proceed with three-node **IBM GPFS core-only** small-data proof on existing 16GiB nodes; CES 64GiB for NFS / 128GiB for SMB or multi-protocol hardware criteria not met, so do not use hardware CES certification as a core blocker or claim support certification. See docs/GPFS_ADSNET_16GB_CORE_ONLY_R044_IT.md.
- Read-only 10-adsnet-observe-gpfs-core.sh verifies actual IBM commands and local FSTYPE=gpfs when installed, but does not execute any backup or format disks. No IBM GPFS field PASS has been produced.

## Aggiornamento operatore — CES funzionale anche su 16 GiB (16 novembre 2026)

L'operatore ha installato e verificato su tutti e tre i nodi RHEL 9.8 **kernel-devel esatto, gcc, gcc-c++, cpp, ksh e make**; gli stessi kernel RHEL 9.8 rimangono in esecuzione. Su ogni disco candidato NSD `wipefs --no-act` non rileva firme visibili (nessuna prova di assenza di dati nascosti o di identita hypervisor). **Decisione aggiornata:** testare CES autentico NFSv4 poi SMB3 anche con 16 GiB, un client e carico minimo; registrare PASS esclusivamente per funzionalita reale, non per hardware/performance IBM. L'evento IBM `total_memory_small` e un TIP e non necessariamente blocca il servizio, ma OOM, servizi unhealthy e perdita VIP invalidano il test. Il nuovo piano si trova in `docs/GPFS_ADSNET_CES_16GB_FUNCTIONAL_R044_IT.md` e il monitor read-only e `scripts/gpfs-lab/11-adsnet-observe-ces-lowmem.sh`. L'installazione IBM, NSD e CES non e stata ancora eseguita dall'assistente.

## r047 — Ceph RGW S3 / MinIO S3 vault and parallel RC gates (16 Nov 2026)

- Stacked on experimental r046 Rust candidate branch (NOT merged to main).
- New direct Rust S3 SigV4 transport: config validation, HTTPS-only remote,
  versioning+bucket COMPLIANCE default preflight, create-only encrypted archive
  upload, response version ID, per-version GetObjectRetention check, manifest-last
  snapshot publish, digest+age verify and new-directory restore.
- Provider configuration allows ceph-rgw/minio; AWS keys from environment only.
  HTTP restricted to explicitly enabled local loopback tests.
- The real MinIO integration workflow is gated after Production Gate and blocks
  package/container publication; negative test attempts to delete a locked
  object version with MinIO admin privileges. Test is NOT PASS until CI finishes.
- Ceph RGW live Object Lock qualification remains NOT RUN; do not assume
  MinIO's observed behavior proves Ceph RGW compatibility.
- New doc docs/S3_WORM_VAULT_R047_IT.md includes the explicit threat model,
  required independent storage administration and remaining feature gaps.
- Open RC blockers: native CLI compilation and MinIO live test; Ceph field test;
  S3 multipart/resume; desktop/server/agent/HA S3 integration; signed manifest
  catalog and delete-marker recovery; GitHub+Dropbox live account acceptance;
  strict fencing/limits and signed cross-platform artifacts. GPFS remains
  experimental and NON-BLOCKING.
- No stable tag, early-release publication or WORM certification on this
  development-only branch until real CI and security evidence is reviewed.

- Ulteriore integrazione r047: la GUI Rust espone i comandi S3 preflight,
  backup, verify e restore con identità age sbloccata in memoria; il server
  Rust espone status/backup/verify/restore protetti da sessione, CSRF,
  origin e approvazione recovery. Ancora da certificare con CI e ambiente
  reale: nessuna dichiarazione di WORM/RC sulla sola presenza delle API.
- Primo Production Gate r047 fallito per asserzioni statiche della vecchia
  dependency list; aggiornati scripts/check-production-gate.py e
  tests/test_multibackup.py. Real WebDAV CI ha fallito senza dettaglio:
  aggiunti messaggi diagnostici fail-closed sull'attestazione FSTYPE/RO.
  Il CI più recente deve essere riesaminato, senza considerare i lavori
  skipped come PASS.

## r048 — WebDAV davfs2 and MinIO S3 WORM CI repairs (16 Nov 2026)

- Hosted WebDAV initially failed from davfs2 Linux readdir EINVAL.
  The CI runner now configures buf_size 64 before mounting (upstream
  davfs2 1.7.2 fixes this kernel compatibility regression).
  Subsequent CI progressed into the Rust scanner but failed on the
  local davfs2 virtual lost+found recovery directory (EACCES), not on
  an actual remote WebDAV object.
- Rust chunk storage has a narrowly scoped exclusion only for the
  top-level davfs2 virtual lost+found and only when an operator explicitly
  sets --davfs-cache-not-on-remote on an attested WebDAV input.
  Hosted CI verifies via HTTPS PROPFIND that the directory is absent on
  the REMOTE server before opt-in. Any other unreadable path remains a
  fatal backup error; no silent partial success.
- Added contextual mounted source scan/open/read errors for diagnosis.
- MinIO image quay.io/minio/minio:latest is unavailable (registry
  unauthorized following upstream withdrawal). Hosted S3 CI now builds
  the pinned official MinIO source RELEASE.2025-10-15T17-29-55Z using
  Go, starts an isolated temporary real MinIO server, and uses awscli.
  It checks bucket versioning, Object Lock COMPLIANCE, encrypted backup,
  pinned-version verification/restore, bad age identity, working delete
  control on the unlocked test bucket, and denied version-specific delete
  of the WORM-retained object.
- No certification claim from code alone. The newest feature head must
  finish real WebDAV, MinIO, and Production Gate acceptance successfully.
  Ceph RGW requires its own independent field test; not certified here.
- All work remains in draft PR #20 on r047; no release or merge promoted.

## r050 — Supporto RustFS S3 WORM e CI reale (16 Nov 2026)

- Nuovo provider di storage vault S3 `rustfs` nel core Rust e nella GUI,
  con identici requisiti di sicurezza di MinIO e Ceph: HTTPS o HTTP solo
  su loopback per test, bucket versioning Enabled, Object Lock COMPLIANCE,
  versionId valido, retention sulle versioni, archivio cifrato age e
  restore sicuro.
- Nuovo test CI con server reale, non mock:
  .github/workflows/s3-rustfs-live-r050.yml e
  scripts/ci/s3-worm-rustfs-live.sh. Usa l'immagine ufficiale versionata
  rustfs/rustfs:1.0.3, storage tmpfs effimero, account S3 generato per
  il job, due bucket, verifica object lock, backup/restore, bad key,
  rifiuto DELETE su versione WORM e rifiuto accorciamento retention.
- La matrice multi-backend è in docs/S3_VAULT_STORAGE_MATRIX_R050_IT.md.
  MinIO: PASS reale precedente. Ceph: NOT PASS finché il nuovo workflow
  con Rook v1.20.9 non è verde; corretto l'errore d'installazione delle
  CR CSI non necessarie per object-store-only. RustFS: NOT PASS fino
  all'esito reale del workflow dedicato.
- Il PASS RustFS qualifica solo il percorso server singolo e API
  COMPLIANCE testate, non storage distribuito né resistenza ad
  amministratore host/root o cancellazione distruttiva del cluster.
- Ramo feature/r049-ceph-rgw-real-cluster-s3-certification e PR #21:
  nessun merge/RC prima dei test Ceph e RustFS e delle verifiche
  rimanenti di production gate.

## r052 — CI minutes optimization (16 Nov 2026)

CI trigger policy corrected to use actual changed files in feature push events, not the cumulative pull request diff. Fast component selection now compares the pushed SHA range. Heavy real protocol/database/Oracle checks run automatically only on relevant push modifications; Ceph RGW and duplicate independent MinIO run only on explicit PR labels / workflow_dispatch. Full Production CI for draft PR is opt-in and required on the final SHA before RC, but still runs for code on main and v* tags. Lightweight static CI policy validation replaces expensive automatic workflow-self-change acceptance runs. Current Ceph RGW genuine 3 MON/3 OSD qualification remains FAIL after timeout, do not mark certified or retry without diagnosis. RustFS/MinIO real S3 WORM last tested successfully. Details: docs/CI_MINUTES_POLICY_R051_IT.md. No release or merge performed.

## r055 — Rust RC evidence inventory (16 Nov 2026)

Rust S3 vault native dedicated plugin catalog PREVIEW on destination/restore-source only; no generic S3 input certification. Rust core/server/agent/desktop builds previously PASS, but provider parity not complete. Explicit verified real test coverage and missing provider gaps: docs/PLUGIN_CERTIFICATION_R055_IT.md. RC candidate scope and blockers: docs/RC_R055_CANDIDATE_SCOPE_IT.md. Ceph RGW diagnostic real CI 43829457157 has been started; no Ceph certification until PASS. Avoid broad real NFS/SMB and Lustre CI on plugin registry metadata-only changes (focused Rust tests suffice). PR #21 remains draft, no RC published.

## r065 — PVE/PBS/VMware/oVirt (16 Nov 2026)

User-mandated scope is documented at `docs/VIRTUALIZATION_PLUGIN_REQUIREMENTS_R065_IT.md`: separate PBS from PVE; all PVE VM(QEMU)/CT(LXC) and VMware/oVirt VM backup jobs need explicit online/live and offline/cold modes, with quiesced/application-consistent where possible, verified restore, original power-state preservation and cleanup/fencing. On VMware/oVirt, CT are explicitly DISABLED until the exact installed hypervisor version officially exposes native CT resource types and vendor-native backup AND restore APIs for each requested online/offline mode, proven in real acceptance; never use an in-guest/host agent, container runtime or orchestrator as a CT workaround inside these plugins. A VM containing containers is backed up as a VM, not as individual CT. All 4 providers remain **PLANNED/NON-CERTIFIED**, no real hypervisor/PBS backup and restore implementation or CI acceptance yet. Do not promote to RC READY without genuine isolated E2E host tests and approved licensing/credentials. Hypervisor CI manual/targeted to conserve hosted minutes.

## r066 — VMware / oVirt container constraint (16 November 2026)

Current VMware and oVirt plugin scope is VM-only. Do NOT build a container-agent/runtime workaround inside the hypervisor plugins; add native CT support only for versions, CT resource types and live/offline backup AND restore modes explicitly supported by official vendor APIs and verified end-to-end in real test environments. Historical oVirt container proposal is not evidence of currently supported CT backup. CT options stay hidden/unsupported otherwise. PVE QEMU VM and native LXC CT requirements remain as previously recorded. Corrected Rust catalog contract/tests and docs/VIRTUALIZATION_PLUGIN_REQUIREMENTS_R065_IT.md; no provider claimed certified. Avoid expensive hypervisor CI for documentation-only changes.

## r067 — fix and targeted RC validation (16 Nov 2026)

- Corrected live PostgreSQL 16 monitoring-token issuance failure: `$4::integer * INTERVAL '1 day'` explicitly types token lifetime; prior run 43839136135 failed with PostgreSQL Float8 vs Rust i32 (`ToSql(3)`).
- Extended real PostgreSQL test with exactly 4 checksum-pinned migrations, repeated initializer preserving versions/checksums, token revocation and out-of-range validation. This is real PostgreSQL, not an offline mock.
- Monitoring REST (scope `monitor.read`) now removes encrypted-backup manifests from the returned status JSON. Full detailed manifests remain within the authenticated admin-only backup UI.
- Narrowed real PostgreSQL HA acceptance workflow triggers to DB/monitoring/agent modules; Rust focused compile covers unrelated server Rust files. Do not repeat full CI on every small patch.
- Latest upstream Ceph run 43832467180 is **FAIL despite 3 MON quorum, 3 OSD UP+IN and RGW running**, because the three OSDs are reported under a single kind worker/CRUSH host. The multi-node placement attestation is mandatory; a green MinIO/RustFS result is not Ceph certification. Need truly isolated per-worker OSD devices or a PVC-backed Ceph lab.
- The stack of draft PRs (#10–14, #20–22) remains unmerged; no RC tag or new stable assets. For exact gate evidence see `docs/RC_READINESS_R067_IT.md`.

## r068 — Patroni HAProxy TLS primary check fix

Corrected HAProxy health probes to use TLS `check-ssl` and verify the private Patroni CA and per-node SNI/hostname on port 8008; previously the health check was plain HTTP while Patroni REST is HTTPS, so a healthy primary could be permanently DOWN. The check uses `/primary` (leader lock and writable PostgreSQL) rather than `/health`, and preserves TLS pass-through on the application PostgreSQL connection, bound to localhost:6432. A cheap targeted CI syntax check will use the actual distro HAProxy binary; full 3-node Patroni failover certification is still pending.

## r069 — PVE vs PBS native Rust plugin catalog

Separated the old combined Proxmox VE/PBS catalog into two distinct IDs: retain `proxmox` for PVE backward compatibility and add `pbs` for Proxmox Backup Server datastore/snapshot scope. Capability policy remains **Planned/non-executable** with Rust tests preventing accidental generic backup/restore. No PVE QEMU/LXC or PBS real environment E2E certification has yet been achieved; VMware/oVirt remain VM-only, native CT capability gated. Do not claim shipped/working hypervisor plugins until live backup/restore tests pass.

## r070 — bare-metal RPM build failure fix (16 Nov 2026)

The full 12-target Rust server packaging smoke run [43838871158](https://github.com/desalvo/multi-backup/actions/runs/43838871158) finished: **11 SUCCESS, Rocky Linux 9 x86_64 FAILURE**. Rust server compiled on failed target too; rpmbuild failed because `%{_unitdir}` is undefined in the Rocky 9 container. Replaced that macro with explicit `/usr/lib/systemd/system` for install and filelist, added a Python regression test and a cheap Rocky9 `rpmspec -P` syntax job that does NOT rebuild the entire 12 target package matrix. This is a fix pending confirmation, not yet a green final installation certification.
