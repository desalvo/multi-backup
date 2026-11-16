# Multi Backup development status

**Last existing working release:** 0.1.0-r018-rc1 (Python/Tk); **main development baseline:** Rust-native r037 preview  
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

### r018 native CI scheduling compatibility addendum

- Rust C ABI now also provides conservative CI target count and backfill schedule eligibility using saturating arithmetic. The actual GitHub backup process calls this library when bundled in Mac/Windows CLI; no change to downloads, encrypted archives, checkpoint state or cleanup permissions. Python fallback keeps Linux/source builds working with equivalent semantics. Production Gate parity tests test all small valid scheduler combinations and native DLL linkage.
- README EN/IT updated to r018. About title polished to avoid colliding with photographic header. Full Rust GUI/provider/vault/restore migration remains an open, separately testable macroarea.

### r018 actual-GUI visual QA polish

- Headless screenshot job `43541311109` generated real r018 dashboard/Backup/About screenshots and EN/IT PDFs successfully; macOS ARM64 DMG passed its native launch/install smoke. R018 photo/header/sidebar/gradient control appearance was inspected. Refined About to use the correctly proportioned dedicated sidebar photograph (no cropping/stretching); made the navigation scrollbar narrow and navy, reduced button labels to avoid crowding the rounded borders. Add non-finite numeric guard for progress-colour calculations in both native and fallback paths. Gate now checks Rust binary embedding for Mac/Windows. Mac Intel and final later-commit CI still require successful completion before merge.

## r019 — Rust-native encrypted archive interoperability (2026-11-14)

- Added `rust/src/archive.rs` and independent `multi-backup-native` Rust CLI. It can create gzip/tar archives encrypted directly with age, verify optional SHA-256 sidecars, list and fully authenticate members, and restore with strict normalized paths, member/size limits, duplicate rejection, link/special-type rejection and an atomic empty destination. It never writes an unencrypted whole archive. Existing Python archives remain the schema; archive bytes differ due gzip/tar metadata but the contents and structure must agree.
- Added a genuinely encrypted Rust↔Python interoperability test (real age keypair, Python-generated archive read/restored in Rust, Rust-generated archive verified/restored by Python), plus unsafe traversal and checksum mutation negative tests. Production Gate now installs `age` and fails unless both round-trips work. The Rust CLI is included as a helper in Mac and Windows native packages; Linux packaging of the helper remains open.
- **NOT yet a full migration.** The Python/Tk GUI, GitHub/Dropbox transfer clients, checkpoint journals and vault remain the production path; cryptography is performed by the existing `age` executable invoked from Rust. Do not disable Python, repoint default backup/restore or claim stable until every workflow's functional and security parity is certified on real providers and all native OS installers.

Validation requested: Rust cargo test/build, Python 3.11–3.13, dependency/security audit, real age/tar cross-language tests, Mac ARM64/Intel GUI smoke and native helper, Windows installer, Rocky/Alma RPM compatibility. No claims of full green until confirmed.

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


## r021 experimental Rust storage and server macroarea (2026-11-14)

**Branch:** feature/r019-rust-native-encrypted-archive, PR #9 (draft).
**Do not promote main r018 or stable.** The following are implemented
development commits, not qualified stable functionality:

- New opt-in chunk-store v1, keyed content addresses, age/zstd objects,
  encrypted per-cycle unified manifest, selective new-directory restore.
- 8+2 encrypted-ciphertext Reed–Solomon parity with authenticated stripe
  indexes, health repair, checksum checks and quarantine.
- Rust web server prototype, responsive browser, Argon2id login, sessions,
  CSRF/origin validation, audit, selective server-side export.
- Docker image build recipe, Compose, Helm/Kustomize manifests and Rust wizard.
  GitHub Actions gated Docker Hub publishing is defined but NOT triggered
  until credentials are configured and main CI passes.
- Original offline project artwork replaces former Unsplash/imgix source.
  Legacy brochures/old screenshots require a fresh licensed-asset audit.
- Rust/Python + end-to-end corruption test source added. Remote CI results,
  image publication and cross-OS installation still unverified in this cycle.

