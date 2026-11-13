# Multi Backup - portable project handoff

**Project:** `desalvo/multi-backup`  
**Version:** `0.1.0-r005-rc1`  
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
