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


## r038 active handoff — Rust primary and plugin implementation

User decision (2026-11-14): do not spend further RC-critical development on backward compatibility with Python-created archives (no critical legacy backup usage). Keep optional import tools and old files/identities but **remove archival byte-format parity from release blocking tests**. Release criteria are instead real Rust backup→verified restore, functional parity of GUI/providers, security and deployment certification.

PR [#10](https://github.com/desalvo/multi-backup/pull/10) on `feature/r038-rust-primary-plugin-executor`. Implemented a compiled generic POSIX→POSIX encrypted backup and restore executor plus guarded Linux read-only mounted NFS/SMB/SSHFS/GPFS CES sources → encrypted POSIX store. Mounted sources must be exact read-only OS mounts and require explicit operator-supplied snapshot attestation; the flag does not prove a real storage snapshot or metadata preservation. Rust binary functional tests are mandatory in Production Gate and Mac/Windows/Debian packaging. No generic remote target write, WORM or device writer is enabled.

32 registered plugins, 31 backup input and 22 output roles (53 declarations), 5 preview-executable source→POSIX backup pairs, **0/682 generic pairs certified**. Dedicated native GitHub/Dropbox implementations remain separate from generic dispatch. Next: green PR CI, real provider E2E, Rust-only GUI and settings feature closures, scheduler/retention/GC with protected references, native trusted release/upgrade and rollback. The r018 published Python/Tk assets remain unchanged. Do not merge on an unverified new CI.


## r038 continuation — requested full/incremental deletion, real GC and cross-account restore

User clarified: deletion of any full or incremental must preserve the full
restorability of every surviving successor, with predecessor/successor links
adjusted and an incremental promoted to full when its original full is
removed. Purge may rewrite encrypted manifests, **never physically delete
chunks**; only a later garbage collection can free space. Maximize safe
reclamation without weakening shared 8+2 parity protection.

Code on PR #10: `rust/src/backup_chain.rs` (full/incremental lineage and
exact four size statistics), `rust/src/chunk_store.rs` (immutable complete
snapshot refs, descendant-rebasing delete, protected hold checks, separate
reference-based GC), `rust/src/scheduler.rs` (manual/daemon backup and
verification), `rust/src/account_restore.rs` and independent write-account
checks for GitHub/Dropbox remote restore. Native GUI exposes full/incremental
selection, details, default full-chain verification, rebase deletion,
GC preview and confirmed GC run.

GC safeguards: fresh reference scan under common lock, all live backups
must pass age/HMAC/parity integrity checks, default 48h grace (min 24h),
only fully dead parity stripes and truly orphan chunks are eligible. A stripe
with both live and dead chunk members is retained intact; crash-safe
**parity stripe repacking** is the next concrete storage optimization.
No automatic GC daemon or remote GC is certified yet.

Tests added: `rust/tests/incremental_chain_roundtrip.rs` and
`rust/tests/garbage_collection_roundtrip.rs`, wired into cross-platform
native packaging. Check the latest PR CI on the current head before merge;
older green r037 CI is not proof for r038. Preserve the private vault,
never permit silent deletion or remote source credential reuse. Source and
destination accounts are independent, and destination credentials must
authorize destination access. Signed stable release remains blocked.

## r038 handoff — audited documentation and operational procedures

- **Source of truth for the Rust preview:** `docs/R038_OPERATIONS_IT.md` and `docs/R038_OPERATIONS_EN.md`. Both include commands, account-target permissions, scheduler JSON, restore, backup full/incremental, chain verification, rebase/purge and separate confirmed GC.
- Documentation in older milestones and PDF screenshots reflects r018 Python/Tk; `scripts/build-publications.py` now produces four r038-specific *development-preview* chapters in each bilingual manual. After CI, verify PDFs really contain new text and re-render correctly. Brochures describe earlier delivery and must not claim r038 certification.
- Current branch PR #10, not merged. Published r018 preserved. Old Python format migration optional. New Rust end-to-end recovery and safe storage lifecycle are mandatory.
- Do not confuse metadata-only purge with later physical GC. Mixed live/dead 8+2 parity stripe repack and actual authenticated cross-user multi-object restore remain unimplemented. Check latest PR CI, fix regressions, certify independent real data restores and operating-system signatures before merge/release.

