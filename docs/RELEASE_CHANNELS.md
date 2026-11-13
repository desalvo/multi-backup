# Native releases, channels, and trust

**Version:** `0.1.0-r010-rc1`. Sources: GitHub repository `desalvo/multi-backup`.

## Trigger contract

- Any normal push to `main`: the validation workflow runs (no native release).
- A push to `main` whose *head commit message* contains the literal `[full-ci]`: validation plus all native builds run; if all required builds pass, binaries are uploaded to the rolling **`early-release`** GitHub Release. This is never promoted to stable automatically.
- A push of an ordinary version tag matching `v*` **without** `-rc` (for example `v0.1.0`): validation plus all native builds run, then assets are uploaded under that tag as **stable**. Stable is never triggered by a `[full-ci]` message.
- Manually dispatched builds are early only.

A release contains DMGs for macOS Apple Silicon and x86_64, a Windows x64 setup EXE and ZIP, separate RockyLinux/AlmaLinux EL8, EL9 and EL10 RPMs, the English and Italian manuals and brochures, checksums and a machine-readable manifest. Files are downloadable from the matching GitHub Release.

## RPM repositories

The `rpm-repositories` workflow generates per-distribution repository metadata and `.repo` definitions, publishing into the `early-release` or `stable` subtree of the `gh-pages` branch. GitHub Pages must be enabled by the repository administrator before public `https://desalvo.github.io/multi-backup/...` download URLs become active. Repository definitions are generated **disabled** with `gpgcheck=1` and `repo_gpgcheck=1`; enabling requires signed RPMs and signed repository metadata. This is deliberate fail-closed behavior. Until signed metadata and public GPG key distribution are configured, direct download plus independent verification is the RC mechanism.

## Automatic updates

GUI and CLI check the configured channel (default **stable**) at app startup and every 6 hours while GUI is open. Users may choose **early-release** in Settings. The updater queries GitHub Releases over HTTPS and will only stage an asset when GitHub provides a SHA-256 digest. Staging does not execute any executable. Replacing installed files in use requires an OS-specific installer, signature and user authorization.

**Remaining production gates:** Apple Developer ID signing and notarization; Microsoft Authenticode signing; signed RPMs and `repomd.xml`; installer acceptance tests, migration tests against representative legacy backups, and checks of automatic update behavior in installed native apps. Never publish a stable release that fails these gates.

## Native build matrix

| OS | Architecture | Baseline | File |
| --- | --- | --- | --- |
| macOS | Apple Silicon / Intel | macOS 13+ | `.dmg` |
| Windows | x64 | Windows 11 | `.exe`, `.zip` |
| RockyLinux | x86_64 | 8, 9, 10 | `.rpm` |
| AlmaLinux | x86_64 | 8, 9, 10 | `.rpm` |

## Known packaging risks

Helper CLI dependencies (`gh`, `git-lfs`, `age`) must be found on PATH or bundled by the native installer; `git` is a system prerequisite. The CI aborts Windows packaging if `age.exe` is unavailable. Linux RPM currently depends on external helpers which must be installed separately. Satisfying the build matrix is not equivalent to validating notarization or app execution on each target OS.

## 0.1.0-r008-rc1: single Production Gate and functioning macOS app (required)

`ci.yml` is the **only event-triggered CI**. Every push and pull request first executes one job, `Production Gate - tests, security and release policy`, covering Python 3.11/3.12/3.13, CLI smoke, Ruff, Bandit, vulnerability scans of **both** runtime and documentation dependencies, release assets and workflow invariants. The `build-native.yml` workflow is **reusable only** and is called **after the gate succeeds** for `[full-ci]`, ordinary version tags or an explicit manual dispatch. Publication depends on *all* native builds being successful, and RPM indexes cannot be published before the Windows and macOS build checks are also green.

The macOS installer is a drag-and-drop disk image with **exactly one visible item**: `Multi Backup.app` (copy it into `/Applications`). The CLI and `age`, `age-keygen`, `gh`, `git-lfs` are embedded *inside* the `.app` rather than shown as separate download items. The GUI `--self-test` validates its bundled assets; the CLI is available at `/Applications/Multi Backup.app/Contents/MacOS/multi-backup`. macOS must have functional `git` installed (Apple Command Line Tools if absent). No naked Python script or binary is placed at DMG top level.

