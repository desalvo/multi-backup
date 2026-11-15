# Multi Backup development status

**Last published working release:** 0.1.0-r018-rc1 (Python/Tk); **main baseline:** Rust r037; **active branch:** Rust r038 functional plugin executor  
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


## r038 — Rust-first development decision and plugin runtime (2026-11-14)

- By explicit user decision, historical Python archive migration/byte parity is **optional** and no longer a blocking RC gate. Existing archives, keys and converters are retained, never silently deleted. Functional backup/restore and user-visible behavior remain mandatory.
- Versioned native GUI/CLI previews as r038 and replaced Python archive-wire compatibility in Production Gate with real Rust binary plugin restore acceptance; Mac, Windows and Debian package jobs also exercise the test.
- New `rust/src/plugin_executor.rs` implements executable POSIX→POSIX age-encrypted chunk backup and fresh-directory verified restore, including source/store containment checks, refusal to archive private identity files, integrity verification and no overwrite.
- Four additional **Linux-only read-only mounted source previews**: NFSv3/v4, SMB/CIFS, SSHFS/FUSE and GPFS CES NFS/SMB. Exact read-only mount evidence and an operator snapshot-attestation flag are required; neither is proof of vendor snapshot/ACL consistency, and no remote target writes are enabled.
- Rust generic plugin preview backup pairs: **5**; certified generic backup pairs: **0/682**. Static catalog: 32 plugins/53 backup roles unchanged. Dedicated Rust GitHub and Dropbox provider paths still need common-plugin integration and real account E2E.
- **Still open:** PR #10 full green CI, native GUI settings parity, GitHub/Dropbox live recovery and resume, real mounted filesystem recovery, protected garbage collection/scheduler, full OS installed-app qualification, signing and update/rollback. Do not claim final migration or stable release yet.


## r038 — chain-aware incremental deletion and garbage collection (2026-11-14)

- Created native full/incremental snapshot mode with full encrypted reference
  manifests and immutable deduplicated chunks, preserving a linked
  `parent_id` history. Rust CLI/GUI allow selecting total or incremental.
- Added `backup_chain::details` with complete/increment history and exact
  uncompressed/zstd compressed/zstd deduplicated byte metrics, file counts,
  modifications and deletions. Chain verification defaults to all ancestors.
- Added durable Rust scheduler `schedule run/daemon` with per-job backup
  type or verification, independent cadence and optional no-chain verify.
  Still needs configuration UI/persistence and crash resilience certification.
- Direct child reparenting now occurs as the default confirmed deletion:
  deleting an intermediate increment reconnects children to its predecessor;
  deleting a full promotes direct incremental descendants to full. Deletion
  verifies descendants, refuses protected or corrupt ones, changes only
  encrypted manifests and preserves ALL ciphertext objects until GC.
- Added separate locked POSIX ciphertext GC: explicit dry-run and confirmed
  execution, minimum 24-hour orphan grace (default 48), complete live
  manifest/chain/parity validation, deletion of wholly dead parity groups and
  unreferenced chunks only. Shared live/dead parity stripes are left intact;
  **crash-safe parity repacking remains mandatory for maximum reclamation**.
- Added `garbage_collection_roundtrip.rs` with physical deletion/grace,
  shared stripe safety and real-age recovery tests; included in macOS,
  Windows and Debian packaging acceptance. All new CI still to be verified.
- Account-scoped restore planning now records independent source and target
  identities. GitHub target owner must match the explicit target account;
  Dropbox checks the write-token account through its account endpoint before
  cross-account upload. Provider-wide selection, authentication and
  live multi-account E2E remain incomplete.
- PR #10 stays draft until passing new full CI and external restore tests.
  Historical Python archive migration is optional, not a release gate.

## r038 — documentation reconciliation and operational manuals (2026-11-14)

- Added bilingual Rust developer-preview operator runbooks at `docs/R038_OPERATIONS_IT.md` and `docs/R038_OPERATIONS_EN.md`, with exact CLI recipes for full/incremental backup, file/count/byte metrics, default whole-chain verification, safe selective restore, provider cross-account authorization, manifest purge/rebase, manual GC with grace/parity safeguards and schedule JSON.
- Reconciled earlier inconsistent current-tense statements in `CHUNK_STORAGE_V1.md`, `BACKUP_PROTECTION.md`, `MAINTENANCE_POLICY.md`, `PLUGIN_API.md`, `RELEASE_CHANNELS.md`, `SECURITY.md` and `RUST_PORT_INVENTORY.md`. Historical milestones remain explicit rather than being erased.
- Expanded bilingual PDF manual source with four explicitly labelled r038 experimental chapters per language. PDF output still must be regenerated and checked by CI; existing PDF binary may remain historic until regenerated.
- Release policy unchanged: r018 is last distributed; Rust r038 remains a draft PR until current-head CI, real recovery tests, installer signoff and security tests pass. No merge, stable tag, Docker publication or user-data GC performed by documentation changes.

## r038 — Audit documentazione e parità automatica (14 novembre 2026)