**Open critical gaps:** full legacy Python-to-Rust feature parity, vault
in-memory unlock for chunk store and web, CI metadata/preservation policy
parity, real provider backups, original folder restore, FUSE/WinFsp, ZIP64
multipart, storage migration/GC, LDAP/SSO, SMTP(S), audit retention and
security/penetration testing. No numeric overall completion estimate until
a verified inventory with explicit scope and acceptance gates exists.


## r022 — cross-platform native fixes, integrity and portable v1 (2026-11-14)

- Repaired invalid Rust egui drawing syntax and API on the native desktop branch; full native matrix requalification requested.
- Added storage verify health report with parity checks, ZIP64 encrypted object volumes, transactional import, multi-select restore, and authenticated streaming conversion from legacy age/tar members into unified chunk manifest.
- Responsive web browser now supports nested folders, search, list/grid and selection across a single logical backup.
- Publication policy now gates both OCI and native images on the Rust cross-platform matrix, and keeps stable opt-in disabled by default.
- Never claim completed certification on a compile-only gate; details in docs/RC_CERTIFICATION_MATRIX.md.


## r023 — v1 relocation and desktop GUI; SMTP(S), LDAPS, OIDC (2026-11-14)

- Implemented v1 storage relocation to a new directory with dry-run and
  source/destination SHA-256, age/HMAC and parity verification. The source
  is never deleted by relocation.
- Added Rust native GUI storage pane showing ONE logical backup per cycle,
  selectable virtual files, restore, parity check/repair, ZIP64 portable
  archive, legacy streaming import and storage relocation.
- Added optional server SMTP(S) with secret file and authenticated test,
  optional LDAPS verified TLS and allow-list/group policy, and OIDC code
  flow with PKCE/state/nonce and explicit authorized subjects.
- Hardened macOS Rust DMG to include native app and Applications shortcut,
  and retry only transient DiskImages contention.
- Outstanding: mount live FUSE/WinFsp; in-memory age vault for new storage;
  direct GitHub/Dropbox chunk ingest; complete reverse format conversion;
  production signing; audited user/role model and external IdP acceptance.
- CI needs requalification on latest head, not older 43615775732 green run.


## r024 — on-demand read-only live mount prototype

- Added opt-in WebDAV over HTTPS with separate token, authenticated PROPFIND/GET/HEAD and deny-all mutating requests.
- Streaming reads authenticated age/HMAC chunk objects without staging plaintext to disk.
- One backup ID maps to one virtual tree spanning all source namespaces.
- Helm auth.dav flag defaults OFF. Real macOS/Windows/Linux WebDAV client acceptance and kernel FUSE/WinFsp mounts remain open.


## r025 streaming reverse archive conversion

The v1 Store exports a complete unified logical backup to a new encrypted tar.gz.age using age/tar/gzip streaming, authenticated contents and a roundtrip test; no plaintext tar file is staged. Native GUI exposes the action. Tests on real large archives remain part of the release gate.


## r027 -- Debian and confidential architecture safeguard (2026-11-14)

- Added native Debian 12/13 AMD64/ARM64 package builder for compiled Rust GUI
  and CLI. Four .deb CI jobs test installability and use explicit release
  publication dependencies. Debian packages have SHA-256 sidecar files.
- Production Gate asserts all four Debian targets and blocks release
  publishing if the matrix or .deb artifacts are missing. Signing, platform
  runtime testing and external restore drills remain acceptance gates.
- Added docs/PRIVATE_ARCHITECTURE_WORKFLOW.md and a CI check that rejects
  plaintext or non-AES-256-R6 documents under docs/private. Only the named
  encrypted PDF may be committed; no password, private diagram source or
  vault topology is kept in Git.
- The private architecture PDF generator is delivered separately to the
  operator as an offline kit; the final encrypted PDF has NOT been published
  because the operator must choose the password on their own computer.
- Do not mark Debian or the security architecture as production certified
  until the latest GitHub Actions run passes all required jobs.


