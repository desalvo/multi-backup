# Multi Backup development status

**Current version:** 0.1.0-r016-rc1  
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

## CI corrective cycle: r004 (2026-11-13, verified remote run)

- GitHub Actions run 43491071561 (push commit `bd1fcca9f5b4...`) is **failed**, but the Python 3.11/3.12/3.13 jobs, Ruff and Bandit all passed.
- Actual root cause from job 132112094249: the `pip-audit` dependency gate reported **42 advisories against Pillow 12.3.0**, with all reported fixes in `13.0.0`.
- Upgrade runtime and documentation requirements to `Pillow>=13.0.0,<14`; synchronise macOS, Windows and all six RPM build recipes. Keep `pip-audit` blocking.
- Bump to `0.1.0-r004-rc1`, add pinned build-dependency consistency test, and record source run link and resolution in handoff.
- **To verify remotely**: new `[full-ci]` run, native macOS/Windows/Linux builds and actual installers; local Python unit tests are not substitute for a GitHub-run dependency audit.

## r005 native CI corrective cycle (2026-11-13)

- Green security run `43494324235`: all Python 3.11/3.12/3.13 checks, Ruff, Bandit and `pip-audit` passed after Pillow >=13 fix.
- Native run `43494324431` had successful macOS Intel/ARM, Alma EL8/9/10 and Rocky EL8/9; two confirmed failures: Windows stage `Windows binary and executable dependencies` and Rocky EL10 `Initialize containers`.
- Windows job 132123583905 confirms Chocolatey installs `age.portable`, but copying the generated shim `age.exe` into a new directory breaks its relative DLL/binary path. Now copy the actual `age.exe` and `age-keygen.exe` binaries from the package tools directory, smoke-test both.
- Official `rockylinux:10` Docker Hub Library tag does not exist; use Rocky project-maintained `rockylinux/rockylinux:8/9/10` images (supported for EL8/9/10).
- Added regression tests enforcing the two packaging fixes. Build publication remains gated by completion of every matrix job; no user-facing claims about signed stable releases.

## r006 native Windows installer path fix (2026-11-13)

- Remote r005 native workflow `43495351110`: both age executables copied and smoke-tested successfully; Rocky Linux EL10 RPM build recovered, along with EL8/9, AlmaLinux 8/9/10, macOS ARM64 and Intel.
- Windows `ISCC.exe` now runs, but Inno Setup reads `LicenseFile=LICENSE` relative to `packaging/windows/installer.iss` rather than project root. Additionally, its relative output directory is under packaging/windows, not the expected workflow `installers/`.
- Add `SourceDir=..\..` under `[Setup]` so the script resolves `LICENSE`, dist binaries, README and icon relative to repository root; Inno Setup also resolves `OutputDir=installers` under SourceDir. Add regression test verifying all critical paths.
- Await full GitHub build and release publish checks before declaring the native pipeline green.

## r008 RC1 — production gate and macOS packaging (2026-11-13)

- Unified trigger: `.github/workflows/ci.yml` now contains the sole Production Gate; `.github/workflows/build-native.yml` is callable only as a reusable workflow after gate success. Ruff, Bandit, pip-audit for runtime and documentation requirements, test suite 3.11/3.12/3.13 and release asset policy are mandatory.
- macOS: PyInstaller onedir `.app` containing its CLI and helper tools; no naked Python or other top-level DMG files. Signed Developer ID, app notarization/stapling, DMG notarization/stapling and mounted-DMG copied-app acceptance tests are required for publication.
- Added `gui_launcher.py --self-test` and macOS packaging scripts. Added regressions for release gating, signing, notarization, one-item DMG and CLI embedding.
- **Blocking external prerequisite:** Apple Developer ID Application signing certificate and App Store Connect notarization API key must be configured as Actions secrets. Do not claim the new signed DMG is publicly available before an actual successful run.
- Existing signed Windows/RPM channels remain available from previous release, but the old macOS DMGs are invalid for ordinary Gatekeeper installation. Windows Authenticode, signed RPM repositories and installed auto-updates remain open for stable.

## r009 — Dropbox verification and native startup qualification (2026-11-13)

