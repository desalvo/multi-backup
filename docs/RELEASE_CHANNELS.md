# Native releases, channels, and trust

**Version:** `0.1.0-r003-rc1`. Sources: GitHub repository `desalvo/multi-backup`.

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