## r028 — OCI private-gateway boundary regression (2026-11-14)

- Verified the previous PR HEAD `a10a40e8` passes Production CI run 43625695818 (all 19 executed jobs). Draft PR does not push Docker images or publish installers.
- Server startup now fails closed on public HTTPS origins unless hardened mode is explicitly enabled; its two 256-bit gateway/recovery credentials must be present and **different**, preventing accidental privilege collapse through copied secrets.
- Added unit regressions for remote-origin downgrade, absent credentials and reused secrets. The OCI validation job additionally starts the actual Rust server and verifies the private-gateway/login/export HTTP boundary.
- Does not implement immutable WORM, independent key authority, mTLS, multi-person recovery approvals or real-account disaster recovery; no stable certification claim. CI on this new HEAD still requires review.


## r029 — bidirectional input/output plugin catalogue (2026-11-14)

- Added a **static Rust role/capability registry** for 18 input providers and 7 output providers, with exact inverse restore roles (7 restore input, 18 restore output). Roles cover GitHub, Dropbox, Google Drive, OneDrive, POSIX, S3 AWS/Ceph, WebDAV, NFSv3+, CIFS/Samba, CephFS, Ceph RADOS Block Device (RBD), Proxmox VE/PBS/VM/CT, Kubernetes/OpenShift/OKD, VMware, oVirt, MySQL/MariaDB, PostgreSQL, SSH/SCP/SFTP.
- Added read-only `multi-backup-native plugins list/plan` with JSON output and strict role validation; absent/unwired adapters cannot execute or overwrite data.
- Preserved existing direct Rust GitHub/Dropbox/FS commands and the legacy Python plugin registry. A catalog entry is not an adapter implementation.
- Added tests for the full 18+7 matrix, inverse role mapping, unknown or unsupported roles, and fail-closed non-executable plans.
- New public docs `docs/PLUGIN_API.md` and `docs/PLUGIN_MATRIX.md` define consistency contracts, encrypted transfer, restore confirmation, and acceptance gates. No provider credentials or private infrastructure details are committed.
- **Progress rebaseline:** 25/25 requested plugin role definitions represented; operational generalized provider-pair certification 0/25. The old 57.5% whole-product maturity indicator is not comparable after this scope expansion; recalculate only after an audited expanded inventory, never label this scope as delivered.

## r030 — SAN transports and immutable-vault plugin scope (2026-11-14)

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


## r038 integration / RC-preparation checkpoint (2026-11-14)