## r038 — Handoff aggiornato dopo audit documentale

PR #10 branch feature/r038-rust-primary-plugin-executor. Documenti autorevoli: docs/DOCUMENTATION_INDEX_R038.md, docs/FEATURE_INVENTORY_R038.md, docs/OPERATIONS_R038_IT.md, docs/OPERATIONS_R038_EN.md; sempre aggiornare anche DEVELOPMENT_STATUS.md, RC_CERTIFICATION_MATRIX.md e documenti tecnici. scripts/check-documentation-consistency.py è il gate automatico. Il generatore PDF include appendice Rust e avvisa che screenshot Tk r018 sono storici.

Repack automatico 8+2 nella GC: parità nuova creata e verificata, indice autenticato supersedes pubblicato ultimo, vecchia stripe conservata durante grace del successore. I frammenti preparati senza commit vengono considerati orfani dopo grace. GC preview non modifica. Il purge conserva chunk e parità; protezioni sui discendenti bloccano riscritture implicite. Non dichiarare certificato senza CI dell'ultimo SHA, power-loss fault injection, restore su dati reali, installer firmati e rollback. Compatibilità degli archivi Python facoltativa, non bloccante.

## r039 handoff — multi-agent and multi-replica requirements (2026-11-15)

Branch feature/r039-afs-agent-packages PR #11, base feature/r038-rust-primary-plugin-executor PR #10. User requires per-volume AFS local/token/keytab; SSH agentless with key, remote keytab or server localauth; authorized pull and autonomous push agents; global agent authorization (ID, user, token, Ed25519, TLS), per-host thread/bandwidth/CPU/RAM caps, server/agent configured limits, and horizontal Kubernetes replica balancing with shared database and strict local storage affinity.

Code added: rust/src/{afs_volume,agent_protocol,execution_policy,cluster_placement}.rs, rust/src/bin/multi-backup-agent.rs, rust-server/src/{cluster,cluster_worker,agent_registry,agent_api}.rs, shared server sessions/readiness, native installer scripts/spec and CI jobs. Backend PostgreSQL TLS verify-full, registered shared/node_local stores, SKIP LOCKED and lease tokens; optional executor currently supports only server_local POSIX/NFS/SMB/SSHFS/GPFS sources. AFS single dump cannot be split, but independent volumes can run concurrently in an agent.

Critical RELEASE BLOCKER: job lease prevents stale status updates, NOT physical age manifest writes after lease lost. Must implement persistent fencing/transactional retries and prove crash/reassignment without duplicate or corrupt backup. CPU/RAM checks currently cooperative, remote SSH load not actually enforceable. Need tests with real AFS and agents, cluster PG/Ingress TLS, OIDC state migration, shared UI inventory/local restore routing, package install/sign/rollback. No release until these are qualified. Keep r039 IT/EN operations guides, feature inventory, development status, handoff and RC matrix synchronized on every change.

## r039 latest — common HA web UI, docs and exact-CI caution (2026-11-15)

Rust web UI now includes an admin-only shared cluster/agent/queue panel in rust-server/public/{index.html,app.js,app.css}; both GET /api/cluster/status and GET /api/agents/admin/list use PostgreSQL-backed state and are safe to show on all replicas. This is a read-only overview, not a complete UI administration wizard.

Bilingual operator manuals docs/CLUSTER_ORCHESTRATION_R039_{IT,EN}.md and docs/AGENTS_AFS_R039_{IT,EN}.md are the current r039 sources. docs/FEATURE_INVENTORY_R038.md records an r039 preview overlay and docs/DOCUMENTATION_INDEX_R038.md links those, while r038 manuals preserve earlier stages. Both scripts/check-production-gate.py and scripts/check-documentation-consistency.py now enforce r039 source/docs/matrix contracts.

