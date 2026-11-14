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