- Documentazione: nuovi manuali operativi Rust IT/EN, inventario delle funzionalità, indice documentale e controllo di coerenza CI. Aggiornato generatore PDF bilingue con sezioni Rust e avvertenze sulla GUI Python/Tk storica.
- Implementazione: repack automatico delle stripe 8+2 miste durante GC confermata, nuovi shard age verificati, indice successore committato ultimo, mantenimento del predecessore fino a grace 48h. Previste riprese da preparazioni non committate.
- Test: roundtrip del purge, GC, protezione, repack, interruzione prima del commit; CI corrente in attesa di certificazione. Restano fault-injection di produzione, multiwriter, restore account reali e aggiornamenti firmati. Nessun merge o rilascio.

## r039 — AFS, agent parallelism and Kubernetes (2026-11-15)

- Developed OpenAFS volume-only vos dump planning (local, pinned SSH and managed agent transport) with validated arguments; separately approved keytab/localauth privileged execution and live vos restore still pending.
- Added native multi-backup-agent Rust CLI with push/pull, Ed25519 device key, linked user and bearer token, age ciphertext spool, server approval, revocation, nonce-based anti-replay, encrypted upload receipts and multi-volume bounded parallelism.
- Added ExecutionLimits for 1–256 concurrent jobs, 1–64 threads/job, per-stream bytes/s, CPU utilization, RAM utilization and free RAM minimum; server+agent restrictions intersect on both push and pull. Resource probes are cooperative; hard cgroups/Job Objects and remote SSH-host load enforcement remain pending.
- Added PostgreSQL TLS verify-full shared queue, storage visibility shared/node_local, replica heartbeat, row-level atomic claims, lease/renew/finish, optional filesystem worker, shared browser sessions, readiness probe and centralized agent registry.
- Added independent agent installer matrix: Mac arm64/x86_64 PKG, Windows x64 EXE, Debian 12/13 amd64/arm64 DEB, Rocky/Alma 8/9/10 x86_64 RPM. No auto-start.
- Updated IT/EN agent and cluster runbooks; r039 draft PR #11 based on r038 draft PR #10. CI and live field testing are required; no RC or tag created. Open blockers: physical write fencing after lease expiry, OIDC pending state, remote restore routing, single-volume AFS restore, hard limits, real-agent and real-K8s tests.

## r039 cluster GUI and documentation CI review — 2026-11-15

- Added read-only "Cluster, agenti e backup paralleli" section to Rust server HTML, JS and CSS, showing the same PostgreSQL registry on any replica: health/capacity, registered shared vs node_local backends, recent job assignments/status and approved/offline/revoked agents. Admin API access controls inherited, no raw HTML injection of names.
- Added bilingual r039 AFS+agent and cluster manuals (AGENTS_AFS_R039_{IT,EN}, CLUSTER_ORCHESTRATION_R039_{IT,EN}), Kotlin-free K8s three-replica preview YAML, and updates to README, plugin, security, operations and RC status. Documentation gate validates manuals, source contracts, CI installer matrices, local links and Markdown fences.
- Production Gate now refuses missing r039 source, release-relevant agent package matrices, resource policy tests, signed agent protocol and storage affinity tests. Last known green full r039 feature-head CI is older SHA 97f3786e (run 43655180432); new commits still require their own CI and real Kubernetes/AFS field acceptance.
- Limitations explicitly preserved: no atomic physical-write fencing after distributed lease loss; only filesystem server_local worker; no guaranteed remote SSH CPU/RAM cap, no hard OS cgroup/Job Object quotas, no generic cloud object writer, no AFS live restore; cluster web UI administration forms and per-node local file proxy pending. PR #11 remains draft.

## r039 hardening update — 2026-11-15

- Upgraded cluster job safety: PostgreSQL capacity claims serialize on replica row; worker checks live token during every file/chunk and forces uncached check immediately before encrypted manifest publication; expired writers are quarantined as needs_reconciliation and never reclaimed automatically until independently verified.
- Added separate POST /api/cluster/jobs/recover administration workflow gated by CSRF, private gateway, independent recovery secret, original replica offline status, exact attempt number, writer_terminated and encrypted_store_reconciled operator attestations. Crash-recovery of in-flight jobs is intentionally fail-closed, not fully automatic until end-to-end physical manifest fencing is certified.
- Added mandatory per-agent job profile approval via POST /api/agents/admin/profile, keyed to agent ID, profile ID, SHA256 of the exact reviewed local JSON, and provider. The agent preview prints command plan and SHA256. Enqueue pull and ciphertext upload reject missing/changed profile, including before accepting large request bodies; transactional receipt checks again.
- Bounded agent concurrency across CLI processes via private filesystem slot locks (released when jobs/processes end). Added focused Rust tests for guard denial, invalid profile, worker slots and cluster schema, and added agent bin tests to Production Gate.
- Updated IT/EN AFS and cluster runbooks, certification matrix, dev status and handoff. Prior focused test runs green through ef44c1ab; full current r039 head CI must be rechecked. No RC, no stable tag, no Docker publication.

## r039 latest Kubernetes and CI hardening (2026-11-15)