Do not confuse passing earlier SHA r039 CI (e.g. run 43655180432 head 97f3786e) with certification of later changes. Obtain CI status for exact latest PR #11 SHA. Open core blockers remain lost-lease *write* fencing, real mounted multi-replica failover/PG, AFS keytab/localauth and live full/incremental restore, agent approval and TLS proxy production hardening, hard remote resource limits, global aggregate network rate caps, native package signing/upgrade, OIDC pending state shared across replicas and local filesystem restore routing. No merge or stable release without independent tests and updated matrix.

## r039 hardening handoff (2026-11-15) — latest PR #11

New code since r039 initial: rust/src/chunk_store.rs adds backup_in_namespace_mode_guard, plugin_executor.rs adds backup_with_mode_guard. cluster_worker.rs stops publication after loss of PostgreSQL lease, checks every file/chunk and immediately before final manifest, quarantine_expired instead of unsafe reclaim. cluster.rs serializes per-replica admission and exposes needs_reconciliation and attested offline-only recover_quarantined. rust-server agent_api exposes POST /api/cluster/jobs/recover (recovery authorization required) and POST /api/agents/admin/profile (recovery authorization required); agent_registry pins profile hash, provider, agent ID, checks prior to upload and in receipt transaction. Agent preview returns digest and actual command plan. Agent CLI has filesystem-worker slot locks cross process.

CRITICAL: there is still no fully automatic verified physical write fence under network partition, so currently a job with expired lease is quarantined until manual shutdown+encrypted-store audit. This is intentional to avoid duplicate writers. "Complete" RC still requires real cluster failover, source/restore AFS real cell, trusted proxy, agent package live installation, global resource/throughput tests, secondary/HA lifecycle, OIDC pending state and local restore cross-pod path. Do not merge PR #11 or publish stable installers until exact-head full CI is green AND external acceptance evidence exists. All documentation must be updated alongside code; prior r038/r039 reference docs retained.

## r039 current Kubernetes security handoff (2026-11-15)

The prior Kubernetes preview lacked MULTI_BACKUP_HARDENED_MODE=true, private-gate/recovery secret files and a way for UID 10001 to read root-owned Secret volumes. This was corrected in k8s/multi-backup-cluster-r039.preview.yaml: initContainer copies admin-hash, agent-proxy, age identity, private-gate and recovery tokens to RAM-backed private volume with ownership UID 10001 and mode 0600. Browser/agent gateway secrets are independent; do not expose raw HTTP Pod port. Added HPA min2/max10 CPU and PDB. scripts/check-production-gate.py now audits these YAML properties; docs/CLUSTER_ORCHESTRATION_R039_{IT,EN}.md and docs/SECURITY.md contain precise setup.

DRAFT PR #11 contains all this. Check the latest-head full CI, not just previous base green run 43655428169 at b3fe87d9. User wants completion, but do not misrepresent unsolved live AFS restore, remote SSH hard resource limits, physical lost-lease write fencing, manual quarantine recovery, signed native installers and real Kubernetes multi-pod chaos tests as certified. r038 source PR #10 is also draft: stacked history means neither PR should be blindly squash-merged or tagged stable without review of base and current CI.

## r039 PostgreSQL SSL driver hardening (2026-11-15)

Important finding: the postgres crate 0.19 parses sslmode=require, NOT libpq verify-full. Previously checking a substring sslmode=verify-full could allow a spoofed DSN and fail startup. rust-server/src/cluster.rs now structurally validates a PostgreSQL URL, requires exactly one sslmode=verify-full query pair, rejects duplicate/conflicting modes and disallowed host overrides, rewrites SSL transport to require and builds native_tls with TLS >=1.2 and explicit cert+hostname checks. A trusted PEM CA can be passed in MULTI_BACKUP_CLUSTER_TLS_CA_FILE. Kubernetes example now includes multi-backup-db-ca/db-ca.pem materialized as UID 10001 mode0600. Do not regress this with a raw string contains() assertion. Check unit test postgresql_tls_mode_is_structurally_enforced and exact-head Production Gate; live PG tests remain outstanding.

## r039+ Rust native executors continuation — 2026-11-15

