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

## r071 — PostgreSQL DB backup fencing and long-running lease (16 Nov 2026)

Fixed the server-internal backup writer's fixed 30-minute lease: `renew_db_backup_lease` now extends only a still-valid UUID job+token lease on PostgreSQL; a dedicated 45s heartbeat keeps large `pg_dump`/age/S3-WORM jobs alive, and loss of the lease/PG quorum prevents a success record. The encrypted database dump and S3 object are checked against the current lease before and after remote write; a write committed during connectivity loss is an orphan requiring explicit reconciliation, not an automatic success. Genuine PG16 CI adds wrong-token, late renewal, completed-token replay and fail-closed expired-writer checks. This change does not certify simultaneous database+data snapshot consistency or 3-node Patroni failover. Full RC still requires signed packages, isolated disaster recovery, and final SHA gate.

## r072 — Ceph RGW CRUSH host evidence (16 Nov 2026)

Real CI 43832467180 showed three OSDs UP+IN, 3-MON quorum and RGW on distinct *virtual kind Kubernetes workers* but failed the mandatory 3-host CRUSH tree check. Added strict disposable-kind-only alignment: before a native CRUSH move, verify three real running pods, unique OSD IDs, their actual Kubernetes node assignments, exact unique pre-created loop block devices, active Ceph OSDs and MON quorum. Any mismatch fails closed; final independent 3-host attestation remains mandatory, no mocked PASS or physical failure-domain claim. Offline guard tests run in the cheap CI policy workflow. Full Ceph test remains explicit opt-in and Ceph is NOT certified until a real PASS.

## r073 — read-only native Rust PVE/PBS inventory (16 Nov 2026)

New `rust/src/virtualization_inventory.rs` supports explicit read-only PVE QEMU/LXC guest discovery and PBS datastore listing over verified HTTPS and scoped API token from owner-only (0600) JSON config. Redirects refused; response bounded to 1 MiB; token redacted from all output. CLI: `multi-backup-native plugins virtualization-inventory --config-json OWNER_ONLY.json --json`. The descriptor for both providers remains **PLANNED, NON-EXECUTABLE**: this API inventory is NOT a VM/CT backup, snapshot, restore, or live-account certification. Offline unit tests verify native guest types, fail-closed protocols and duplicate identities. No hypervisor field run without disposable infrastructure and explicit credentials.

## r074 — Rust virtualization CLI help and CI cost fix

Fixed the Rust CLI help-line escaping introduced by r073. Removed unrelated `rust/src/lib.rs` edits from the independently expensive RustFS WORM automatic trigger; dedicated S3 vault/archive/Cargo changes still trigger that live acceptance, and final full Production CI remains mandatory. Cheap policy regression test prevents the unrelated trigger from returning. No hypervisor plugin has been declared READY.

## r075 — cheap single-target Rocky 9 RPM install certification

Added `.github/workflows/rocky9-server-r075.yml` with trigger restricted to one explicit request file `.github/ci-requests/rocky9-server-r075.request`. Exactly one disposable Rocky Linux 9 x86_64 container builds the current Rust server using native compilers, constructs the RPM, installs it with **real DNF dependency resolution** (EPEL for age), validates service/user/0600 config, removes it and verifies a sentinel under the backup data directory survives. Not a full distro matrix, signed release, bare-metal boot or HA field test. This replaces the temptation to rerun all 12 platform builds merely to confirm the prior rpmbuild %_unitdir fix. A failed test must remain FAIL until diagnosed.

## r076 — one-shot genuine Ceph RGW WORM live acceptance

The PR label was accepted but did not create a Ceph workflow run on a non-default nested feature PR. A **single explicit one-shot request file** `.github/ci-requests/ceph-rgw-r076.request` now triggers exactly the isolated existing 30-minute Rook-Ceph 3-MON/3-OSD RGW workflow on the feature branch. It runs the new actual OSD-pod/device CRUSH alignment guard before the independent unchanged host attestation and actual S3 Object Lock COMPLIANCE/age backup, verify, restore and adversarial delete tests. The marker cannot launch future repeated tests unless deliberately edited. This is still three separate virtual kind worker nodes on one GitHub physical runner, so it cannot certify three independent physical fault domains. No PASS until actual successful job logs; do not execute other expensive matrices.

## r077 — SNMP standalone support without PostgreSQL schema mutations

SNMP polling now uses a validated **read-only PostgreSQL connection constructor** and never calls the startup auto-migration function. A standalone server without shared PostgreSQL produces its own safe local server metrics instead of failing the SNMP extend command. Shared-cluster SNMP remains a read-only inspection of the running schema. Unit tests ensure the standalone output has one local node, no primary database and no manifest secrets. This is not a substitute for real Net-SNMP authPriv integration and external network-isolation acceptance.

## r078 — native Rust release without Python rebundling (16 Nov 2026)

The previous main `ci.yml → build-native.yml` publication path still rebuilt Python/PyInstaller GUI/CLI installers despite compiling true native Rust preview packages. **Redirected `native-release` to `.github/workflows/build-rust-release.yml`**, a reusable same-run artifact collector: it does not rebuild any GUI/CLI/agent, requires the complete Rust-only Mac/Windows/RPM/DEB/agent job matrix and Production Gate/real MinIO/Kubernetes passes, rejects missing/duplicate/legacy assets, regenerates checksums and a provenance release manifest, then updates ONLY a rolling *early-release* from main. On stable tags it fails closed pending Developer ID notarization, Windows signature and GPG. The old PyInstaller workflow remains in the repository as a legacy reference but is not called by production release. Legacy Mac Python smoke is now opt-in on workflow_dispatch, reducing 2 expensive Mac jobs on ordinary full CI. Legacy Python-screen capture/manual publication remains an explicit separate gate, not included as misleading Rust user documentation in published native assets. Static Production Gate tests updated to enforce the correct real Rust publication path; new collector unit tests added. **No release published/merged from this draft branch; real final full CI on final SHA still required.**

## r079 — Ceph real pod node/device mismatch and fast RC policy tests (16 Nov 2026)