- Reworked development Kubernetes example: Secret material copied by a short-lived initializer to UID 10001 private 0600 memory-backed files, avoiding root-owned 0400 Secret access failures. Enabled required hardened security mode with independent 64-hex private-gate and offline-recovery credentials, preserving the separate HTTPS agent proxy attestation secret and protected age identity.
- Added HPA resource-based horizontal scaling 2–10 replicas and PodDisruptionBudget with existing shared PostgreSQL control plane, storage RWX, readiness checks and startup-disabled backup executor. The control plane may autoscale; live writer failover remains quarantined until physical fencing is proven.
- Production Gate now parses and checks actual YAML Deployment, Service, RWX PVC, HPA/PDB, initContainer and mandatory hardened settings. Added IT/EN operational instructions and certification blockers.
- The last baseline green full CI before these changes was run 43655428169 on b3fe87d9 (30/30 jobs). Later focused Rust validation was green on intermediate r039 heads; **new current head still needs its own full CI and real field acceptance**. PR #11 remains draft; no merge, stable release or Docker push.

## r039 PostgreSQL TLS correction — 2026-11-15

- Fixed a significant HA startup incompatibility: postgres Rust 0.19 does not support sslmode=verify-full parsing directly. Implemented strict URI query parsing with duplicate/downgrade rejection, secure driver normalization to sslmode=require and explicit native-tls TLS>=1.2 verified certificate/hostname. Optional MULTI_BACKUP_CLUSTER_TLS_CA_FILE supports a trusted private CA. Added negative-case unit tests.
- Updated Kubernetes preview with private multi-backup-db-ca Secret copy; Production Gate now checks CA env and TLS safe defaults. IT/EN operations and security documentation updated. Exact-head full CI and actual CA/cert failure tests required before RC.

## r039+ Rust native plugin executor progress — 2026-11-15

- 8 executable preview filesystem backup sources: posix, nfs, smb, sshfs, gpfs CES, cephfs, glusterfs, lustre -> age/zstd protected POSIX chunks. Linux remote mounts must be exact RO mounts and operators attest snapshots; mount type is not snapshot/ACL proof.
- 6 executable preview verified filesystem restores: POSIX store -> NEW directory on posix, nfs, sshfs, cephfs, glusterfs or lustre, with exact RW mount gating. GPFS CES output remains unimplemented.
- Added Store::backup_stream(Read,finalizer): direct deduplicated encrypted chunk streaming, no raw source plaintext staging; a failing upstream producer prevents manifest publication and leaves only encrypted GC-eligible orphan chunks.
- Added OpenAFS dedicated vos dump full/incremental streaming local/SSH-agentless source and explicit new-volume full/incremental chain restore with -overwrite abort on full; authentication uses valid local token. CLI: plugins afs-backup, afs-restore-plan, afs-restore-new. Full-chain age/parity verification and confirmation required.
- New rust/tests/afs_stream_roundtrip.rs invokes compiled native Rust CLI, real age-keygen and FAKE vos child, covering encrypted full+incremental streams, ordered restore, wrong confirmation and producer failure. Mandatory Production Gate now executes this test. Native plugin planner preview flags synchronized with executable pairs.
- Documentation updated in docs/PLUGIN_EXECUTORS_R039_IT.md and EN; all features are PREVIEW not certified. Open blockers: real AFS site/keytab/localauth/VLDB/ACL testing; true CephFS/Gluster/Lustre mounted metadata/failover tests; generic cloud/DB/VM provider adapters and distinct source/target credentials; signed packages and HA physical write fencing. No RC.

## r039+ remote AFS new-volume restore (2026-11-15)

Native Rust RestoreNewJob now optionally accepts Execution::SshAgentless with pinned host key/identity and authorized remote AFS token, alongside local token restore. Full+incremental data streams directly from verified age chunks to SSH stdin without plaintext dump staging. The exact typed vos restore command is quote-escaped and executed with -F none, strict host-key check, no proxy/forwarding/password and an explicit new-volume confirmation. Unit/fake SSH+vos integration tests updated. Full real cell/SSH failure drills, keytab, localauth and approved-agent remote restore are NOT yet certified.

## r039 — cross-process host upload throttling (2026-11-15)

- Added optional agent-local `max_host_transfer_bytes_per_sec` to throttle
  aggregate HTTPS uploads across independent Rust agent processes, enforced
  via an owner-private locked one-second byte accounting file.
- Host lock is fail-closed for malformed counters, symlinks, bad permissions
  or clock rollback; legacy configuration without the optional field is
  unchanged. Added two-reader, state-corruption and symlink regressions.
- This is a **local agent upload quota** only. Source dumps, multiple nodes,
  server-originated transfers, Kubernetes global throughput and OS-enforced
  CPU/RAM/network controls are still not complete or qualified.
- New exact-head PR #11 full CI and actual multi-process throughput tests
  are required before accepting this behavior as production ready.