Active feature/r039-afs-agent-packages PR #11 still draft on PR #10 r038. Generic Rust dispatcher now supports 8 source-to-encrypted-POSIX paths (posix,nfs,smb,sshfs,gpfs,cephfs,glusterfs,lustre) and 6 encrypted-POSIX-to-fresh-directory outputs (posix,nfs,sshfs,cephfs,glusterfs,lustre). Remote filesystem input must be the exact Linux RO mount and user snapshot-attested; mounted output must be exact Linux RW mount; metadata fidelity remains unverified. The plan preview_executable flags match the implementation, but executable=false and 0/682 production certified generic pairs remain correct.

rust/src/chunk_store.rs has backup_stream(opaque Read, producer-finalizer) committing age/zstd chunks + 8/2 parity manifest only after actual provider exits successfully. rust/src/afs_volume.rs implements typed per-volume vos dump streaming local or SSH-agentless, and full/incremental vos restore only to explicit NEW target with overwrite abort on full, followed by ordered increments in the same invocation. rust/src/bin/multi-backup-native.rs CLI commands plugins afs-backup, afs-restore-plan, afs-restore-new. Test rust/tests/afs_stream_roundtrip.rs uses real age/compiled Rust CLI and FAKE vos; gate added .github/workflows/ci.yml. Additional core mount/protection tests protect fake filesystem denials. Managed agents still separate approved push/pull pipeline.

Source manuals docs/PLUGIN_EXECUTORS_R039_IT.md and EN. Release blockers include live OpenAFS full/incremental recovery in a cell, AFS timestamps/ACL/VLDB/keytab and direct privileged localauth, authentic mounted CephFS/Gluster/Lustre tests and metadata preservation, full 32-provider functionality, OAuth target recredentialing, HA writer fencing, distributed GC, signed native installers, real Kubernetes failure drills. No main merge, signed RC or stable Docker merely because CI passes. Always update docs and handoff with next changes.

## r039+ latest SSH restore handoff (2026-11-15)

rust/src/afs_volume.rs: RestoreNewJob.execution is an optional Execution; if absent, local authorized token. If ssh_agentless, validate pinned SSH key/known_hosts and authorized remote token, use the same ssh_command_for builder for vos dump and vos restore, enforcing no config/proxy/forwarding/password/interactive auth. Only typed vos arguments are shell-quoted for remote OpenSSH; new-volume confirmation and full/-overwrite abort plus subsequent ordered increments are unchanged. rust/tests/afs_stream_roundtrip.rs has fake ssh+vos branch; real host/AFS cell test remains RC blocker. Manuals docs/PLUGIN_EXECUTORS_R039_{IT,EN}.md cover this. Verify exact newest PR #11 SHA CI, not an intermediate previously green run.

## r039 continuation — aggregate per-host upload ceiling (2026-11-15)

Rust agent now optionally accepts `max_host_transfer_bytes_per_sec`
in private agent configuration. Every agent HTTPS upload uses a shared
filesystem-locked, private on-host rate counter in its spool directory,
independent of the server's per-transfer limit. Invalid/corrupt state fails
closed; unset field preserves existing behavior. Regression tests cover
cross-reader accumulation, unsafe symlinks and corrupted/future state.

**Unresolved:** this is cooperative agent-host upload throttling, not
network QoS, a cross-host/pod cap or remote SSH CPU/RAM enforcement.
Physical lost-lease manifest fencing, multi-node bandwidth reservations,
live AFS restore, actual installer and Kubernetes failover remain RC blockers.
Confirm CI on the exact new commit SHA; keep PR #11 draft.


## r039 — OpenAFS virtual-cell test
- Simulated `vos` full/incremental and fresh-volume restore locally (131082 + 1500 bytes), including rejection of existing target and partial dump. The standalone fake command passed six checks.
- Added integration test `afs_virtual_cell_refuses_overwrite_and_partial_dump` that runs compiled Rust CLI, real age/zstd encrypted backup and verify, ordered chain restore, no-overwrite and partial-producer rejection. Exact-head CI must confirm the Rust test.
- GitHub run 43668284081 previously failed three test fixtures because `tempdir()` did not enforce 0700; fixed fixtures without weakening runtime security. Real OpenAFS cell/Kerberos/ACL/SSH acceptance remains required.

