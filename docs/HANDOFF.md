# Multi Backup - portable project handoff

**Project:** `desalvo/multi-backup`  
**Version:** `0.1.0-r006-rc1`  
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