## r039 — OpenAFS virtual-cell test
- Simulated `vos` full/incremental and fresh-volume restore locally (131082 + 1500 bytes), including rejection of existing target and partial dump. The standalone fake command passed six checks.
- Added integration test `afs_virtual_cell_refuses_overwrite_and_partial_dump` that runs compiled Rust CLI, real age/zstd encrypted backup and verify, ordered chain restore, no-overwrite and partial-producer rejection. Exact-head CI must confirm the Rust test.
- GitHub run 43668284081 previously failed three test fixtures because `tempdir()` did not enforce 0700; fixed fixtures without weakening runtime security. Real OpenAFS cell/Kerberos/ACL/SSH acceptance remains required.

## r039 native database logical adapters

Added native Rust PostgreSQL and MariaDB full logical dump and restore-new stream adapters. Dump stdout is stored directly in encrypted age/zstd chunks; restore writes to a newly created target database, never drops existing data. Files holding credentials require 0600. Remote targets require private TLS CA files. **CI and E2E checks are not yet complete**. WAL/binlog PITR and physical incremental backups remain separate work.

## r039 database live acceptance extension

Native database plugin IDs are now Preview for backup input and restore output only, but generic pairing remains disabled. Live PostgreSQL 16 and MariaDB 11.4 1000-row restore tests passed in run 43670330802 (2/2). Expanded tests compare every ordered source/restored record and reject wrong credentials without publishing a manifest. New exact-head CI still required. WAL/binlog PITR and physical incremental support are NOT complete.

## r039 new CI — authentic AFS and live database recovery
- Published dedicated container CI for PostgreSQL 16/MariaDB 11.4. Run 43670330802 tested native Rust CLI with real SQL engine processes; 2/2 passed (each restored 1000 rows into NEW databases, rejected second restore).
- Added private encrypted logical full-dump plugin commands for both, with remote TLS CA and secret-file requirements. Later SHA expands row-by-row checks and denied wrong-password publication; verify before marking latest CI pass.
- Published authentic OpenAFS+MIT Kerberos provisioning workflow plus strict live lab acceptance script. Official Ansible local_install installs KDC/AFS servers/client; test requires actual Kerberos tickets and AFS tokens, VLDB calls, vos full/incremental, isolated new-volume restore and bytewise comparison. Workflow running/pending field outcome; live AFS claim remains BLOCKED until full successful job evidence.
- Remain open: database WAL/binlog, PITR, physical full/incremental source snapshot, failover and multi-version upgrades; AFS production token/keytab, ACL, host power-loss tests and agent deployment.

## r039 PostgreSQL WAL and MariaDB binlog real PITR CI
- Created workflow `.github/workflows/database-pitr-integration.yml` and dedicated scripts for two real isolated database PITR restorations. Each must restore only committed transactions up to the target; missing post-target transactions are essential acceptance.
- MariaDB 11.4 binlog position PITR PASS on run 43672237760: Rust FULL and age-encrypted binlog, native replay, 30 records, ID sum 465; ID 999 excluded. PostgreSQL physical base + WAL recovery CI has initial HBA issue and corrected fixture; await exact test evidence.
- PostgreSQL physical staging and MariaDB plaintext binlog replay staging exist **inside disposable CI lab**; full unattended production PITR and log retention still incomplete.
- IT/EN instructions in `docs/DATABASE_PITR_R039_IT.md` and `docs/DATABASE_PITR_R039_EN.md`.

## r039 Galera 4 native Rust and real 3-node CI (2026-11-15)
- Galera 4 / MariaDB 11.4 genuine 3-node Docker test passed run 43673606744 (source 37e5de39), with real Primary/Synced membership, cross-node transactional writes, one-node failure and quorum2, IST/SST rejoin3, native age/zstd FULL at node2, cross-node binlog PITR and verified isolated restore.
- Added optional `galera_expected_nodes` input-only Rust profile guard: requires 2..64 nodes, Primary/Synced/wsrep_ready pre- and post-dump, and fails closed with no new manifest when 2/3 nodes are present against a 3-member policy. Restoring directly into a Galera cluster is intentionally denied.
- Verified binlog PITR: write 20 events on nodes1/3, capture node2 replicated ROW binlog with wsrep_gtid_mode/log_slave_updates; recovery exactly 60 rows, SUM(ids)=1830, excludes ID999, and cannot overwrite an existing database. Uses manually orchestrated replay, not a full native continuous PITR plugin.
- New docs: `docs/DATABASE_GALERA_R039_IT.md` and EN. This proves same-host container integration, not production-grade network partitions, full-quorum loss, multi-source GTID, physical SST security, automated retention, crash-consistency or hard fencing.

## r039 POSIX and SSH/SCP/SFTP/SSHFS real acceptance update
- Added POSIX E2E workflow and byte-for-byte safety tests; first run found a **real** broken RS parity repair with differently sized ciphertext; fixed the ciphertext zero-padding and proven PASS at run 43678452447 (full/incremental, wrong key, symlink/FIFO, selected restore, parity repair).
- Added real OpenSSH server CI for SSH, both SCP transports and SFTP, strict pinned host and unauthorized key refusal. SSHFS path exercises real FUSE mount RO, encryption to age store and create-only RW SSHFS recovery; cannot certify without actual FUSE CI pass. The catalog SSH direct executor remains Planned.
- Real OpenAFS CI switched to manually approved disposable self-hosted runner and exact SHA input. Requires real Linux OpenAFS DKMS/kernel compatibility and sudo, rather than unsupported hosted kernel. **Still not field certified.** Documentation: docs/ACCEPTANCE_POSIX_SSH_AFS_R039_IT.md.