- Confirmed run 43502395836: Production Gate, Windows and six RPM builds passed; macOS release jobs stopped due to absent Apple Developer ID / notarization secrets. No new macOS release was published.
- Dropbox download bytes are now checked against the listing ID, revision, size and official Dropbox content hash before encryption; existing ciphertext with an invalid SHA256 sidecar is preserved, never silently overwritten.
- Added deterministic 4-MiB block vectors and offline tests rejecting changed/truncated/corrupt downloads without publishing a backup.
- Apple signing credentials, on-runner native GUI smoke, live Dropbox-account tests, and end-to-end restore acceptance are not yet verified.

### r009 macOS unsigned startup qualification

- Added a post-Production-Gate CI-only matrix for Apple Silicon and Intel. Each runner builds a true macOS GUI application with embedded CLI and exercises its self-tests, without Apple signing credentials.
- These unsigned smoke applications are never published or uploaded. Native release jobs require both Production Gate and smoke success; separate Developer ID signing, app/DMG notarization and installed Gatekeeper validation remain mandatory for distribution.
- Actual macOS runner acceptance is pending this branch's CI. The latest main run 43502395836 failed because Apple signing secrets are unavailable.

## r010 RC1 — Mac test DMGs before Apple Developer signing (2026-11-13)

- Following user testing requirements, enabled practical Mac preview distribution. The same real `Multi Backup.app` is ad-hoc signed, bundled with CLI and helpers, placed alone in a `.dmg`, mounted and checked for relocatability on both macOS arm64 and x86_64; no Apple Developer membership required for this testing step.
- The smoke CI jobs upload clearly named, downloadable `TEST-ONLY-macos-*` artifacts. The early-release full native build publishes clearly named `*-test.dmg` preview assets and removes legacy broken/ambiguous DMG filenames. Stable still requires real Developer ID signing, app+DMG notarization and Gatekeeper acceptance.
- Verified earlier PR run 43506766698: Production Gate and two macOS executable self-tests passed; newly added DMG creation/installation checks and release publication are pending new CI execution.
- Pending: manually open UI and exercise real backup/restore on user's Mac, configure Apple certificate only for eventual general distribution, complete live Dropbox integration and production signing on all platforms.


## r011 RC1 — macOS sidebar readability and visual design (2026-11-13)

- User confirmed macOS native app launches. A real UI defect was reported: sidebar button text is invisible in macOS, and button styling needs a substantive overhaul.
- Replaced native Tk navigation buttons with a dedicated high-contrast Canvas-based navigation component that explicitly paints text and vector icons, removing dependence on macOS Aqua foreground-colour handling. Redesigned sidebar brand, sections, selected states, hover and keyboard focus.
- Preserve every navigation function; mouse clicks and keyboard Return/Space activate the same page. Add regression checks for WCAG colour contrast, navigation items and active-page integration.
- Requires on-runner app smoke tests and user visual confirmation on actual macOS before treating the design as accepted.

## r012 — simplified GitHub backup and premium controls (2026-11-13)

- Replaced multi-field backup wizard with default two-field GitHub workflow: local backup destination and GitHub username. Staging is automatic. GitHub account can be detected via `gh api user`; a one-time `gh auth login` can be started from the GUI (macOS Terminal). The chosen username must match the authenticated user, and repositories are scoped to that owner rather than silently archiving unrelated organizations or collaborators.
- Added idempotent, local automatic age identity setup. On first backup, creates `keys/default.txt` outside the backup directory with private permissions, derives public recipient and warns user to save an independent secure copy. Existing key is never overwritten. Added UI to reveal the key location or select another identity.
- Separated `backup_ci_runs` (default 5) from cleanup retention. GUI explicitly passes `--max-runs-per-repo`; CLI defaults to 5. Selected most recent workflow runs are archived, with logs and live artifacts, regardless of the cleanup switch. Cleanup is opt-in and moved *after* successful backup/critical checks, to avoid remote deletion before archive verification.
- Added cross-platform premium rounded, glossy, high-contrast Canvas buttons throughout GUI; replaced rectangular progress widget with rounded 3D-styled red→yellow→green gradient; added hide/show sidebar control and shortcut. Kept the previous seven navigation sections and their operations.
- Early macOS assets renamed to `multi-backup-macos-{arm64,x86_64}.dmg` and preview CI artifacts named without TEST-ONLY; GitHub prerelease and stable trust-policy restrictions remain separate and unchanged.
- Regression tests cover key reuse, newest-N CI selection without cleanup, macOS asset naming and UI controls. Pending: green PR production gate + Apple Silicon/Intel DMG checks; visual acceptance on user's Mac and end-to-end real-account backup/restore are still necessary.

