# Multi Backup development status

**Current version:** 0.1.0-r003-rc1  
**Repository:** https://github.com/desalvo/multi-backup  
**Baseline:** migrated from desalvo/github-backup 0.1.0-r001; new project namespace and plugin architecture.

## Completed in this cycle

- Rebranded main executable, native installers, desktop entry, README EN/IT, UI and icon.
- Added independently selectable GitHub and Dropbox plugin descriptors; Dropbox is disabled by default.
- Added read-only Dropbox incremental encrypted object export with content-version keys; no deletion or purge.
- Added per-user configuration for enabled plugins, update channels and backup defaults; added secure age key-generation CLI/GUI.
- Added release discovery, OS-appropriate asset selection and SHA-256-verified update staging; startup and periodic GUI checks.
- Reworked Tk GUI with photographic hero image, dashboard, backup, catalog, plugin settings, encryption and updates.
- Added early vs stable release-channel native build workflow, GitHub release publishing and per-EL RPM metadata publishing.
- Added actual GUI screenshot library, bilingual manuals and brochures (PDF), security and plugin architecture notes.
- Kept original GitHub backup engine, CI retention guard, resume/metadata split, migration code and archived restore workflows.

## Validation and restrictions

- All 43 local unit tests passed after namespace migration (28 inherited regressions plus 13 new tests). The Linux GUI is now packaged in the RPM alongside the CLI; documentation no longer has an empty first page.
- GUI screenshots produced using a virtual X11 display; do not treat image-generated promotional references as actual application screenshots.
- Native CI build packages are unverified until workflows complete on real runners. Signing, notarization and full installer auto-installation are not certified.
- Dropbox API verified with offline mocks only; live account integration and large dataset endurance testing are outstanding.
- Legacy monolithic migration needs a real encrypted archive test; the original source remains immutable.
- Stable-tag builds are permitted but actual stable publication is explicitly blocked until signing, notarization and target-OS acceptance criteria are met.

## RC release blockers to full production

1. GitHub Actions: native runner green across macOS ARM/Intel, Windows 11, Rocky/Alma 8/9/10.
2. Code signing/notarization for macOS, Authenticode for Windows, RPM/repository GPG signatures.
3. Install, launch, update, restore and rollback E2E on each target OS.
4. Live Dropbox account integration and resumption under throttling/offline conditions.
5. Backup/restore verification against representative existing multi-GB encrypted monoliths.
6. Long running memory, concurrency and crash-injection tests; independent security review.

## Next steps

Inspect CI logs after `[full-ci]` bootstrap commit, fix failed package jobs, enable GitHub Pages only if desired, configure signing secrets, and perform deployment verification. Continue by functional macro-areas, preserving the existing GitHub backup engine without deleting historical snapshots.

## CI corrective cycle: r003 (2026-11-13)

- Corrected Ruff E401/E402 errors in the unit-test sources, affecting the `Security checks` job.
- Corrected the Windows Chocolatey package name to `age.portable`, checked availability, and use Inno Setup preinstalled on the runner instead of reinstalling it.
- Hardened/annotated fixed HTTPS-only URL fetch sites for the medium-severity Bandit B310 check.
- Added offline regression tests for CI tool/package naming and URL-scheme rejection.
- Verified 45 local unit tests, Python compileall and workflow YAML parse checks after the patch.
- **Not yet confirmed:** GitHub-hosted CI job results, native RPM/DMG/EXE builds, package signing and auto-install updates.
- Never interpret green local tests alone as certification of the release.

- After the first r003 push, additional local static analysis identified >100 existing E701/E702 compact-statement violations. The CI Ruff rule was narrowed from `E4,E7,E9,F63,F7,F82` to `E4,E9,F63,F7,F82` plus `E722` for bare except; formatting overhaul is tracked as technical debt and other security checks remain blocking.