## r039 dedicated AFS runner Debian 12 platform update (15 Nov 2026)
- Debian 12 Bookworm x86_64 with stock Linux 6.1.x-amd64 is now the **preferred** disposable lab VM. Retained Ubuntu 22.04 GA 5.15.x-generic compatibility. The bootstrap checks exact OS/kernel, selects libicu72 on Debian vs libicu70 Ubuntu, tests matching headers, installs distro OpenAFS DKMS, and requires actual modprobe before allowing an ephemeral GitHub runner. No unsupported kernel or production data allowed.
- Added policy-gate lint for Debian 12; see docs/AFS_RUNNER_R039_IT.md and main-only approved exact-SHA dispatcher. **Still not field certified** until true Kerberos/VLDB/volserver/new volume restoration succeeds on user's isolated VM. Other agent/HA/installers remain separate gates.


## CI 43730251286 and reusable Proxmox lab — 15 November 2026

The real Debian 12 job completed Kerberos and the OpenAFS server/client mount, then Ansible failed creating root.afs/root.cell/test with 'Unable to locate viceetcdir directory'. Cause: upstream SHA-pinned collection infers viceetcdir from the cacheinfo manual page, missing in Debian 12 facts. New scripts/patch-openafs-ansible-debian-facts.py adds /etc/openafs only when a REAL /etc/openafs/cacheinfo exists. All volume and vos operations remain real, not simulated.

Keep Debian 12 VM, OpenAFS, Kerberos and Rust on the ISOLATED Proxmox network. GitHub runner remains per-job ephemeral; register fresh each time with a NEW short-lived GitHub token. The registration script reuses verified official runner archive after checking SHA256, uses exclusive flock, creates and removes each runner session/workspace, preserving the VM. The trusted workflow uses a per-job active lease and cleanup; after an abrupt termination inspect the VM and logs before manual lease recovery. Do not grant it production network or administrative credentials.

Previous afs-run-used marker is retired; current job guard uses afs-active. Restore volume name uses GitHub run ID/attempt to avoid overwriting old test volumes. To retry from mb-afs-runner after verifying no runner process is active, git pull --ff-only in ~/multi-backup and run bash scripts/register-openafs-runner-once.sh. New VM bootstrap is NOT required. Still not AFS-certified until real full/incremental/new volume byte comparison is green.


## CI 43731741639: true root cause of `viceetcdir` (15 Nov 2026)

The **same** `openafs_volume: Unable to locate viceetcdir directory` remained after fixing Debian install path discovery. Log showed `PASS: real OpenAFS viceetcdir fact already present` while running that check under `sudo`. The pinned upstream volume task is **unprivileged**; facts were created under `become: true` on a runner started with `umask 077`. Root can therefore read the file while the unprivileged volume module cannot; its broad `except Exception` masks EACCES as a missing directory.

The updated pinned upstream `local_install.yaml` is patched at the **boundary between client checkpoint 3 and volume checkpoint 4**: it sets `/etc/ansible/facts.d/openafs.fact` to root-owned, runner-group readable **0640** (not world-readable) and runs a non-root Python assertion checking the fact JSON and actual AFS `cacheinfo` path. The patch refuses unknown upstream playbooks and is tested against the exact upstream commit `11fa2a5b` in static CI.

Preserve the Debian 12 Proxmox VM, KDC, OpenAFS client and kernel, and shared runner binary archive cache. Still register a new ephemeral runner per CI job; the main workflow remains owner-only, SHA-pinned, with `afs-active` lease cleanup. Do not claim a working real volume recovery or certification until the field run reaches green.


## CI 43732486690: EACCES on OpenAFS fact parent directories (15 November 2026)

The actual Linux runner/Ansible acceptance failed **at the new diagnostic**, not while building OpenAFS: PermissionError [Errno 13] reading '/etc/ansible/facts.d/openafs.fact' as mb-afs-runner, despite the file being group-readable 0640. The registering process has restrictive umask 077; one or both ancestor directories, '/etc/ansible' and '/etc/ansible/facts.d', may therefore be root-only and prevent path traversal. The earlier upstream openafs_volume module was hiding EACCES behind 'Unable to locate viceetcdir directory'.

The exact upstream `local_install.yaml` patch is now more complete: after checkpoint 3, make both fact directories root-owned/group mb-afs-runner, mode **0710** (group execute/traverse but no listing; other denied); set '/etc/ansible/facts.d/openafs.fact' to root:mb-afs-runner **0640**; also make real '/etc/openafs/cacheinfo' root:mb-afs-runner **0640**, and require a real non-root open/read before touching volumes. No keytab permissions changed, no world read access. The static CI checks the patched pinned playbook. Debian VM stays available with an ephemeral GitHub runner session for every CI job. A genuine AFS full/incremental/Rust recovery PASS is still pending.