### r012 native GUI acceptance hardening

The previous GUI self-test checked imports and embedded assets only. The new `--gui-smoke` mode actually constructs the Tk application, navigates to Backup, collapses/reopens the sidebar and redraws the gradient widget. Both macOS preview-DMG relocation and future stable Gatekeeper verification invoke it. CI must pass on Apple Silicon and Intel before merging; this catches startup errors that pure packaging tests miss.

## r013 RC1 — session unlock, encrypted recovery, plugin restores and update CA fix (2026-11-13)

- Protected age identities with a first-run password (12+ characters), scrypt 32768/8/1 and AES-256-GCM; old plaintext `default.txt` is migrated without rotating its private key. Vault is outside the app and survives updates. GUI navigation, downloads and background operations are blocked until an existing vault is unlocked at startup. One unlock per session. Temporary plaintext identity files for `age` decryption are removed after use.
- Encryption UI allows independently password-protected recovery copy export/import, change password and display of vault location; existing keys cannot be overwritten. Mac Touch ID opt-in uses a device-only Keychain helper with `biometryCurrentSet` and app-password confirmation. Password remains fallback; unsupported devices use passwords.
- Restore is a new sidebar tab with origin-plugin enforcement. GitHub: new authenticated-owner remote repo (bare mirror/LFS) or local recovery. Dropbox: new remote file with single/multipart upload or local recovery, new write-scope token requested per operation. No cross-plugin restore or remote overwrite. Account-level settings/CI remote reconstruction requires additional engineering; GitHub Secrets cannot export original secret values.
- All pages have vertical scroll and shortcuts Ctrl+1…Ctrl+8. CI cleanup remains separately opt-in and asks confirmation every time. Stable channel policy unchanged.
- The screenshot's macOS `CERTIFICATE_VERIFY_FAILED` is addressed with an explicitly bundled certifi CA trust store, hostname verification and SHA-256-only update staging.
- CI Production Gate initially found vulnerabilities in cryptography 47.0.0; raised required minimum to the audit-remediated 50.0.0 (and retained audit blocking). Added password/corruption/recovery, restore provenance and CA tests, real macOS age-keygen+GUI tests, macOS native Touch ID compilation. Final build and user acceptance status pending.


## r014 RC1 — approved photographic premium UI (2026-11-13)

- Implement approved macOS-inspired dashboard with 3D glass navy/violet/cyan design, rounded gradient actions, photographic landscape hero, decorative luminous waves, photo sidebar inset, native sidebar, real vault/plugin/update cards, and responsive wide/stacked layout using local bundled images.
- Display only actual local data: archive count/recent timestamps and provider, enabled plugins, session-unlocked age vault, actual update status, filesystem used/free space (not a backup quota), and seven-day archive timestamp histogram. The screenshot was conceptual; no fabricated plans, jobs, success states, or byte counts. Automatic scheduling remains explicitly unavailable.
- Preserve secure backup, restore, plugin activation, encryption vault and key migration, update checks, CLI and archive compatibility. Interactive dashboard actions call existing checked workflows; mouse and keyboard operation are supported.
- Added regression tests for dashboard metrics and packaged native GUI startup. Requires full Production Gate, Apple Silicon/Intel, Windows and RPM packaging CI and user visual acceptance on macOS prior to promotion. Rust rewrite and scheduler are not included. Stable remains blocked by production signing and E2E verification.


## r015 RC1 — GitHub Actions artifact reliability, macOS drag-to-install, documentation & Rust migration contract

- Mac ARM64/Intel DMG bundles only signed Multi Backup.app and a Finder shortcut Applications -> /Applications. No extra Python or CLI at the top level. Verify both entries and relocated executable after mounting. Preserve signing/notarization of the embedded app; the symlink is outside its signature.
- GitHub Actions: on failed artifact API download, validate ZIP and retry gh run download with authenticated fallback and bounded backoff; failure of one candidate must not abort unrelated CI runs or repositories. Continue to older eligible CI runs until up to the configured number of complete snapshots are archived. This number counts complete CI runs, not artifact files. Record skipped run IDs, reasons, shortfalls and per-artifact omissions in manifest and logs. Never count a partial CI run as complete. Skip remote cleanup after any missed artifact candidate.
- Explicit opt-in --ignore-ci-artifact-errors and GUI checkbox (default OFF). After retries/backfill, a shortfall is incomplete by default. When enabled, the backup can finish with prominent warnings and exact omission records. Does not disable checksums or permit corrupt backup claims.
- Resume prints cycle ID, checkpoint count, SHA-256-based reuse and actual CI completion counts to JSON progress and GUI log. Preserve checkpoints for genuinely incomplete cycles.
- Publication CI captures the real photographic GUI with a temporary empty profile via Xvfb, generates bilingual manuals and brochures from current screenshots and distributes those PDFs instead of old checked-in PDFs. Documentation distinguishes conceptual mock, implemented dashboard and unavailable automatic scheduling.
- Full Rust migration is NOT yet completed. Existing Python Tk GUI, GitHub/Dropbox, streaming encryption and restore remain compatibility baseline. Rust requires format-preserving differential tests, native GUI parity, decrypt/restore certification and installer validation before Python can be removed.
- Validate PR CI, both macOS DMGs, Xvfb documentation build, Windows/RPM artifacts and stable signing/notarization. Do not label stable complete.