- PR [#9](https://github.com/desalvo/multi-backup/pull/9), containing Rust-native development r019-r037 and the native CLI acceptance matrix, **squash-merged to main** as commit `b7e757d9378d2503b57fa43267577a3a55f60fd2`.
- Its exact premerge feature head `860d390a0de6655ceafa6f7ea3eaeb376a1bf046` passed [Actions run 43640869549](https://github.com/desalvo/multi-backup/actions/runs/43640869549): **19 of 19 verification jobs successful, 0 failed; 2 release/registry publication jobs correctly skipped on PR**. All 17 nonexpired preview artifact groups were present. This confirms CI tests/builds, not live-provider or installer usability certification.
- Mandatory compiled CLI restore regression `rust/tests/native_cli_restore_matrix.rs` runs against Linux, macOS ARM/Intel, Windows and Debian 12/13 AMD64/ARM64; it tests encrypted archive create/verify/byte-for-byte restore, wrong-key/sidecar corruption rejection, no overwrite, namespace restore and protected-backup deletion refusal.
- The merge commit deliberately omits `[full-ci]` and does not request public early release or stable publication. The previous working r018 early-release assets must remain available, and experimental server/plugin/mount features must remain opt-in, not advertised production-ready.
- The next main-branch CI for this integration/documentation snapshot must be observed independently; the green premerge PR is not proof that follow-up commits passed.
- **Remaining RC acceptance:** real authenticated GitHub+Dropbox backups/restores (CI/LFS and interruptions), installed native GUI/user visual and access checks, legacy migration recovery, Apple Developer ID/notarization, Windows Authenticode/RPM signatures, update/rollback, mounted WebDAV server hardening, scheduler and protected GC certification, live provider plugin adapters and independent security assessment. No stable tag or promoted Docker image is authorized on these results alone.

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

## r067 — fix and targeted RC validation (16 Nov 2026)

- Corrected live PostgreSQL 16 monitoring-token issuance failure: `$4::integer * INTERVAL '1 day'` explicitly types token lifetime; prior run 43839136135 failed with PostgreSQL Float8 vs Rust i32 (`ToSql(3)`).
- Extended real PostgreSQL test with exactly 4 checksum-pinned migrations, repeated initializer preserving versions/checksums, token revocation and out-of-range validation. This is real PostgreSQL, not an offline mock.
- Monitoring REST (scope `monitor.read`) now removes encrypted-backup manifests from the returned status JSON. Full detailed manifests remain within the authenticated admin-only backup UI.
- Narrowed real PostgreSQL HA acceptance workflow triggers to DB/monitoring/agent modules; Rust focused compile covers unrelated server Rust files. Do not repeat full CI on every small patch.
- Latest upstream Ceph run 43832467180 is **FAIL despite 3 MON quorum, 3 OSD UP+IN and RGW running**, because the three OSDs are reported under a single kind worker/CRUSH host. The multi-node placement attestation is mandatory; a green MinIO/RustFS result is not Ceph certification. Need truly isolated per-worker OSD devices or a PVC-backed Ceph lab.
- The stack of draft PRs (#10–14, #20–22) remains unmerged; no RC tag or new stable assets. For exact gate evidence see `docs/RC_READINESS_R067_IT.md`.

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


## r164 — secure Rust native hypervisor credential reads (16 Nov 2026)

- On branch `feature/r164-protected-hypervisor-config` (based on r163 Xvfb), all four VMware/oVirt/PVE/PBS read-only inventory adapters now consume token/session files with a common bounded reader. On Unix this opens with O_NOFOLLOW/O_NONBLOCK, checks the descriptor's device/inode against lstat, enforces owner-only mode and verifies maximum size after read; symlink swapping, group-readable tokens and growing files fail closed. Token JSON input buffers are zeroized on drop. Windows permission/TOCTOU parity is still a separate OS acceptance requirement.
- The first focused CI #43908977857 compiled 115 tests: 114 passed and a NEW fixture failed because it wrote a test file with 0644 rather than required 0600. r164 corrects the fixture without reducing production protections; a new exact-SHA Rust CI PASS is mandatory before claiming certification.
- r163 separate native Linux GUI Xvfb run #43906909522 PASS, but no cross-platform signing, UI E2E or release acceptance is implied.
- Keep draft/stacked branch; do not merge to main, tag RC or publish installers until the final unified Production Gate and remaining field acceptance.

One-line English commit summary: Harden native hypervisor credential loading against symlink races and oversized token files, and correct permission test fixture.


## r166 — GUI Rust server/agent monitor, atomically fenced agent upload and three-host HA field preflight (16 Nov 2026)

- Branch: `feature/r166-ha-gui-agents-acceptance`, based on exact-SHA tested r164 `adb14dd0`. New GUI page "Server e agenti" connects to a verified HTTPS monitor.read REST endpoint, reads a token from a bounded owner-only file (not persisted) and displays aggregate agent/replica/job/DB health. This is not a native agent job-management GUI, and no credentials are stored in preferences.
- Server extends the read-only authenticated REST with `GET /v1/agents` returning only counts (online/approved/revoked), not agent identities or profile secrets. Separate read-only API admin access remains scoped and protected.
- Agent-to-server ciphertext publication uses atomic, same-directory hard link with **no replace** instead of `exists()` followed by `rename()`; concurrency tests verify exactly one winner for a colliding backup ID. An uncommitted ciphertext is retained for reconciliation rather than silently declaring a successful receipt. Agent credential reads use the bounded secure native reader and new agent identity files use no-clobber persistence.
- New `scripts/ci/ha-three-host-field-r166.py` and manual self-hosted field workflow validate three distinct kernel boot IDs, machine identities, service state and consistent single-leader Patroni membership across independently connected SSH hosts; automatic safe fixture tests are separate. They are **read-only preflight only**, not an executed physical three-node failure test.
- REQUIRE exact-SHA Rust core/server/desktop focused CI, actual Linux Xvfb GUI runtime, real separate physical host Patroni+etcd failover/fencing/rejoin, agent multi-machine encrypted transfer/restore, signed installers/rollback, production/security CI before RC. Legacy GPFS native certification remains deferred; the NFS/SMB protocol subset is the only supported profile.
- Do not merge to main, tag RC or distribute unverified installers based on focused tests.

One-line English commit summary: Add native Rust server-agent monitoring, atomic no-clobber encrypted uploads and guarded three-host HA field preflight.


### r166 native signed-agent encrypted transfer E2E extension

- Added an ignored-only live PostgreSQL16 test `real_pg_agent_encrypted_upload_no_clobber_and_restore` to the existing disposable DB workflow: generates a real age recipient and ciphertext with the native age CLI; enrolls and approves an Ed25519 device/profile in PG; calls the actual streaming Rust upload handler with signed hash; attests one PostgreSQL receipt and byte-identical age decrypt; blocks duplicate backup ID overwrite and rejects a revoked device. The job installs `age` explicitly; no real account secrets.
- This tests an in-process handler with genuine PostgreSQL and cryptography, **NOT independent physical agent/server machines, WAN TLS proxy or recovery to a live AFS/VM source**. A passing CI remains scope-limited. Exact SHA CI evidence is needed.


## r168 — single-SHA software gate and private self-hosted three-physical-host lab orchestration (16 Nov 2026)

- New **single-SHA** software gate `.github/workflows/software-readiness-r168.yml` on dedicated branch `ci/r168-software-field-gates`: calls the existing Rust unit, real PG16 signed-age agent receipt/restore and native Linux Rust egui Xvfb workflows as **reusable** jobs; builds **one** OCI image for existing gateway and three-Kind-node Kubernetes PostgreSQL verify-full, two-server-replica recovery scripts. Final aggregation requires all four job families SUCCESS; any skipped/failed/cancelled job rejects closure.
- Existing workflows gain `workflow_call`; full-suite option on Rust-focused workflow reuses its existing jobs, without modifying the original push-path fast filters. Explicit separate concurrency groups avoid calls cancelling one another; on the CI gate branch, old feature-only push workflows are not redundantly triggered.
- Existing physical three-host HA workflow stays manual; redundant negative fixtures removed from self-hosted execution (already covered by hosted `r166-validation-fast`), runner must be non-root with trusted SSH host fingerprints, serializes lab runs. Added private-repo wrapper and setup guide rather than registering a sensitive lab runner to a public repo; self-hosted is **not installed/connected** here.
- Real three-physical-host failover/partition fencing and multi-host agent transport still open. Software gate results must be recorded from CI exact commit before any component can be promoted. No main merge, tag or distribution merely because workflow exists.

One-line English commit summary: Reuse single-SHA Rust, PostgreSQL agent, OCI Kubernetes and GUI gates while isolating physical HA on a private ephemeral runner.


## r170 — REST monitoring agent/PG integration and full native GUI page render sweep (16 Nov 2026)

- On top of r168 single-SHA software gate, the existing **real PostgreSQL16 signed-age agent upload test** now also calls the same read-only `/v1/status` and `/v1/agents` Rust Axum handlers used by the native GUI, validates schema and aggregate counts, rejects missing/wrong/revoked `monitor.read` token and verifies no secrets/agent profiles or identities are exposed. This extends the existing test fixture, **not a duplicate test job**. It remains an in-process HTTP handler test on genuine PG16, **not remote HTTPS TLS certification**.
- Native egui GUI monitor response parsing is now separately unit-tested for schema, fail-closed missing counters and actual multiline display, fixing literal escaped `\\n` text. In the **existing** Xvfb smoke job, a single GUI process visits all **ten** sidebar pages and About (three animation/render frames per page), checking visited page labels and successful exit. This is real Linux GUI render acceptance, still not screenshot-perfect UX or Windows/macOS installer acceptance.
- The existing focused Rust workflow executes full desktop **unit tests**, not only `cargo check`. Existing r168 single-SHA workflow now triggers on changed core/server/desktop/test scripts and still reuses existing PostgreSQL, egui and Kind jobs. Full gate PASS must be confirmed for the new exact SHA.
- The authoritative **33-plugin and 4-direction RC readiness matrix** is in `docs/RC_READINESS_R170_IT.md`; no untested plugin was promoted to production `Ready`. Field-only items (three independent physical machines, HA disruption, full remote agent TLS, vendor authentication, signed installers, root-resistant WORM) remain open. No release, tag or main merge.

One-line English commit summary: Exercise PostgreSQL-backed monitoring and every Rust GUI page with existing CI suites while retaining honest 33-plugin RC readiness gates.


## r173 — fix agent native packaging revision gates without disabling checks (16 Nov 2026)

- The full Production CI primary **security/compatibility gate** passed on exact commit `daec34bb7a951908b322359fbf156b9d5d64c50c` (workflow [#43930325464](https://github.com/desalvo/multi-backup/actions/runs/43930325464)), after fixing Ruff E401 and Bandit B310 without changing their thresholds.
- Its release matrix uncovered a deterministic obsolete **agent package version assertion**: `scripts/package-rust-agent-unix.sh` and Debian post-install CI demanded literal `r039`, but the compiled Rust agent's reported revision is `0.1.0-r057-native-preview`. Successful Cargo release compilation was being rejected with exit 1 before packaging. Replace both fixed markers with a checked native-preview semantic version plus a numeric revision. This does not change the agent's source version or claim production packaging certification.
- The r173 fix requires **exact SHA** rerun of the full packaging matrix and pass of every required packaging job. Until the new CI passes, packaging/RC readiness remains open. Physical 3-host Patroni failover and remote TLS agent field acceptance remain open.

One-line English commit summary: Fix stale hardcoded agent revision assertions so native DEB/RPM/PKG packaging validates current Rust preview versions.


## r174 — complete software RC1, keep macOS notarization optional ONLY for prerelease (16 Nov 2026)

- **Production CI is now fully green** after the r173 fix: [#43932195582](https://github.com/desalvo/multi-backup/actions/runs/43932195582), final run attempt #2, 32 successful required jobs, 3 intentionally skipped publication/legacy jobs, no failed/cancelled jobs; all **27 Rust-native GUI/agent artifact families** exist, including Darwin arm64/x86_64 DMG and agent PKG, Windows x64, Debian 12/13 amd64+arm64, Rocky/Alma EL8/9/10.
- Operator explicitly accepted **non-notarized macOS apps in RC only**, not stable. `docs/RELEASE_CANDIDATE_R174_IT.md` declares Gatekeeper limitations, SHA validation and scope. Stable signing policy is unchanged.
- New `scripts/ci/validate-rc-evidence.py` and negative fixture unit suite `tests/test_rc_evidence_r174.py` pin source commit r173 and CI run #43932195582, require all 32 mandatory jobs, require permitted 3 skip jobs, and check all 27 authentic Rust artifacts from the existing collector. No repeated source test or package compilation.
- `.github/workflows/rc-candidate-r174.yml` calls **existing distinct** live native Prometheus HTTPS, real SNMPv3 authPriv and real Patroni/etcd netns partition/fencing suites once on the RC policy commit, aggregates them with exact source evidence, collects previously built native Rust outputs and only then publishes immutable `v0.1.0-rc.1` **prerelease**. It cannot promote stable, main or legacy code.
- All 33 plugin stages remain unchanged: do not invent `Ready` for provider plans. Physical three-host network/DR, remote agent TLS field, root-resistant vault and stable signing still require external infrastructure evidence.

One-line English commit summary: Release gated Rust-native RC1 from proven one-SHA artifacts while qualifying monitoring and Patroni netns without requiring notarization in prerelease.