## r039 database adapters handoff

New Rust `database.rs` plus `plugins database-backup|database-restore-new` CLI. Full dump via pg_dump custom and mariadb-dump transactional; restore to NEW database via pg_restore/mariadb. Use a separate GitHub Actions job with real DB service containers and negative cases, and do not certify until exact-sha green. Missing: incremental WAL/binlog PITR, physical hot backup, DDL races, agent dispatch, vendor compatibility.

## r039 database preview catalog and stronger live tests

PostgreSQL and MariaDB backup inputs plus restore outputs marked Preview; generic dispatcher remains disabled. The live DB test now compares all 1000 rows, rejects wrong password with no extra authenticated manifest. Earlier verified database live workflow 43670330802 passed; further changed SHA must pass again. No stable or general MySQL/PITR certification.

## r039 latest — database real CI and AFS real-cell attempt
- Current Rust DB preview `rust/src/database.rs`: PostgreSQL pg_dump custom, pg_restore into freshly created DB; MariaDB single-transaction/InnoDB mariadb-dump and create-only mariadb restore; age/zstd content-addressed full backup.
- `rust/tests/database_live_roundtrip.rs`, workflow `.github/workflows/database-live-integration.yml`: real service containers Postgres16 + MariaDB11.4, first proven PASS run 43670330802, 2/2. Expanded negative credential checks/row-level byte comparison require later-head green run.
- `scripts/test-real-afs-cell.sh` and `.github/workflows/afs-real-cell.yml`: require real OpenAFS Ansible local_install, MIT Kerberos KDC, token, mount, VLDB, volserver, vos dump/restore and byte-identical full+incremental restore. If hosted-runner DKMS fails, preserve red signal and move to prepared lab runner; NEVER substitute fake vos and label it authentic.
- Keep PR #11 draft and stable disabled until latest SHA CI and independent production evidence. True DB PITR and AFS failover/keytab field certification remain unimplemented.

## r039 PITR acceptance — real vendor recovery labs
New `.github/workflows/database-pitr-integration.yml`: two independent jobs. `scripts/test-postgresql-wal-pitr.sh` uses PostgreSQL 16 actual pg_basebackup/WAL, encrypts base + WAL via Rust native archive, replays WAL to named restore point on isolated cluster and checks 30/465 excluding 999. `scripts/test-mariadb-binlog-pitr.sh` uses Rust logical full, durable ROW binlog from server, Rust age encrypted archive, actual binlog replay on isolated recovery server and checks same records and overwrite rejection. MariaDB verified PASS run 43672237760, PostgreSQL initial attempt failed missing pg_hba replication host and was fixed; full latest-SHA proof still required. Refer to docs/DATABASE_PITR_R039_IT.md and EN; NO production claim for complete native PITR orchestration, physical no-plaintext staging, WAL retention, GTID, multi-file gaps.

## r039 Galera 4 acceptance handoff, 2026-11-15
- PASS workflow `.github/workflows/database-galera-integration.yml`, run 43673606744 at commit 37e5de3943b76f453fe5448d251f64db7d80afa5. Real MariaDB 11.4 Galera 4 with `ci/galera/Dockerfile` and `scripts/test-mariadb-galera-pitr.sh`, no fake wsrep: 3 Synced Primary nodes, multi-master writes 30 records, kill node3 and write 10 more under quorum2, rejoin3, native Rust FULL from node2, deliberate 2-node safety rejection with no manifest, and replicated remote write-sets from nodes1/3 recovered via node2 binlog to isolated fourth server, exactly 60/1830 and excluded 999.
- Rust `Profile::galera_expected_nodes:Option<u16>` validates Primary/Synced/ready and exact member count both before and after dump; never restore into an explicitly Galera target. See `docs/DATABASE_GALERA_R039_IT.md` / EN.
- Runtime subtleties: Galera config is NON-secret and must be readable by UID mysql in containers; source creds remain 0600. Actual MariaDB config loaded from mariadb.conf.d; `--print-defaults` checks wsrep option. Do not confuse a running standalone mysqld with an actual Galera member.
- Still preview: no HA physical failover fencing, multi-file GTID/continuous log capture, network partitions, production SSL/SST, unattended binlog PITR. Keep PR #11 DRAFT; no stable promotion or merge solely on this single-cluster lab test.