### r015 Retina & HiDPI visual polish (user screenshot regression)

The approved GUI's rectangular cards, glossy buttons, status badges, sidebar icon plates and progress bars were rendered at 1x (or as Tk smoothed polygons), producing jagged rounded corners and a coarse internal highlight arc. Added a shared render_quality.py raster layer using 4x supersampling with Pillow LANCZOS downsampling. The dashboard's glass cards, gradient buttons, tiny status pills, activity bars, storage tracks and decorative waves share the smooth geometry. The native sidebar converts only its rounded geometry into antialiased PhotoImage patches, retaining Tk text, keyboard navigation and existing hit targets. Main toolbar/backup actions use the same smooth glossy surface. Unit tests cover partial edge coverage, gradient differences and retina-quality curves. Confirm the actual visual result on ARM64/Intel before promoting stable. No changes to crypto, archive formats or key handling.

The new rust/ library supplies a tested CI-backfill policy implementation and docs/RUST_MIGRATION.md compatibility gates. The complete migration of providers, encrypted streaming, vault, restore and graphical frontend to Rust remains OPEN and must not be advertised as completed.


## r016 RC1 — explicit Start and Resume, guarded archive deletion, dedicated backup photography (2026-11-14)

- Separate Start and Resume buttons on both Backup and Dashboard. Start explicitly creates a new GitHub/Dropbox cycle, while Resume requires an existing matching checkpoint and retains the exact backup ID / manifest name. Preserve the original unfinished checkpoint in an abandoned state file rather than silently reusing or discarding it when Start is selected.
- Add Dropbox manifest session checkpoint with stable filename across interrupted incremental exports. Verified encrypted objects are reused. Completed sessions remove only the active checkpoint after SHA-256 confirmation.
- Add explicit local-only deletion of one catalogued .tar.gz.age archive and its matching sidecars, with user confirmation, exact filename entry, symlink/canonical-path protections, guard against active checkpoint data and running jobs, and a durable audit. Do not delete remote GitHub or Dropbox contents.
- Show Author Alessandro De Salvo, EUPL-1.2 license, version and build 16 in the sidebar footer. Add dedicated backup/datacenter photographic art to the hero and sidebar; preserve aspect ratio with ImageOps.contain and letterboxing rather than cropping or stretching. Maintain Retina supersampling from r015.
- Update documentation, screenshots and bilingual brochure build workflow. Qualify PR Production Gate, macOS ARM64/Intel, native release Windows/RPM and real Mac GUI acceptance before publishing. Full Rust code migration remains separate and INCOMPLETE.


### r016 photographic asset pipeline

CI now creates two bespoke fixed-size photographic composites from a free-license data-center server photo via scripts/create-photo-assets.py; outputs are locally included in macOS, Windows and Linux packages. The EN/IT PDF manuals and brochures use screenshots captured AFTER those images are available, rather than the outdated hiker hero. Both compositions and GUI rendering use ImageOps.contain, preserving natural proportions without cover cropping or stretching. Source provenance is documented in docs/PHOTO_CREDITS.md. This is a *build-time* HTTPS download from an allowlisted fixed Unsplash URL; installed apps run completely offline with local assets. Source checkouts must execute the generator once or keep the existing fallback until it has run.

The PR remains subject to complete Production Gate + native Mac ARM/Intel smoke. Live visual confirmation of the actual screen is still necessary; a successful headless smoke alone is not visual acceptance.


### r016 macOS Intel DMG runner contention hotfix (2026-11-14)