Ceph live run [43846329965](https://github.com/desalvo/multi-backup/actions/runs/43846329965) is FAIL: 3 MON quorum, 3 real OSD UP+IN and RGW, but two Rook OSD pods were scheduled on the same virtual kind worker, with their ROOK_BLOCK_PATH mismatching the original worker/device allocation. The previous CRUSH-only alignment correctly REFUSED as unsafe. New `scripts/ci/ceph-osd-node-align.py` checks exact 3 genuine UP OSD IDs, pod names/status, unique real loop paths and intended workers before issuing a narrowly scoped `kubectl patch deployment rook-ceph-osd-N` nodeSelector on disposable kind, then retests and applies previously guarded CRUSH repair. No mock cluster, no lowering 3 MON/3 OSD/3 CRUSH hosts, no production cluster modifications. Cheap regression tests added. Also added Rust release collector offline tests and `scripts/check-production-gate.py` to the cheap CI policy job so future release wiring mistakes do not waste a full CI. **No Ceph PASS** until a fresh real S3 WORM run passes with correct pod/CRUSH placement.

## r081 — private-only PostgreSQL/Patroni/etcd network exposure gate

Added cheap, fail-closed static tests for Kubernetes Service/Pod exposure, HAProxy's localhost-only 6432 primary routing, Patroni's private VLAN 5432/8008 listeners, TLS-authenticated private etcd endpoints and the server's strict TLS-to-proxy DSN. This protects against accidental publication of DB ports 5432, etcd 2379/2380 or Patroni 8008 by changed example manifests. **It is a static release policy, NOT proof of actual firewalld/nftables, cloud network ACL isolation, host scanning or Patroni HA failover**: these need a disposable real deployment before HA RC.

## r071-r082 — evidenza RC verificata, 16 novembre 2026

- PostgreSQL: r071 aggiunge heartbeat lease 45s e fencing anti-replay per backup DB nel WORM; CI PostgreSQL reale https://github.com/desalvo/multi-backup/actions/runs/43843182470 e Rust https://github.com/desalvo/multi-backup/actions/runs/43843182685 sono PASS. SNMP standalone read-only evita le migrazioni dello schema al polling; PG https://github.com/desalvo/multi-backup/actions/runs/43846716012 e Rust https://github.com/desalvo/multi-backup/actions/runs/43846715919 PASS nei job eseguiti.
- Bare metal: RPM nativo Rust installato, testato con vere dipendenze DNF e disinstallato su Rocky Linux 9 x86_64, preservando dati: PASS https://github.com/desalvo/multi-backup/actions/runs/43846119136. Matrice 12 OS/CPU, aggiornamenti e certificazioni firma non ancora finali.
- RustFS: real server 1.0.3 con backup/verify/restore age e Object Lock COMPLIANCE PASS https://github.com/desalvo/multi-backup/actions/runs/43844964347. MinIO PASS precedente. Ceph RGW https://github.com/desalvo/multi-backup/actions/runs/43846329965 FAIL nonostante MON, OSD e RGW reali: pod OSD non collocati sulle rispettive allocazioni disco-worker; correzione guardata r079 e singolo collaudo reale https://github.com/desalvo/multi-backup/actions/runs/43848726571 avviato. NON dichiarare Ceph certificato prima di esito positivo.
- PVE/PBS: Rust API read-only per inventario QEMU/LXC e datastore, con HTTPS e token scoped, test focused https://github.com/desalvo/multi-backup/actions/runs/43845460116 PASS. Nessun backup live/offline/restauro PVE/PBS pronto. VMware/oVirt VM-only Planned, CT solo su API native vendor documentate e validate per versione.
- Distribuzione Rust: il workflow release ora raccoglie senza ricompilazione PyInstaller i veri artefatti GUI/CLI/agent prodotti dalla stessa Production CI, con matrice obbligatoria, checksum e manifest; stable resta bloccata fino alle firme. Workflow legacy mantenuto soltanto come riferimento e non chiamato dal nuovo publisher. Non ancora eseguita full CI di release sul SHA finale.
- Sicurezza: gate statici anti-esposizione PostgreSQL 5432, Patroni 8008, etcd 2379/2380 e HAProxy loopback 6432, TLS verificato, guard OSD. CI leggera https://github.com/desalvo/multi-backup/actions/runs/43849085464 PASS. Non sostituisce scansioni reali dall'esterno né failover Patroni.
- RC: ancora necessari DR combinato database+dati su cluster vuoto, real Patroni 3-nodi/split-brain/failback, test sicurezza monitoring REST/SNMPv3 e rete esterna, installer/update/rollback e firme, ultima full Production CI sullo SHA finale, merge ordinato della catena PR. GitHub/Dropbox test-account e hypervisor non qualificati restano fuori da RC limitata.

One-line English commit summary: Harden PostgreSQL WORM leases, certify Rocky 9 Rust RPM installation, replace Python release repackaging with verified Rust artifacts, gate private database networking, and extend guarded Ceph/PVE-PBS acceptance.


## r083 — WORM-pinned isolated disaster recovery script hardening (16 Nov 2026)

- Fixed offline `scripts/db-disaster-restore.sh`: pinned S3 manifest versions must be used for **both verify and restore**. The DB variable `MULTI_BACKUP_DR_DB_MANIFEST_VERSION` now reaches restore; the paired data archive supports `MULTI_BACKUP_DR_DATA_MANIFEST_VERSION` on verify and restore. This permits access to retained versions hidden by S3 delete markers when the operator supplies the recorded version IDs.
- Fail closed if the stage root is not operator-owned or is not empty, and reject a data manifest pin without a paired data snapshot. Existing guards still require isolated NEW EMPTY PostgreSQL primary, explicit opt-in, local service stopped, private 0700 staging, and no overwrite.
- Seven isolated shell drill tests verify pinned flags, unpinned mode, empty-root, standby, existing user objects, missing confirmation, and unmatched data receipt; all 7 PASS locally. The lightweight CI policy workflow runs `bash -n` plus the drill on relevant changes.
- **Not a live DR qualification**: these tests use mocked commands. Combined DB+data consistency and genuine pg_dump→age→S3 WORM→pg_restore in an isolated PostgreSQL cluster remain a blocking RC acceptance. Patroni HA / split brain / release signing / final Production Gate also remain outstanding.

One-line English commit summary: Preserve pinned WORM manifest versions across database and data disaster restores, refuse unsafe staging, and add seven isolated RC regression drills.


## r084 — disposable real DB+data disaster-restore acceptance lab (16 Nov 2026)

- Added `scripts/ci/db-disaster-rustfs-live.sh` to the existing **real RustFS S3 WORM** CI suite (reuses its actual disposable RustFS 1.0.3 bucket/container to avoid a second expensive provider spin-up). The test provisions a separate ephemeral PostgreSQL 16 instance and a source/empty target database; streams a real `pg_dump -Fc` into age, commits both the encrypted PostgreSQL dump and associated data archive through the native Rust S3 WORM adapter.
- The test creates unversioned S3 delete markers over **both** retained manifests, invokes the hardened offline DR script with independent pinned `manifest_version` receipts, compares restored SQL rows and files byte-for-byte, and checks refusal of a second restore to a now-populated target. Host-only credentials, loopback-only ports, container cleanup, ephemeral keys and fresh data.
- **Qualification is pending** until the real GitHub Actions job completes successfully on this SHA. A syntactically correct script does NOT count as a successful PostgreSQL/RustFS test. Even PASS will certify only this ephemeral RustFS+PG16 lab, not synchronized production DB+data snapshots, Patroni failover, Ceph RGW, physical failure domains, release signing or cross-OS installers.

One-line English commit summary: Add a real ephemeral PostgreSQL-to-RustFS WORM disaster-restore drill with pinned DB/data receipts and fail-closed repeated-restore checks.


## r085 — real PG16 + RustFS WORM disaster-recovery evidence (16 Nov 2026)

- **PASS on exact functional SHA `aca51a4ed6d126b3e2af7e1c94a8fbfea412995e`**: GitHub Actions https://github.com/desalvo/multi-backup/actions/runs/43866257791 ran real RustFS 1.0.3 S3 Object Lock COMPLIANCE plus ephemeral real PostgreSQL 16. The logs confirm `DB logical restore completed into new empty cluster`, `PASS real isolated PG16 -> age -> RustFS WORM -> pinned DB+data restore`, and final RustFS negative-delete/retention suite PASS. The current lightweight static CI https://github.com/desalvo/multi-backup/actions/runs/43866257960 is also **PASS**.
- Scope of certification: encrypted PostgreSQL logical dump, separate data backup, S3 WORM retained-version recovery after paired manifest delete markers, byte-exact data/row verification and fail-closed repeat restore, all on **one disposable GitHub-hosted runner**, not a live production environment.
- Does NOT certify coordinated transactional point-in-time snapshots of a production DB with an independently running filesystem data backup, server application resume, three-node Patroni/etcd, recovery on distinct failure-domain hosts, Ceph RGW, OS installer signatures or native update/rollback.
- Remaining narrow RC release gates: final SHA full Production Gate, release installation/update/rollback/firma on supported native target platforms, HTTPS monitor token revocation/SNMPv3 and network isolation acceptance, review of PR stacking/mergeability and a documented limited-provider scope. HA RC additionally requires real 3-node primary kill, DCS partition, fencing, synchronous standby promotion, no split brain, node rejoin and client failover.
- Ceph 3-node kind RGW trial https://github.com/desalvo/multi-backup/actions/runs/43848726571 remains **FAIL** despite running MON/OSD/RGW; 89 PG remained remapped and a 320s readiness deadline expired after CRUSH reconciliation. Ceph remains outside limited RC, GPFS experimental and non-blocking; PVE/PBS backup engines, VMware/oVirt, GitHub/Dropbox authenticated field integrations are not certified.

One-line English commit summary: Record real PostgreSQL and RustFS WORM disaster-recovery PASS evidence while keeping HA, Ceph and signed RC publication gated.


## r086–r112 — Ceph RGW and real HA acceptance engineering (16 Nov 2026)

- **Ceph RGW**: r086–r088 repair the Rook OSD pod's *actual* `ROOK_CRUSHMAP_HOSTNAME` as well as its Kubernetes nodeSelector and require three distinct CRUSH hosts, real OSD IDs/devices, 3 MON quorum, 3 OSD UP/IN and all PGs `active+clean` without remapping. The real Rook/Ceph 20.2.4 lab [43868812396](https://github.com/desalvo/multi-backup/actions/runs/43868812396) confirmed three distributed OSDs/89 clean PGs, but the first guard incorrectly read `num_remapped_pgs` from `pgmap` rather than `osdmap`: false-negative. r097 repaired this with negative tests. [43870989879](https://github.com/desalvo/multi-backup/actions/runs/43870989879) again proved 3 MON, 3 real OSD, 89 clean PG, RGW live but was blocked by another conservative false-negative: Ceph 20.2.4 status contains `monmap.num_mons=3` without optional `monmap.mons`. r107 repaired the strict MON count/quorum parser and added compact-format regression tests; cheap CI [43873254293](https://github.com/desalvo/multi-backup/actions/runs/43873254293) PASS. One-shot real Ceph [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) checks genuine RGW Object Lock COMPLIANCE, denied retained-version deletion/shortening and age encrypted pinned manifest rescue after delete marker. **Do not certify Ceph WORM without the final end-to-end job PASS.** Three kind workers share one physical runner, not independent physical failure domains.
- **HAProxy**: r089–r090 add actual HAProxy/TLS three-backend test verifying `GET /primary`, valid server CA/SNI, automatic routing change, no stale write fallback and invalid-SAN refusal. [43869197926](https://github.com/desalvo/multi-backup/actions/runs/43869197926) PASS, but responders intentionally simulate Patroni.
- **Patroni PostgreSQL16 / etcd**: r091–r112 introduce a real disposable three-process PG16/Patroni + three-member mutually authenticated TLS etcd test. Once the loopback certificate included its actual 127.0.0.1 source IP SAN (without disabling verification), real etcd quorum and three genuine PostgreSQL instances with streaming replicas were achieved [43871725242](https://github.com/desalvo/multi-backup/actions/runs/43871725242). The HA release gate remains FAIL: a leader crash happened before DCS publication of a sync candidate or the test incorrectly interpreted PostgreSQL/Patroni readiness. r103–r111 separate elected write primary from DCS-confirmed sync election and accept real `streaming` state, with independent `GET /sync` check; one-shot [43874152163](https://github.com/desalvo/multi-backup/actions/runs/43874152163) still requires final examination. Do not claim failover, rejoin or split-brain fencing certified until the actual code path passes.
- r105/r106 add cheap fail-closed CI regression guards for HTTPS/mTLS/private PostgreSQL/etcd, synchronous strict, one-shot targeted real HA tests, no synthetic PASS, and Python syntax. Lightweight runs [43872963782](https://github.com/desalvo/multi-backup/actions/runs/43872963782) and [43873629288](https://github.com/desalvo/multi-backup/actions/runs/43873629288) PASS.
- Earlier isolated PG16+RustFS WORM database/data disaster restore [43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) remains PASS within its real single-runner profile. Final candidate SHA full Production Gate, native installers/signatures/notarization, monitoring REST HTTPS/SNMPv3, private network field acceptance, HA three-independent-host validation and PR-stack reconciliation **still block an HA RC**. GPFS experimental/nonblocking. PVE/PBS backup engines and VMware/oVirt VM integrations remain planned/unqualified; inventory-only is not backup.

One-line English commit summary: Repair real Ceph CRUSH and compact status attestation, add Patroni HTTPS primary routing and genuine mTLS etcd/PG HA fault-injection gates, and preserve fail-closed RC qualification.


## r115 — GENUINE Ceph RGW S3 WORM multi-node virtual acceptance PASS (16 Nov 2026)

- **CERTIFIED for the EXACT tested Ceph/Rook profile:** [real GitHub Actions #43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605), tested SHA `05c128a57f4050c927d975bf59b390005d0471b0`, **SUCCESS**, including actual Rook 1.20.9 / Ceph 20.2.4, three MON in quorum, 3 real UP/IN OSDs on three distinct virtual kind worker/CRUSH host nodes and 89 placement groups all `active+clean` (zero remapped).
- The very same real S3 RGW account confirms delete of unprotected control object, Ceph S3 bucket versioning and Object Lock COMPLIANCE preflight; Rust CLI age-encrypted backup, authenticated verify and byte-identical restore; denies deletion of retained object *version* and shortening of retention, verifies retention unchanged, rejects wrong age identity and overwrite, creates malicious unversioned manifest delete marker, verifies unpinned lookup fails and pinned manifest version restores ciphertext byte-for-byte. Logs include `CEPH REAL CLUSTER: PASS`, `PASS: real Ceph RGW S3 COMPLIANCE retention, denied deletion, Rust age restore and pinned recovery after delete marker`, and `PASS: real multi-node Ceph RGW S3 native Rust WORM acceptance`.
- **Scope limitation:** 3 virtual kind workers on one physical hosted runner, ephemeral loop-backed OSD devices, not 3 physically independent hosts, server/admin-root compromise certification, sustained multi-TB resilience or Ceph storage fault-domain disaster recovery. This is genuine Ceph RGW S3 interoperability/WORM functional evidence; it must not be described as physical HA certification.
- Remaining RC/HA release blockers are real Patroni failure+rejoin and quorum fencing, full Production Gate on final SHA, signed/notarized native installers, live HTTP REST+SNMPv3/network security acceptance and PR mergeability. GPFS experimental; hypervisor backup/restore and live Dropbox/GitHub accounts unqualified.

One-line English commit summary: Certify genuine three-worker Ceph RGW Object Lock WORM backup, deletion denial and pinned age restore with real CI PASS.


## r116–r121 — Patroni sync-mode fault injection and real SNMPv3 transport acceptance (16 Nov 2026)

- Real Net-SNMP v3 SHA/AES **authPriv** on `127.0.0.1:11611` with owner-only ephemeral credential files, correct scoped read-only view and extend output: actual hosted CI [43875372585](https://github.com/desalvo/multi-backup/actions/runs/43875372585) **PASS**. Actual wrong auth-key rejected; v2c `public` rejected with v3-only configuration; process bound only on loopback. The metrics producer in this isolated protocol test is a synthetic echo replacement, so it does **not** qualify full Rust-native metrics delivery or external SNMPv3 network deployments.
- Patroni real 3-etcd mTLS and 3-PG16 HA [43874152163](https://github.com/desalvo/multi-backup/actions/runs/43874152163) **FAIL** as a complete gate, but the log confirms a pre-crash Patroni DCS-approved synchronous standby and automatic new-primary promotion after the former leader crash. The strict-mode CI originally demanded a new synchronous standby immediately before allowing former-primary rejoin. r114 corrected the acceptance order: verify durable acknowledged row on newly elected primary, reject unsafe writes when no synchronous standby, rejoin old leader and wait for DCS-qualified sync, then test protected writes and DCS majority-loss fencing.
- Subsequent complete real test [43874914217](https://github.com/desalvo/multi-backup/actions/runs/43874914217) **FAIL** during startup due inconsistent role/state representation of the DCS sync standby. r119 now consults actual Patroni `GET /sync` 200 as the authoritative DCS role check, retains PostgreSQL sync-state confirmation before crashing primary, waits for safe initial election and records redacted /cluster/replication diagnostics on a failure. One-shot real run [r120 one-shot run pending confirmation](https://github.com/desalvo/multi-backup/actions/workflows/patroni-etcd-real-ha-r093.yml) is NOT CERTIFIED in this status document until its exact run ID and outcome are checked. Never equate HAProxy mock-responder acceptance with full Patroni/etcd distributed failover.
- Ceph real Rook-Ceph RGW WORM [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) remains PASS on exact SHA and scope in r115; full signed RC and physical-host HA acceptance remain separate.

One-line English commit summary: Qualify real Ceph RGW WORM and SNMPv3 authPriv, and harden genuine Patroni synchronous failover and quorum-fencing acceptance without relaxing release gates.


## r127 — Production Gate Python matrix PyYAML bootstrap repair

- The exact [full Production CI failure 43881879293](https://github.com/desalvo/multi-backup/actions/runs/43881879293) on the security fix PR is a reproducible **CI environment error**: Python 3.11 test discovery loads `tests/test_db_network_isolation_r081.py` which imports `scripts/ci/check-private-db-exposure.py`, requiring `yaml`, but PyYAML was installed only *later* in the security-audit setup. The 158 discovered tests stopped with `ModuleNotFoundError: No module named 'yaml'`.
- Install `PyYAML>=6,<7` in the **three existing Python runtime setup steps** (3.11, 3.12, 3.13) before their unit suite, avoiding CI-only dependency additions to the shipped application. Added an independent cheap workflow-regression test that checks all three environment setups.
- Full Production Gate remains **NOT CERTIFIED** until a successful exact-candidate-SHA run; this repair only removes the evidenced environment blocker. The previously verified PostgreSQL/MinIO/RustFS/Ceph test results and HA failures retain their own scope. GPFS remains suspended/outside the RC.

One-line English commit summary: Install CI-only PyYAML before all Python test suites and prevent production-gate dependency regression.


## r128 — eliminate HA strict-mode race by genuine two-standby kill (16 November 2026)

- The latest real [Patroni/etcd job #43876968124](https://github.com/desalvo/multi-backup/actions/runs/43876968124) reached automatic promotion and recovered the pre-crash acknowledged row, but incorrectly inferred an unsafe new-primary write solely from a missing DCS `/sync` response immediately before a SQL commit. Its own diagnostics show PostgreSQL's new synchronous sender was already connected by the error report (`mb-node1|sync|streaming`), a **time-of-check/time-of-use ambiguity**.
- The r128 acceptance drill now explicitly terminates the **remaining second standby** and its PostgreSQL process in addition to the old primary, independently confirms the new primary has **zero streaming synchronous senders** and no DCS `/sync` endpoint, then submits a timed write while BOTH peers remain down. Any successful commit under this controlled absence remains a blocking real HA FAIL, never silently reclassified as success.
- The old primary must rejoin and become a DCS-qualified synchronous standby, an acknowledged write must survive, then the other standby must rejoin before an independent DCS-quorum-loss write-fencing drill. No mock. Existing real mTLS etcd and private PG guards remain mandatory; one-shot CI request r128 is bounded to the disposable GitHub-hosted runner.
- Scope: three real PG/Patroni processes and three actual TLS etcd members on one runner, not physical three-host failure domains. No RC or HA certification unless this entire exact-SHA real job passes.

One-line English commit summary: Prove strict Patroni fencing with both genuine standbys stopped and require both nodes to rejoin before quorum fault injection.


## r130 — HA strict-sync client timeout vs real write fence

- The exact [real HA r128 run 43882693772](https://github.com/desalvo/multi-backup/actions/runs/43882693772) proved a new PostgreSQL primary held `synchronous_standby_names='*'` and `synchronous_commit='remote_apply'` after **both physical standby PostgreSQL processes were stopped**; the timed INSERT never ACKed and `subprocess.run` raised `TimeoutExpired` after nine seconds. Thus the prior complete CI job is **FAIL because the test harness did not handle a blocked client**, not because a committed write bypassed strict replication.
- The negative probe now configures statement timeout before SQL via `PGOPTIONS`; a PostgreSQL cancellation is accepted only when it specifically reports statement timeout/cancellation. If the backend continues waiting until the client deadline, only this deliberate subprocess timeout is accepted. In both cases the test independently proves the elected primary still answers read-only SQL and no DCS synchronous replica returned. A successful write or unrelated TLS/auth/permission failure remains an immediate HA failure.
- The test continues after this negative fence and must verify old-leader rejoin, the former second standby's rejoin, acknowledged post-rejoin writes, and failure of stale writes after etcd majority loss. No change to production Patroni strict settings or emergency fail-open override; run one focused bounded real lab r130. GPFS unchanged suspended/out of RC.

One-line English commit summary: Treat validated strict PostgreSQL commit blocking as expected and complete rejoin plus DCS-majority fault injection.


## r134 — integrate reviewed native Proxmox PVE/PBS Rust workstreams into RC development branch

- Consolidates the three executable Rust files that passed [r125 real Rust CI 43881321237](https://github.com/desalvo/multi-backup/actions/runs/43881321237) and [r129 real Rust CI 43883196245](https://github.com/desalvo/multi-backup/actions/runs/43883196245) from stacked draft PRs #24/#25 into active `feature/r060-baremetal-server-cluster-vault`. The two provider paths are independent and neither allows deleting existing archives.
- **PVE preview:** explicitly confirmed one-guest native QEMU VM/LXC CT `vzdump` task trigger (snapshot/suspend/stop), exact HTTPS inventory+node+kind+VMID match, `remove=0`, scoped token, protected UPID and read-only task status; task success is **not** a Multi Backup archive verification.
- **PBS preview:** scoped TLS read-only datastore snapshot inventory `vm`, `ct`, `host`, strict namespace/identity validation and provenance: PBS verification OK is NOT independently verified encrypted backup or restore.
- Field work still required: PVE live/offline backup archive ingestion, PBS export verification, create-only restores of VM/CT, test account and real hypervisor certification. VMware/oVirt VM adapters remain planned; CT disabled without documented native support. GPFS suspended/excluded from RC.
- Production Gate on the final release candidate is mandatory; the integration is a development branch commit, not a stable tag or a released installer.

One-line English commit summary: Integrate passed native PVE vzdump and PBS snapshot inventory Rust adapters into the RC development line.


## r138 — verified native HTTPS metrics and TLS cryptoprovider in main RC line

- Integrate previously reviewed secure native Rust monitor+TLS runtime fixes from the r131–r135 branch. Focused Rust [43885772022](https://github.com/desalvo/multi-backup/actions/runs/43885772022) **PASS**; real PostgreSQL [43885772144](https://github.com/desalvo/multi-backup/actions/runs/43885772144) **PASS**; native server HTTPS Prometheus with X.509 CA verification, correct/wrong/missing bearer and plaintext-denial [43885771957](https://github.com/desalvo/multi-backup/actions/runs/43885771957) **PASS**. The test caught and led to a fix for a real rustls multi-provider startup panic.
- Remote/private VLAN Prometheus now requires TLS **and** bearer; HTTP only loopback. Both REST and Prometheus use owner-only (0600), regular, non-symlinked private TLS keys. One-shot CI request r138 revalidates the integrated development branch SHA (not a stable release).
- Outstanding: genuine 3-independent-host Patroni split-brain/network partitions, PostgreSQL-backed live REST monitor.read HTTPS, PVE/PBS actual backup-media/restore qualification, VMware/oVirt real backup/restore, signed installers and full final Production Gate. GPFS suspended and excluded.

One-line English commit summary: Consolidate real native Rust HTTPS monitoring, TLS key hardening and cryptoprovider startup fix.


## r140 — real SNMPv3 polling now reads the native Rust server's monitoring output

- The earlier real [SNMPv3 acceptance 43875372585](https://github.com/desalvo/multi-backup/actions/runs/43875372585) already verified real Net-SNMP authPriv SHA/AES, wrong auth key rejection, loopback-only bind and v2c denial, but replaced the actual Rust metrics producer with a synthetic `/bin/echo` fixture.
- The r140 CI compiles the **real Rust `multi-backup-server`** and configures Net-SNMP `extend` to execute its native `monitor-snmp-status`; assertions now require real standalone counters `multi_backup_cluster_nodes_total=1`, `multi_backup_database_primary=0`, plus all previous negative network/auth checks.
- A PASS proves end-to-end local SNMPv3 → real Rust metrics producer, not cluster-backed remote polling or production host network separation. No fake readings. The change is a targeted CI acceptance gate; release remains blocked until exact final Production Gate and remaining field tests.

One-line English commit summary: Verify SNMPv3 authPriv against the genuine Rust monitoring producer instead of a synthetic echo.


## r141 — tested VMware/oVirt native read-only VM inventory merged into active RC development

- Integrated the Rust native `hypervisor-inventory` CLI and module from [r137/PR #27](https://github.com/desalvo/multi-backup/pull/27); source [Rust CI #43886521933](https://github.com/desalvo/multi-backup/actions/runs/43886521933) **PASS**, including native core, desktop and server component tests. vCenter `GET /api/vcenter/vm` with session header and oVirt Engine REST v4 `GET /ovirt-engine/api/vms` with Bearer use TLS, 0600 owner-only config, bounded responses, no redirects or mutation. **VM-only inventory**; CT remains disabled and provider-specific live/offline backup+restore needs real field tests. Remote list may be paginated/incomplete; output explicitly says `inventory_complete=false`.
- Consolidated PVE QEMU/LXC native `vzdump` task start/status and PBS vm/ct/host snapshot read-only inventory in r134; both previously passed real Rust unit CI and are in active branch. These are not certified encrypted archives or create-only restores.
- Independent confirmed tests: Ceph RGW Rook 3 virtual workers full S3 COMPLIANCE WORM [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) PASS; PostgreSQL16/RustFS age+paired DB/data WORM offline disaster restore [43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) PASS; functional 3x actual Patroni PG16 + 3x TLS etcd promotion/strict fencing/rejoin/quorum-loss [43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) PASS, **same ephemeral host** (not 3 distinct physical host/network-fault certification). Integrated real native Rust Prometheus HTTPS/bearer/X.509 [43886988152](https://github.com/desalvo/multi-backup/actions/runs/43886988152), focused Rust [43886988008](https://github.com/desalvo/multi-backup/actions/runs/43886988008), real PostgreSQL [43886987919](https://github.com/desalvo/multi-backup/actions/runs/43886987919) PASS on r138.
- New SNMPv3 with genuine native Rust metric generator requested in r140; its own job result must be checked (earlier SNMPv3 transport-only [43875372585](https://github.com/desalvo/multi-backup/actions/runs/43875372585) PASS). **No release**: final Production Gate has not passed on the latest branch; earlier full workflow [43881879293](https://github.com/desalvo/multi-backup/actions/runs/43881879293) failed on missing Python PyYAML fixed in r127, but a fresh final-sha Production run must confirm.
- Remaining RC gates: true independent 3-host network isolation, split-brain/fencing and failback/rejoin; real HTTPS REST monitor.read token lifecycle and external VLAN TLS; real PVE/PBS backup archive/restore and VMware/oVirt native VM backup/restore, or explicitly excluded from a limited RC; macOS/Windows/Linux native install/update/rollback including signature+notarization; end-to-end Production/security gate on exact final SHA; manual operator acceptance. GPFS suspended and excluded from all RC gates.

One-line English commit summary: Consolidate tested VMware and oVirt native VM inventory and refresh evidence-based release gate inventory.


## r142 — final targeted acceptance ledger and RC gating, 16 November 2026

The latest consolidated source SHA is r141 `8ebfcde4c12f0212111fab462af19ba722db715f`, with **all three Rust focused component jobs PASS** in [#43887673635](https://github.com/desalvo/multi-backup/actions/runs/43887673635): encrypted core, native desktop compile and server tests. The r140 genuine SNMPv3 authPriv + **actual Rust native monitor-snmp-status** acceptance [#43887445516](https://github.com/desalvo/multi-backup/actions/runs/43887445516) is also **PASS**; wrong key and SNMPv2c were rejected on the isolated loopback lab. The earlier SNMP-only test's synthetic metric limitation is now addressed.

Consolidated code-level acceptance:
- Real 3-process Patroni/PG16 + 3-process etcd mTLS strict synchronous leader crash/promotion/rejoin/DCS fencing: [#43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) PASS; single host only
- Rook/Ceph RGW actual 3 virtual Kubernetes worker/OSD+MON WORM COMPLIANCE/negative deletes: [#43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) PASS
- PostgreSQL16 and RustFS WORM age encrypted paired database/data disaster restore: [#43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) PASS
- Native Rust Prometheus HTTPS verified CA + bearer and wrong-token/CA/plaintext rejections on integrated r138: [#43886988152](https://github.com/desalvo/multi-backup/actions/runs/43886988152) PASS
- Native Rust PVE QEMU/LXC single-VM/CT backup task submit/poll and PBS VM/CT/host snapshots: [#43885350815](https://github.com/desalvo/multi-backup/actions/runs/43885350815) PASS, integration r134
- Native Rust VMware and oVirt VM-only read-only inventories: [#43887673635](https://github.com/desalvo/multi-backup/actions/runs/43887673635) PASS, integration r141; no CT claims
- Genuine Net-SNMPv3 with native Rust producer: [#43887445516](https://github.com/desalvo/multi-backup/actions/runs/43887445516) PASS, integration r140

**No complete RC:** entire full Production/security gate still NOT PASS on final SHA (earlier failed environment bootstrap fixed in r127 but no final full run); signed/notarized cross-platform native installers and update/rollback, 3-independent-host PostgreSQL/etcd partition/fencing, real REST monitor.read HTTPS+PG lifecycle, and real native hypervisor backup archive+create-only restore remain open. GPFS suspended and excluded from required RC gates. No main merge, release tag or stable publication occurred.

One-line English commit summary: Record green native Rust/SNMP integration evidence and explicit remaining RC release gates.


## r154 — integrate parallel tested Rust GUI, server/agents and GPFS protocol plugin (16 Nov 2026)

- GUI r147/r150: real Rust eframe all-provider searchable registry and safe navigation, nonsecret native prefs atomic owner-only 0600. Native GUI focused compile CI [#43897809149](https://github.com/desalvo/multi-backup/actions/runs/43897809149) PASS; updated prefs [#43900053687](https://github.com/desalvo/multi-backup/actions/runs/43900053687) PASS. This is not a finished signed installer/GUI E2E.
- Server r148: Rust cluster replica now obtains a bounded 1..64 job execution permit **before** claim; default max 4, queued work left durable in PostgreSQL when at capacity. Focused Rust server CI [#43897999559](https://github.com/desalvo/multi-backup/actions/runs/43897999559) PASS. Distributed actual multi-machine and storage manifest fencing remain RC gates.
- Plugin r143–r145: GPFS client compatibility using standard NFS or Samba mounted protocol, FULL/INC age encrypted and create-only network recovery. Real five-job [#43891408917](https://github.com/desalvo/multi-backup/actions/runs/43891408917) PASS and focused Rust core [#43891649347](https://github.com/desalvo/multi-backup/actions/runs/43891649347) PASS. Explicitly **NOT** real IBM CES/native GPFS certification.
- Created `docs/PLUGIN_READINESS_R154_IT.md` with all 33 registered input/output source/target statuses and verified CI references. No claims that catalog-only Planned stages imply executable backup.
- Dedicated Patroni separate-stack netns fault injection being debugged in independent branch; latest documented original functional three-process same-host HA remains PASS #43883579584, physical host HA unqualified. This integration deliberately excludes unpassed netns script.
- Repeat focused Rust GUI+server+core tests and real NFS/SMB matrix on this exact SHA before any RC promotion. No publication/merge to main.

One-line English commit summary: Integrate verified Rust desktop, guarded server workers and GPFS protocol-only NFS/SMB with a complete 33-plugin readiness matrix.


## r157 — genuine three-namespace real network Patroni/etcd acceptance integrated

- [Real HA run 43901963939](https://github.com/desalvo/multi-backup/actions/runs/43901963939) **PASS** with three actual PostgreSQL 16/Patroni and three mutually TLS-authenticated etcd members, each on an independently routed Linux network namespace and point-to-point veth stack on one disposable hosted runner.
- Six namespace reachability probes PASS; witness transaction ACKed on strict synchronous primary; real `iptables DROP` severed the primary from both peers; surviving etcd majority elected a new primary; previously ACKed WAL remained readable; old isolated primary did NOT acknowledge a write; after healing the old leader rejoined as replica and exactly one primary remained. Explicit teardown cleans only ephemeral host subnetwork/forwarding and namespaces.
- Earlier [functional HA 43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) remains PASS for leader process crash, strict no-sync write fencing, both standby rejoin and DCS majority-loss blocking.
- This is **NOT 3 physical hosts/racks or certified external network appliance fault domains**; it certifies genuine isolated Linux network stacks and real peer network partitions. This new integration triggers a one-shot exact-SHA rerun on the active RC feature branch.
- This r157 work does not complete native installer notarization/signatures, hypervisor backup media+restore or final Production Gate. No main merge or release tag.

One-line English commit summary: Integrate real isolated Patroni PG16 and TLS etcd partition, strict fencing and confirmed failover acceptance.


## r158 — server/agent replica capacity and admission startup fail-closed (16 Nov 2026)

- Rust server validates `MULTI_BACKUP_REPLICA_MAX_JOBS` is 1..64 and, when execution is explicitly enabled, that `MULTI_BACKUP_CLUSTER_MAX_ACTIVE_JOBS` is a valid 1..64 number **not exceeding the replica's published PostgreSQL worker capacity**. Invalid configurations abort the server startup instead of accepting jobs the process cannot safely execute.
- Actual native worker already obtains and holds a local semaphore permit before any durable PG job claim and through lease heartbeat and final status commit (r148). A saturated replica leaves pending jobs in PostgreSQL for other eligible workers. Rust unit tests cover boundary/rejection cases. [Focused server CI 43902435010](https://github.com/desalvo/multi-backup/actions/runs/43902435010) **PASS**.
- This is admission control, not a certified global exactly-once transaction/storage write fence; real multi-replica network failure testing and manifest atomic fencing remain open. No signed RC/stable release has been issued.

One-line English commit summary: Reject mismatched replica admission limits at server startup before distributing jobs.


## r160 — extend PVE Rust recovery prerequisites with actual remote backup media inventory

- Native `plugins pve-backup-media --config-json CONFIG --node NODE --storage STORAGE --guest-id VMID --json` lists provider-identified PVE `vzdump` backup media using the read-only endpoint `GET /api2/json/nodes/NODE/storage/STORAGE/content?content=backup&vmid=VMID`. Bounded HTTPS/no redirect, owner-only scoped token; rejects other guest's VMID, another datastore, unsafe volume paths, duplicates, oversized or malformed responses and cross-provider credentials.
- Information includes VM/CT subtype when source explicitly provides it, format, volume ID, size/time, protected flag and provider verification outcome. Crucially `multibackup_archive_verified=false`, `exported_to_multibackup=false` and `multibackup_restore_certified=false` regardless of the provider state.
- [Focused Rust core unit CI 43904400603](https://github.com/desalvo/multi-backup/actions/runs/43904400603) **PASS** on source r159. A real authenticated Proxmox cluster, ciphertext archive capture, restore-new VM/CT and verification against source remain RC/field blockers.
- Integrated into current Rust RC development line, not main or a release tag; full final Production Gate remains mandatory.

One-line English commit summary: Integrate tested PVE native backup-media inventory with strict per-guest verification provenance.


## r161 — native GUI secure provider inventory panels (PVE/PBS/VMware/oVirt)

- The real Rust egui Plugin page now offers owner-only JSON API configuration paths (never persisted in native settings) and read-only actions: PVE/PBS inventory, native PVE per-VMID backup media listing, PBS datastore/namespace snapshot list and VMware/oVirt native VM inventory. Uses actual typed Rust adapters, verified HTTPS and bounded response; no subprocess, shell, destructive VM actions or hidden automatic backup claims.
- Async worker keeps UI responsive and writes only sanitized/redacted, at most 12k characters, into local inventory preview; a separate success message explicitly says not archived/restored/verified by Multi Backup. Native GUI tests reject persistence of API config path and other credentials, and guard all unavailable operations.
- PVE per-VMID backup media parser and direct CLI came from r159 and have separate [Rust core CI 43904400603](https://github.com/desalvo/multi-backup/actions/runs/43904400603) PASS. A real physical PVE/PBS/VMware/oVirt lab and create-only VM/CT restore are still unqualified; compile and native GUI runtime acceptance must pass on this branch before merge.

One-line English commit summary: Expose typed read-only PVE/PBS and VMware/oVirt inventories in native Rust GUI without persisting API secrets.


## r163 — genuine native Rust egui/Xvfb runtime acceptance

- An opt-in disposable GitHub-hosted runner compiles the actual Rust desktop app, runs its built-in artwork/gradient/catalog self-test, and executes `--smoke` against an actual Xvfb X11 framebuffer and Mesa software OpenGL. This exercises the eframe application loop, native window creation, three rendered frames and graceful close, rather than only `cargo check`.
- The job uses isolated temporary HOME/XDG config, no real credentials or production archives, and deletes all temporary files on exit. A startup panic, broken graphics/OpenGL setup or inability to close fails CI.
- A passing job is limited to native Linux GUI runtime. It is not a visual screenshot certification, signed/notarized macOS/Windows installer qualification or complete UI backup and restore journey.

One-line English commit summary: Verify native Rust egui GUI startup and rendering through isolated Xvfb CI.


### r164 — 16 Nov 2026 — open integration branch

- Branch: `feature/r164-protected-hypervisor-config`, parent branch `feature/r163-desktop-real-xvfb-smoke` (Linux Xvfb real GUI #43906909522 PASS), upstream RC PR #22 remains DRAFT. Read-only PVE/PBS/VMware/oVirt API credential JSON now uses shared Unix O_NOFOLLOW, fd inode/mode check, strict size cap, and zeroized input. No hypervisor backup or VM/CT restore is thereby certified.
- Initial focused Rust #43908977857 failed only a new 0644 test fixture; 114 tests passed. Fix sets 0600 in fixture; check follow-up exact-SHA CI before any integration. Keep GPFS protocol-only NFS/SMB, native GPFS postponed.
- Outstanding RC gates: signed native Mac/Windows/Linux packages, installed updater/rollback, real multi-machine agent quotas and fenced shared-storage manifests, physical 3-host PG16 Patroni/etcd acceptance, live hypervisor backups/restores, REST external TLS/PG token acceptance, final security/Production CI. No main merge/tag/stable release.


### r166 — Native GUI/server/agent integration and three-host HA field preflight (16 Nov 2026)

- Continue from `feature/r166-ha-gui-agents-acceptance` (parent r164, r163 native Xvfb run PASS). Full integration is intentionally Draft until exact SHA CI passes; code source and new tests must be reviewed before merge.
- `rust-desktop/src/main.rs`: added Server e agenti page querying HTTPS /v1/status and /v1/agents with independently supplied 0600 `mbmon_` token path. No token in native settings. Invalid origins with HTTP/userinfo/query/fragment or path fail closed.
- `rust-server/src/monitoring.rs`: monitor.read protected agent aggregate counters only, no username or profile hash. `rust-server/src/agent_api.rs`: atomic no-overwrite hardlink ciphertext publication, private append-only behavior at file namespace level, full backup receipt still requires PostgreSQL commit; no WORM certification.
- `rust/src/bin/multi-backup-agent.rs`: use shared bounded O_NOFOLLOW secure file reader; first-use credentials use persist_noclobber; existing sequence counter intentionally retains atomic replacement under lock.
- `scripts/ci/ha-three-host-field-r166.py` and manual field workflow: three genuine remote kernels required; real outage/split-brain and independent physical fault domains cannot be certified without a dedicated operator-provided lab. Run self-test first.
- Remaining: live 3-host Patroni/etcd failover and fence, server+agent E2E ciphertext/restore on independent hosts, GUI Mac/Windows/Linux signed installation/upgrades/rollback, final full Production Gate on exact SHA. No release/tag/main merge.


### r166 signed agent upload live acceptance extension

- `rust-server/src/live_pg_acceptance.rs`: ignored CI test invokes actual streaming upload handler under genuine PG16, real locally generated age ciphertext, Ed25519 credentials, durable on-disk spool, PG receipt, real decrypted byte-level restore, collision 409 and revoked 401.
- `.github/workflows/agent-cluster-live-pg.yml`: installs age binary for the test. Do not mark this as remote TLS/three-host certification. Recheck exact CI result before promoting.


### r168 — HA self-hosted private controller and nonduplicate software acceptance

- Branch `ci/r168-software-field-gates` is stacked on `feature/r166-ha-gui-agents-acceptance`; preserve the existing PR history. The new software-readiness workflow reuses the exact Rust focused, PostgreSQL16 agent, real Xvfb GUI, hardened OCI gateway and Kind Kubernetes test scripts and aggregates a single SHA. Its dedicated CI branch avoids simultaneous automatic reruns of the feature workflows.
- Existing field HA workflow reuses the **same** `scripts/ci/ha-three-host-field-r166.py`; no duplicate parser negative-fixture tests on the self-hosted runner. Documentation and a pinned-public-SHA example wrapper for a **private** GitHub repository running an **ephemeral** self-hosted controller are in `docs/HA_RUNNER_SELF_HOSTED_R168_IT.md` and `docs/examples/ha-private-runner.workflow.yml`. No runner registration or secrets available from current connector; operator must configure the real lab.
- Gate boundaries: positive software CI does NOT certify three independent physical Patroni hosts, actual network split, signed native installers across macOS/Windows/Linux, all plugins or final RC. Do not overstate readiness or merge/tag early.


### r170 — continue from CI r168 base

The existing PG16 ignored acceptance test now includes real database-backed `monitor.read` API status/agent-count checks (token denial, revocation and identity nonleakage) on actual Axum handler. No extra duplicate PG job. egui REST schema summary is unit-tested; literal backslash-n display bug fixed; the existing GUI Xvfb smoke loops through 10 pages and About in one process. Rust-focused desktop job now runs `cargo test --all-targets`. The single-SHA gate was updated to watch relevant source/test files. Confirm green CI exact head before marking extended acceptance PASS. See `docs/RC_READINESS_R170_IT.md` for exact 33-plugin x4 direction readiness; zero newly promoted Ready without real provider qualification. Physical 3-host HA still needs a private ephemeral SSH-runner lab and operator authorization; no vendor systems or credentials available.


### r174 RC1 qualification and notarization exception

- User explicitly allows **RC** unnotarized macOS artifacts; stable signing/notarization controls remain mandatory.
- Production CI https://github.com/desalvo/multi-backup/actions/runs/43932195582 source `1f1a726a3ffed7b46a7498b99891bfb935570865` is **fully green** after rerun of only cancelled native agent macOS arm64 PKG job: 32 mandatory successes, 3 intentional skips, 27 complete Rust artifacts.
- Dedicated `rc-candidate-r174.yml` reuses live Prometheus HTTPS, Net-SNMPv3 authPriv and Patroni netns workflow suites with `workflow_call`, authenticates pinned production run/artifact JSON using `scripts/ci/validate-rc-evidence.py`, runs negative test fixtures, collects only 27 Rust binary artifact groups from that exact completed run and publishes `rc-0.1.0-rc.1` as a **prerelease with clear limitations** when all RC jobs PASS. Source SHA is pinned and no executable source diff accepted from it.
- This RC is a **full application software preview**, not proof of all 33 plugin implementations, independent three-physical-host HA, remote TLS agent field, vault resistant to shared root, notarization or stable production readiness. See `docs/RELEASE_CANDIDATE_R174_IT.md`.


### r175 immutable RC publication resume

- Existing live RC workflow [#43945759433](https://github.com/desalvo/multi-backup/actions/runs/43945759433) on `f314bc3b5043e9ef70fe43c573d6ba84f360158d` ran four independent PASS jobs: exact source CI provenance, live Prometheus TLS, live Net-SNMPv3 authPriv, Patroni+etcd netns failover/fencing. Publication failed **solely** at existing `collect-rust-release.py` because Windows EXEs are nested under `installers/` inside the zip artifact.
- `scripts/ci/normalize-rust-native-artifacts-r175.sh` normalizes only those two known filenames after verifying input regular file and absence of symlink, and compares byte/sha256. Negative tests in `tests/test_rc_windows_normalize_r175.py`. Existing r174 workflow made **manual-only** with same normalization, avoiding repeating live monitoring tests.
- `.github/workflows/rc-publish-r175.yml` auto-triggers on its own addition; pins original binary source `1f1a726a3ffed7b46a7498b99891bfb935570865` run `43932195582` 32/32 required jobs, pins live RC run `43945759433` four successes and expected failed collector only; releases `rc-0.1.0-rc.1` prerelease with manifest and SHA256SUMS only if all fail-closed checks pass. Does NOT rerun native builds or Patroni/Prometheus/SNMP, and does NOT publish stable.
- When reporting, verify actual new RC run/release outcome. Do not claim three physical hosts, root-proof vault, all 33 providers, Windows publisher signing or Apple notarization as complete.


### r177 — verified GitHub RC1 publication

- **DONE:** https://github.com/desalvo/multi-backup/releases/tag/rc-0.1.0-rc.1 is publicly visible as a GitHub prerelease (non-draft), pinned to binary source `1f1a726a3ffed7b46a7498b99891bfb935570865`. Exactly 31 complete uploaded assets incl all 27 Rust-native builds + SHA256SUMS + manifest + RC documentation.
- **CI final publisher PASS:** https://github.com/desalvo/multi-backup/actions/runs/43947524553 (2/2). Prior r174 completed four service/security/HA functional jobs all PASS; r177 uses their exact evidence rather than rerunning. Production matrix 32 PASS https://github.com/desalvo/multi-backup/actions/runs/43932195582 .
- Signatures/notarization are waived ONLY for explicit RC preview, not stable. Do not confuse real NetNS Patroni partitions with three independent physical hosts. The app has 33 provider contracts but not all generic plugin executors. Field-only and stable signing blockers remain unclosed.
- `main` unmodified. PR #30 remains Draft; do not silently merge or promote prerelease to stable without qualification.


## r176 branch handoff

Branch `feature/r176-native-rc2-updates-fencing` based on r175 PR #30. See `docs/RC2_READINESS_R176_IT.md` for exact changed code, test scope, and remaining blockers. RC1 published source unchanged; do not claim new RC2 or stable readiness until the new SHA passes Rust, real PostgreSQL, real MinIO and Mac/Windows runtime validation. Full OS-install auto-update and physical 3-host HA remain unverified.


### r177 — Windows native renderer compatibility and release prerequisite

Real RC1 native Mac windows arm64 + x86_64 PASS, but Windows real GUI window exited 1 on Windows-2025 runner despite prior self-test passing (full workflow #43951219161 FAIL). r177 uses Windows-only WGPU renderer and adds a **current-source** Windows runner build + all-pages graphical smoke with stdout/stderr diagnostic. Never mark the published RC1 Windows executable repaired; it is immutable and still Glow-only. Qualify new source SHA, rebuild Windows installer, publish RC2 only after production gate and Windows runtime success. Native Win GUI test may require interactive runner with GPU support if hosted runner forbids graphical window; do not waive it or fabricate a PASS.


### r178 plugin handoff

Stacked branch `feature/r178-s3-input-dropbox-output`, separate from existing draft PR #31. New dedicated Rust `plugins s3-object-backup` / `dropbox-upload` / `dropbox-backup` commands; no generic mount/destination downgrade. S3 B-In requires version ID + caller-supplied SHA256 and commits encrypted chunks only on verified transfer. Dropbox B-Out uses create-only upload sessions and independent file metadata readback; requires `MULTI_BACKUP_DROPBOX_TOKEN`. New RustFS live test; isolated Dropbox B-Out live test manual-only, strongly gated. Run exact-SHA CI, do not claim Ready until real vendor field and E2E all operations. `docs/PLUGIN_S3_DROPBOX_R178_IT.md` has steps and limitations.


### r179 provider output branch handoff

Branch feature/r179-provider-output-hardening, stacked on r178 PR #32. See docs/PROVIDER_PLUGIN_READINESS_R179_IT.md. New mounted_output.rs, native CLI mounted-backup/mounted-restore (actual age ciphertext only), registry B-Out Preview for eight mounted providers, live 5-mode NFS/Samba/GlusterFS/GPFS compatibility workflow extended. No false native GPFS field claim or SAN/proxmox/oracle promotion. Await exact-SHA CI and field validation, no RC2 tag.


### r180 native SSH B-In

On stacked r179 branch, new `ssh_input.rs`, `plugins ssh-backup` and dedicated OpenSSH real-sshd CI test. Safe path/user/host only, caller SHA256+size and real hostkey pin, manifest-last after process exit. Root encrypted store, remote read without plaintext staging. Direct SCP/SFTP and SSH B-Out are NOT implemented, no release/Ready until CI.


### r181 followup

RBD/PVE B-Out upgraded from Unsupported to Planned ONLY, no runtime executor. Real SSH B-In #43989432365 failed (exit=255 due OpenSSH option split); fixed in next SHA by using `UserKnownHostsFile=<abs path>` as one `-o` option. See r179 provider readiness doc. Do not mark SSH B-In CI PASS until rerun.


## r182 field-CI lab handoff

Branch feature/r182-cephfs-lustre-oracle-field-ci stacked on PR #33. New CephFS kind workflow uses existing real 3 MON/3 OSD Rook RGW cluster + MDS CephFS and isolated Ubuntu client pod; actual Rust age encrypted B-In and B-Out via fuse.ceph-fuse. New Lustre workflow uses pinned LTVM genuine Rocky9 guest, compile same SHA static Rust and pass SHA256-verified binaries over ephemeral bridge; actual Rust Lustre B-In/B-Out and restore. Oracle Free workflow rerun with additional wrong-age-key negative on real RMAN pieces. Historical real AFS certified #43747477666 unchanged adapter module+test script. Only mark green when exact-SHA runs succeed, not on scripted lab existence. See docs/PLUGIN_FIELD_READINESS_R182_IT.md.