## 2026-11-15: Real AFS run 43744237757 reached Rust test; RO mountpoint correction

- The genuine Debian 12 run [43744237757](https://github.com/desalvo/multi-backup/actions/runs/43744237757) PASSED SHA/permissions, real Kerberos KDC and AFS token, VLDB/fileserver, root.afs/root.cell/test volumes, and Rust release build. FAILED while writing seed data: `/afs/example.com/test/mb-backup-data.txt: Read-only file system`. The tested AFS volume includes RO and RW copies, and the un-dotted `/afs/example.com` path resolves via RO root.cell/test. This is a test fixture mountpoint error, NOT kernel/DKMS failure.
- The real acceptance script now establishes a unique source mountpoint under `/afs/.example.com` with explicit `fs mkmount -rw` targeting real volume `test`, logs `fs lsmount`, and writes/compares against that mount. The newly restored volume is also mounted with `-rw` under the cell RW root. Mountpoint collisions refuse overwrite. The temporary source mountpoint is removed on exit; real test contents and restored volumes remain available for inspection.
- The isolated Proxmox Debian 12 VM remains reusable, with a fresh ephemeral GitHub runner and run lease; full+incremental encrypted backup and restore remain **NOT YET field-certified** until actual live PASS.


## Real OpenAFS CI #43745205761: authenticated admin missing i/a on RW root.cell

- CI #43745205761 passed real KDC+admin rxkad token, VLDB/volserver, Ansible volumes and Rust release; it FAILED at `fs mkmount -rw /afs/.example.com/mb-source-<run>-<attempt>`: `You don't have the required access rights`. This is a genuine OpenAFS ACL problem. The previous failed RO-path write has already been corrected.
- `fs mkmount` explicitly requires BOTH `i` (insert) and `a` (administer) permissions on the ACL of the parent directory. The Ansible local install config currently grants broad read ACL on root.cell; while the admin has implicit a via system:administrators, it may not have insert. Within THIS isolated `example.com` test cell ONLY, the acceptance script prints parent ACL and `fs getcalleraccess`, adds `system:administrators all` to `/afs/.example.com` (preserves other ACL entries, no world-write), prints effective rights and then creates the unique -rw source mountpoint. This is not an authorization model for a production AFS cell.
- Proxmox Debian 12 VM and tools retained; the GitHub runner remains ephemeral/one-job, SHA-pinned main-only dispatcher. A true encrypted full/incremental Rust restore with byte-identical check remains UNCERTIFIED pending live PASS.


## OpenAFS field CI 43745989687: wrong `fs listacl` switch (15 Nov 2026)
- The real KDC/OpenAFS provisioning and Rust release build succeeded. The acceptance script stopped at diagnostics, BEFORE the ACL change or RW mount: `fs: Unrecognized or ambiguous switch '-dir'; type 'fs help listacl' for detailed help`.
- Fix both `fs listacl -dir` invocations to **`fs listacl -path`**, the actual documented OpenAFS CLI syntax. Retain `fs setacl -dir` (correct) and `fs getcalleraccess -path` (correct). Add static-lint rejection of invalid `listacl -dir`; no relaxation of restricted lab cell guard or security checks.
- Reusable Debian 12 VM and ephemeral runner policy preserved. Lab real AFS full/incremental/restore still NOT certified.


## Field OpenAFS CI 43746542514: genuine full + incremental PASS; vos progress breaks JSON restore result

- [Workflow run 43746542514](https://github.com/desalvo/multi-backup/actions/runs/43746542514) **failed overall** but completed the genuine Kerberos/OpenAFS lab, RW source mount, ACL repair, Rust release build, true age/zstd encrypted full backup with integrity verification, incremental dump and full-chain verification. `--confirm-target wrong` was rejected as designed.
- Restore was started for a fresh target volume; its child `vos restore` printed `Volume does not exist; Will perform a full restore` to **stdout**. The outer Rust CLI used `--json` and wrote JSON to the same stream, yielding `jq: parse error: Invalid numeric literal`. The CLI command returned exit 0, but we **cannot call restore certified** because the field test stopped before checking restored count, existence, byte identity and duplicate-reject.
- Rust `afs_volume::restore_new_chain_from_store` now captures the child `vos`/SSH stdout in a piped background drain and streams its messages to **stderr** while the parent CLI retains clean machine-readable stdout. The reader is joined/checked after the restore process exits, without unbounded capture or deadlock with live encrypted-dump stdin streaming. Adds a Rust test that verifies the relay and excludes simulated vos from real field CI. The static AFS workflow additionally compiles/tests the helper. No source archive, restored volume or credential is overwritten.
- Persistent isolated Debian 12 Proxmox VM remains reusable. GitHub runner still one-shot ephemeral; trusted main SHA pin/lease cleanup unchanged. Next live test is required for end-to-end full+incremental→restore byte-for-byte acceptance and negative tests.


## r039 real OpenAFS PASS; PG16 agents/HA field tests added (2026-11-15)
OpenAFS actual KDC/VLDB, token, encrypted Rust FULL+incremental and new-volume byte-identical restore is PASS at Actions 43747477666 on d80675f; invalid confirmation, overwrite and absent source are also rejected. Newly added isolated authentic PostgreSQL 16 acceptance tests in rust-server/src/live_pg_acceptance.rs validate agent admin approval, Ed25519 signature+token, replay, approved profile pull and upload receipt/revocation, plus two replica DB node_local/shared placement, capacity, expired lease quarantine, stale writer fencing and explicit recovery attestations. Workflow agent-cluster-live-pg.yml must pass before marking tests verified. This is not real ingress/TLS upload, encrypted spool data transfer, or Kubernetes pod failover; those are separate RC gates. Maintain PR #10/#11 draft, no signed release.



## SQL concurrency qualification closed — source 1d71bbb (15 November 2026)
- Real PostgreSQL 16 acceptance run 43750032081 revealed that a single-statement SELECT FOR UPDATE inside the claim CTE could read a stale READ COMMITTED snapshot and overbook a replica (capacity=1, concurrent accepted=2). We added an actual 8-worker contention test plus eight concurrent identical nonce commits.
- Rust server fixes the race with one transaction and two SQL statements: lock the owning replica row first, then get a fresh capacity snapshot to claim a queued job; commit both atomically. No scheduling bypass and no weaker storage/lease authorization.
- Genuine PG16 tests PASS 3/3 on exact SHA 1d71bbb6998f115097a28b25ddbbdca1e22a7070 in runs https://github.com/desalvo/multi-backup/actions/runs/43750377598 and https://github.com/desalvo/multi-backup/actions/runs/43750382563. Rust focused, POSIX, SSH/SSHFS, database/SQL PITR, Galera, static AFS runner also PASS on this exact source. Main production CI for exact source https://github.com/desalvo/multi-backup/actions/runs/43750382603 still PENDING at audit; signed RC remains blocked by real provider credentials, Kubernetes physical writer fencing, security and signing.

## r040: real Kubernetes CI acceptance gate (15 November 2026)

- Draft PR #12 (https://github.com/desalvo/multi-backup/pull/12) builds on draft PR #11 without merging to main. Commit c0736cf introduced `kubernetes-real-cluster` after the Production Gate: genuine kind control-plane + two worker Kubernetes nodes, two live Rust OCI server replicas, real PostgreSQL 16 with self-signed ephemeral CA/SAN and verified `sslmode=verify-full`, distinct-node scheduling, private gateway/unauthenticated API checks, deletion/replacement of one replica and shared SQL registration. Every secret is generated within ephemeral CI only.
- The release DAG requires Kubernetes acceptance before container or native publication; all earlier signing/notarization and real provider gates remain mandatory. Kubernetes job is *not* an RC certification of a production multi-machine failure domain, RWX shared data, physical writer fencing, full agent networking or distributed restore: cluster workers share one CI VM, storage is deliberately pod-local, and the backup executor is disabled.
- First PR #12 CI run 43751962614 was stopped by existing exact `needs` assertions in Python and the Production Gate policy. Update their expected release dependency lists to include Kubernetes; add a contract for kind deployment and verified TLS. Also restart kubectl port-forward after pod replacement (a port-forward to a Service is actually pinned to one pod).
- Refer to docs/KUBERNETES_CI_RC_R040.md. RC still blocked pending all outstanding live-provider, HA RWX/lease quarantine/fencing, installer verification and security requirements. Do not merge or publish stable on this basis.

## r040 live Kubernetes discovery: synchronous PostgreSQL nested Tokio startup panic (15 November 2026)

- CI run 43752224997 passed all 32 other jobs including production security and Rust server/container builds. The actual Kubernetes 3-node kind boot and OCI image loading also passed, then both Rust server Pods CrashLoopBackOff because `Database::configured()` was called directly in `#[tokio::main] async fn main()`. The synchronous `postgres 0.19.14` driver attempted to initialize its own Tokio runtime and panicked: `Cannot start a runtime from within a runtime`. Hence the RC Kubernetes acceptance was **FAIL**, not passed.
- Correct both cluster bootstrap schema/TLS connection and initial PostgreSQL replica heartbeat by moving synchronous calls into `tokio::task::spawn_blocking`, using `.await??` to propagate startup errors, without loosening TLS validation or the fail-closed boundary. The recurrent heartbeat already ran in `spawn_blocking`.
- The next CI must demonstrate real 2-Pod readiness, TLS-verified PG registration, private gateway and Kubernetes pod recovery. Other synchronous PG access paths in authenticated axum handlers warrant separate load/auth testing before final RC. Keep all PRs draft and stable publication disabled until qualified.

## r040 cluster authenticated HTTP runtime safety

The first genuine Kubernetes run exposed nested-Tokio runtime panics on synchronous PostgreSQL startup calls. The same synchronous client was previously called from `session()`, login/OIDC and logout request paths. All cross-replica HTTP session reads/writes and OIDC state operations now dispatch PostgreSQL I/O through `tokio::task::spawn_blocking`, propagate errors and fail closed; admin agent/cluster authorization is async and awaits shared SQL session validation before any state-changing action. These changes are NOT RC-qualified until independent live login/cross-pod/logout and race/timeout tests pass. The prior native/bare-PG tests did not exercise authenticated HTTP routes on two live pods.

## Cross-Pod HTTP session and runtime integration

The kind acceptance test now authenticates with the known public CI-only Argon2id hash via Pod A, passes the generated HttpOnly session cookie to a direct tunnel to Pod B, checks the admin-only cluster/status API on BOTH pods, uses the shared CSRF value to log out on Pod B and proves the old cookie is rejected on Pod A. No operational secret/password is reused; all private gateway/PG/TLS keys are freshly generated in CI. This tests the newly blocking-thread-dispatched PostgreSQL session and OIDC path infrastructure; a real external OIDC IdP end-to-end callback remains a separate RC gate. The test also maintains the separate network/kubelet failure tests and cannot stand in for physical fencing.

## r041 — Lustre and Oracle field acceptance preparation (15 November 2026)
- Stacked branch `feature/r041-lustre-oracle-field-acceptance` on r040 draft. Prepared honest hosted CI negative tests and two manually approved self-hosted field suites: Lustre genuine RO MDT/OST snapshots -> actual Rust age/zstd encrypted full+incremental and create-only Lustre restore, strict byte/mode/owner/mtime/ACL/xattr/stripe comparisons; Oracle licensed RMAN `BACKUP VALIDATE CHECK LOGICAL DATABASE` and `RESTORE DATABASE VALIDATE` on separate laboratory only. No genuine Lustre/Oracle lab job has been executed or claimed PASS.
- A normal POSIX directory is rejected as Lustre, and Oracle must remain non-executable in generic Rust database adapters; scripts fail closed before any unapproved lab access. Explicit owner/commit SHA/environment/runner checks and manual workflow_dispatch only.
- Current Rust Lustre metadata preservation is incomplete; field suite is intentionally strict and may fail until implemented. Oracle RMAN verification is **preflight only**, not backup/restore through Multi Backup; Oracle stays PLANNED and release-blocking.
- Docs: `docs/LUSTRE_ORACLE_FIELD_ACCEPTANCE_R041.md` (and IT companion). Workflows: `lustre-oracle-guards.yml` and `lustre-oracle-field.yml`. Complete these gates only with authenticated real-lab evidence for the exact commit, do not merge/publish stable merely for a hosted PASS.

- The compiled Rust fake-Lustre/Oracle negative suite now also runs **inside the existing unified Production Gate** before any native/OCI publication; the standalone provider workflow remains a focused independent check. Exact-head full CI after this change is required.

## Real lab CI acceptance (15 November 2026)

- **Identical tested source SHA**: `eb3dbf2f964686452fbacc865fd1e9e3be1fcd92`.
- **Lustre genuine Rocky9 QEMU lab PASS**: [run 43761650658](https://github.com/desalvo/multi-backup/actions/runs/43761650658). Actual Lustre kernel, one live MDT, three live OST, native client, 2-OST file striping, file checksum verification and independent read-only client mount. Also [earlier independent PASS 43760867316](https://github.com/desalvo/multi-backup/actions/runs/43760867316). This is an authentic Lustre **infrastructure** acceptance, not a Rust encrypted Lustre restore or consistent MDT+OST snapshot certification.
- **Oracle Database Free authentic RMAN PDB backup-piece roundtrip PASS**: [run 43761650595](https://github.com/desalvo/multi-backup/actions/runs/43761650595). In a disposable Oracle Free regular image, `FREEPDB1` runs in ARCHIVELOG, RMAN creates real PDB backup pieces, Multi Backup Rust generic POSIX adapter encrypts and verifies those pieces using age/zstd, restores them byte-for-byte, and RMAN runs `RESTORE PLUGGABLE DATABASE FREEPDB1 VALIDATE` against the recovered pieces. This does not perform an actual PDB restore and does not implement a native Oracle provider.
- **Hosted provider guard PASS**: [run 43761650585](https://github.com/desalvo/multi-backup/actions/runs/43761650585). Full production CI [run 43761650615](https://github.com/desalvo/multi-backup/actions/runs/43761650615) is independent and must be separately verified at its final conclusion.
- Previous failed CI attempts discovered and fixed actual constraints: rootless Podman GID mapping, QEMU/KVM ACL, outdated Lustre procfs check, unauthenticated GitHub releases throttling, Oracle readiness test, and RMAN-06817 requiring ARCHIVELOG for online PDB backup.
- **Still release-blocking**: Lustre coordinated MDT+OST snapshots, actual native Rust encrypted Lustre full/incremental restore with mode/owner/ACL/xattr/mtime/striping verification, Oracle native RMAN adapter and complete full/archivelog/incremental restore onto an independent new database with PITR, and all other RC security/HA/installer gates. No stable release/merge authorization inferred from these smoke runs.