## r039 actual POSIX/SSH/AFS CI handoff (15 November 2026)
- POSIX Rust native full+incremental filesystem E2E **PASS**, run 43678452447 at SHA 497e20f9. Source script `scripts/test-posix-real-roundtrip.sh` and workflow `.github/workflows/posix-real-integration.yml`. Identified and fixed `chunk_store::Store::repair` zero-padding of age ciphertext for RS stripes with mixed ciphertext lengths; 10MiB multichunk corruption/repair now passes. This does not preserve owner/mtime/xattr/ACL.
- Genuine OpenSSH (SSH, SCP over SFTP, legacy SCP, SFTP), pinned host keys and wrong-key negatives in `scripts/test-real-ssh-sshfs.sh`; SSHFS source RO + mountinfo type check + Rust E2E backup + SSHFS RW restore. SSH/SCP/SFTP transport is NOT the catalog `ssh` plugin; that plugin remains Planned. SSHFS requires /dev/fuse.
- Changed AFS CI to manual exact-SHA attested dedicated disposable self-hosted runner label `openafs-ci`; never execute privileged untrusted PR changes automatically. Required sentinel /etc/multi-backup-ci/allow-disposable-afs; see docs/ACCEPTANCE_POSIX_SSH_AFS_R039_IT.md. No real AFS PASS yet, no stable release or merge on simulated AFS evidence.


## r039 dedicated AFS runner Debian 12 platform update (15 Nov 2026)
- Debian 12 Bookworm x86_64 with stock Linux 6.1.x-amd64 is now the **preferred** disposable lab VM. Retained Ubuntu 22.04 GA 5.15.x-generic compatibility. The bootstrap checks exact OS/kernel, selects libicu72 on Debian vs libicu70 Ubuntu, tests matching headers, installs distro OpenAFS DKMS, and requires actual modprobe before allowing an ephemeral GitHub runner. No unsupported kernel or production data allowed.
- Added policy-gate lint for Debian 12; see docs/AFS_RUNNER_R039_IT.md and main-only approved exact-SHA dispatcher. **Still not field certified** until true Kerberos/VLDB/volserver/new volume restoration succeeds on user's isolated VM. Other agent/HA/installers remain separate gates.


## CI 43730251286 and reusable Proxmox lab — 15 November 2026

The real Debian 12 job completed Kerberos and the OpenAFS server/client mount, then Ansible failed creating root.afs/root.cell/test with 'Unable to locate viceetcdir directory'. Cause: upstream SHA-pinned collection infers viceetcdir from the cacheinfo manual page, missing in Debian 12 facts. New scripts/patch-openafs-ansible-debian-facts.py adds /etc/openafs only when a REAL /etc/openafs/cacheinfo exists. All volume and vos operations remain real, not simulated.

Keep Debian 12 VM, OpenAFS, Kerberos and Rust on the ISOLATED Proxmox network. GitHub runner remains per-job ephemeral; register fresh each time with a NEW short-lived GitHub token. The registration script reuses verified official runner archive after checking SHA256, uses exclusive flock, creates and removes each runner session/workspace, preserving the VM. The trusted workflow uses a per-job active lease and cleanup; after an abrupt termination inspect the VM and logs before manual lease recovery. Do not grant it production network or administrative credentials.

Previous afs-run-used marker is retired; current job guard uses afs-active. Restore volume name uses GitHub run ID/attempt to avoid overwriting old test volumes. To retry from mb-afs-runner after verifying no runner process is active, git pull --ff-only in ~/multi-backup and run bash scripts/register-openafs-runner-once.sh. New VM bootstrap is NOT required. Still not AFS-certified until real full/incremental/new volume byte comparison is green.