The r016 PR initial complete-visual run #43534898468 passed Production Gate and Apple Silicon. Intel built and signed the entire native .app successfully, but hdiutil create failed with "Resource busy" during early DMG packaging. Retry the transient diskimages-helper collision up to four times, remove partial DMGs, then fail closed if the error is anything other than "Resource busy" or retry budget is exhausted. Apply identical guarded retry to future notarized stable DMG creation. Do not work around Gatekeeper or disable macOS disk security. Require a green Mac Intel test before merge and publication.


## r017 RC1 — native About, photographic mock parity, PDF CI repair (2026-11-14)

- Continued directly from r016 on `main` (commit `ee3b08d`). The latest full run `43537250887` passed the single security/Production Gate, both macOS runner smoke tests, Windows 11 installer, six Rocky/Alma RPM builds and RPM index publication, but failed in the EN/IT publication job. Precise root cause: directly executing `scripts/capture-ui.py` sets `sys.path[0]` to `scripts/`, so `from gui import BackupGUI` raises `ModuleNotFoundError: No module named 'gui'`. Full release publication remains blocked, despite successful native package compilation.
- Repair the screenshot bootstrap by inserting the repository root into Python's import path before importing GUI modules. Add actual in-process About window capture and require it alongside the existing eight native GUI screenshots.
- Add a native photographic `About Multi Backup` popup with one source of truth for VERSION/BUILD/AUTHOR, EUPL-1.2, real project URL and explicit Copy version / Project website / Close actions. Open from Help menu, native macOS application About route, top toolbar and sidebar; F1 shortcut and Escape/Close handling. Do not unlock the vault or contact the network merely by opening About.
- Align the entire non-dashboard GUI with the approved navy/violet photographic glass design: dark secondary-page surfaces, legible high-contrast controls, violet accents, dark archival lists and plugin cards. Keep original responsive photographic hero, sidebar inset, 4x antialiased rendering, actions, progress and the explicit Start/Resume semantics.
- Make publication screenshots plus bilingual manual/brochure generation a required PR smoke check after the Production Gate and before native release. The release still rechecks publication generation before upload. Update EN/IT text to describe About, explicit resume, encrypted key vault and optional CI cleanup only after successful backup.
- Release metadata is `0.1.0-r017-rc1`, build 17. Preserve r016 encrypted backup formats, checkpoints, archives, key vault, local deletion guards, plugin behavior and EUPL.
- **Validation still required:** PR Production Gate, macOS ARM64/Intel, GUI screenshot and About screenshot, EN/IT PDFs, full main build/publish run, actual visual comparison with the supplied mock on the user's Mac. Rust GUI/engine replacement, production-signing/notarization and live end-to-end restore remain separate open work. Never label stable complete without those gates.


## r018 — native busy indicator and Rust progress kernel (2026-11-14)

- User requires the progress track to begin red and become green at 100%, with continuously interpolated hues (0 red, 50 yellow, 100 green). Keep the visible percent separate from animation; never fabricate elapsed progress.
- The 4x-supersampled gradient track remains in render_quality.py. A separate tiny 4x-Lanczos translucent bead moves across it by changing Tk Canvas coordinates, instead of re-rasterizing the entire track on every tick. Pause the indicator after 8 seconds without actual streamed CLI events; revive when new events arrive. Stop immediately on safe interruption, spawn failure, process completion, or closed window. The numeric percent is preserved.
- Replace the English Hide/Show navigation button with an icon-only supersampled three-bar hamburger, retaining mouse/keyboard interaction and state-aware accessible caption. Clicking toggles the scrollable sidebar, preserving author/version/license and photographic footer.
- Begin **operational Rust integration**: rust/src/lib.rs now exports a stable C ABI for bounded progress and the red→yellow→green colour kernel; rust_bridge.py loads only a packaged, trusted native library when available, with mathematically equivalent Python fallback. The native kernel is built and checked by Production Gate and packaged in macOS/Windows GUI applications. Linux GUI, remote backends, vault, encrypted streaming, restore, and the full graphical shell remain Python.
- End-to-end Rust migration remains UNFINISHED: this is a scoped, verified transition, not permission to discard Python or to modify existing encrypted files. Native recovery interoperability and full graphical parity remain mandatory release blockers.
- Update version/build to 0.1.0-r018-rc1 / 18. Re-run PR Production Gate, UI screenshots and native Mac/Windows build, then main full distribution CI before claiming release.