**Release security is fail-closed.** An unsigned, unnotarized macOS app can appear as “damaged” under Gatekeeper. Both macOS runners now import an ephemeral **Developer ID Application** certificate, sign the assembled app and nested executables, notarize and staple the app itself, create a one-item DMG, notarize/staple the DMG, mount it, copy the app to a temporary `Applications` directory and verify its architecture, signature, Gatekeeper acceptance, CLI and GUI bundle self-test. If any step fails or Apple credentials are unavailable, the workflow **must not update the public release**. Do not instruct users to remove quarantine, disable Gatekeeper, or use ad-hoc signatures as a substitute.

### Required GitHub Actions secrets for macOS releases

Configure these under **Settings → Secrets and variables → Actions → Repository secrets**, without inserting private material in the repository or chat:

| Secret | Description |
|---|---|
| `APPLE_DEVELOPER_ID_P12_BASE64` | Base64-encoded **Developer ID Application** certificate export (`.p12`) and private key |
| `APPLE_DEVELOPER_ID_P12_PASSWORD` | The `.p12` export password |
| `APPLE_DEVELOPER_IDENTITY` | The exact `Developer ID Application: ... (TEAMID)` code-signing identity |
| `APPLE_NOTARY_API_KEY_BASE64` | Base64 encoding of the App Store Connect `.p8` API key |
| `APPLE_NOTARY_API_KEY_ID` | Notarization API key ID |
| `APPLE_NOTARY_API_ISSUER_ID` | App Store Connect API key issuer UUID |

After adding them, run **Multi Backup - Production CI** with `workflow_dispatch` on `main`, or push a commit containing `[full-ci]`. GitHub Actions will import the signing key only into a temporary keychain and delete it after the run. Apple notarization uses `xcrun notarytool` and two independently verified tickets. You must have a valid Apple Developer Program account to obtain these credentials. Without these secrets, the old public DMGs are **not** validated for a normal Gatekeeper install.

**Important distinction:** The static Production Gate can be green even when a notarization credential is missing; actual code signing and running the app on macOS are separate native *release acceptance checks*, and release publication requires both. Windows Authenticode and RPM GPG signing are still required for the future stable channel.

## r009 unsigned macOS startup qualification

After the sole Production Gate, both macOS architectures build and exercise the real unsigned application (GUI self-test and bundled CLI). These builds are intentionally never uploaded or released; a distinct Developer ID signed and notarized build plus mounted-DMG Gatekeeper acceptance is still required for publication.

## r010 macOS experimental delivery (no Apple Developer membership needed)

After the sole Production Gate the macOS Apple Silicon and Intel smoke jobs produce an ad-hoc signed single-app DMG, validate it mounted and relocated into a disposable Applications directory, and upload the packages as GitHub Actions artifacts `TEST-ONLY-macos-arm64` and `TEST-ONLY-macos-x86_64`. On `[full-ci]` on main, early-release additionally publishes these packages as `multi-backup-macos-<arch>-test.dmg`. Publication removes legacy broken, ambiguous macOS DMG filenames and clearly marks the rolling release as TEST ONLY. These are not notarized; macOS may need an explicit Open Anyway approval. Never globally disable Gatekeeper. Full Apple Developer ID signing, app/DMG notarization and Gatekeeper testing still apply to the separate stable channel. Stable is also blocked pending Windows and RPM signing.


## r012 current macOS prerelease naming

Following feedback from macOS early testers, the rolling early-release uses the straightforward filenames `multi-backup-macos-arm64.dmg` and `multi-backup-macos-x86_64.dmg` (no `TEST-ONLY` label and no `-test` suffix). The GitHub Actions smoke artifacts are named `macos-arm64-preview` and `macos-x86_64-preview`. Old `-test.dmg` assets are removed on the next successful publication. This changes **only labels**, not the prerelease trust policy: packages remain ad-hoc signed, unnotarized and require explicit user approval if blocked by Gatekeeper. The stable channel still requires Apple Developer ID, notarization and independent acceptance tests, alongside Windows/RPM signing.
