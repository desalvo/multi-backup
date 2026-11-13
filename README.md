# Multi Backup

**Multi Backup `0.1.0-r009-rc1`** is a cross-platform, encrypted, resumable backup client with independent source plugins. It is licensed under **EUPL-1.2**.

![Multi Backup dashboard](docs/screenshots/dashboard.png)

**Release-candidate status:** GitHub backup and archive browsing are inherited from the previous `github-backup` tool. Dropbox incremental export and release discovery are new and require integration testing with real accounts. Native packages and RPM repository publishing are defined by CI but are not certified until their complete runners, installer validation, and signing checks have passed.

## Capabilities

- **GitHub plugin (enabled by default):** mirror repositories with Git LFS, retain distinct snapshots, back up metadata, issues/PRs, workflow runs, logs, release assets, GitHub settings and optional user-supplied credentials. All exported backup archives use `age` encryption. The CI artifact cleanup is **opt-in** and conservatively protects distributable artifacts; it can simulate actions before deletion.
- **Dropbox plugin (disabled by default):** read-only Dropbox API, list file revisions and back up changed versions as individual encrypted archives, reuse already verified objects, create timestamped encrypted manifests. **Never modifies or deletes files in Dropbox; never purges local backups.** Dropbox integration is an RC feature requiring live-account testing.
- **Resumable backups:** atomic archives, SHA-256 verification, checkpoint journals, `--restart` to create a fresh independent cycle.
- **Archive explorer:** list, display, extract, Git checkout and optional read-only FUSE mounting.
- **Native GUI:** Dashboard, Backup, Archives, Plugins, Encryption, Updates and Settings with real-time progress.
- **Two release channels:** `early-release` after a `[full-ci]` commit to `main`, and eventually `stable` after a stable version tag (for example `v0.1.0`). RC1 blocks stable publication until signed native packages and repositories are available. New versions are checked at launch and every six hours by default. Verified installers are staged but never executed without explicit approval.

## Requirements

Python **3.11+** for source execution; `pip install -r requirements.txt` for the graphical interface. GitHub backup additionally requires **Git, Git LFS, GitHub CLI (`gh`), and `age`**; `gh auth login` must have access to the requested repositories. Dropbox requires **age**, an `age1...` recipient, and an access token supplied via the `MULTI_BACKUP_DROPBOX_TOKEN` environment variable or an OS keyring entry (`service=multi-backup`, `username=dropbox`). Never put tokens in CLI arguments or settings files.

For native distributions, package dependencies are checked per platform. Linux RPM package builds are designed for **Rocky/Alma 8, 9 and 10 (x86_64)**, macOS builds target **Ventura 13+ on Apple Silicon and Intel**, and Windows builds target **Windows 11 x64**. Unverified CI builds and unsigned installers must not be treated as production-certified releases.

## Quick start

```bash
# From the source checkout
python3 -m pip install -r requirements.txt
python3 multi_backup.py --version
python3 multi_backup.py plugins
python3 multi_backup.py keys generate --name primary
python3 multi_backup.py backup --backend github --recipient age1YOURPUBLICKEY --progress
python3 multi_backup.py backup --backend github --cleanup-ci-artifacts --keep-ci-runs 5 --cleanup-ci-dry-run --progress
python3 multi_backup.py gui
```

Resume any interrupted GitHub cycle by repeating the same command and settings. Use `--restart` for a brand new cycle; existing complete archives are never silently overwritten.

For Dropbox (explicitly enable the plugin first):

```bash
python3 multi_backup.py plugin enable dropbox
export MULTI_BACKUP_DROPBOX_TOKEN='YOUR_TOKEN'  # Prefer your OS keyring if available
python3 multi_backup.py backup --backend dropbox --recipient age1YOURPUBLICKEY --dropbox-path ''
```

The plugin uses read-only Dropbox permissions and never calls a delete API. Each changed file revision is stored once; repeated cycles reuse SHA-256-verified encrypted archives. Private tokens are never added to a backup by this plugin.

## Browse and recover

```bash
python3 multi_backup.py list --output-dir "$HOME/Multi-Backups"
python3 multi_backup.py browse --identity "$HOME/.config/multi-backup/keys/primary.txt"
python3 multi_backup.py view /path/to/archive.tar.gz.age --identity /path/to/identity.txt
python3 multi_backup.py show /path/to/archive.tar.gz.age path/inside.tar --identity /path/to/identity.txt
python3 multi_backup.py extract /path/to/archive.tar.gz.age /safe/directory --identity /path/to/identity.txt
python3 multi_backup.py checkout /path/to/git-archive.tar.gz.age /fresh/checkout --identity /path/to/identity.txt
```

Use `mount`/`unmount` with `bindfs` on Linux/macOS, or `subst` on Windows; mounting requires a decrypted temporary working copy and therefore a private encrypted storage volume. Never expose decrypted temporary files to shared workstations.

## Release channels and updates

```bash
python3 multi_backup.py settings channel stable
python3 multi_backup.py settings channel early-release
python3 multi_backup.py updates check
python3 multi_backup.py updates download --download-dir "$HOME/Downloads"
```

Update discovery uses the public GitHub Releases API. Binary downloads require a GitHub-provided SHA-256 digest and enforce an HTTPS allowlist. **The RC does not silently install unsigned updates**. Trusted OS code signing, macOS notarization and signing keys for RPM repositories must be configured before enabling unattended upgrades.

The early release points to [GitHub Releases: early-release](https://github.com/desalvo/multi-backup/releases/tag/early-release); stable releases appear in [GitHub Releases](https://github.com/desalvo/multi-backup/releases). RPM `repodata` is generated for each Rocky/Alma version and channel on the `gh-pages` branch. Its `.repo` definitions are deliberately **disabled** (`enabled=0`, `gpgcheck=1`) until RPM and repo metadata signing is available.

## Documentation

- [User manual, English (PDF)](docs/manuals/Multi-Backup-User-Manual-EN.pdf)
- [Manuale utente, italiano (PDF)](docs/manuals/Multi-Backup-Manuale-Utente-IT.pdf)
- [Brochure, English (PDF)](docs/brochures/Multi-Backup-Brochure-EN.pdf)
- [Brochure, italiano (PDF)](docs/brochures/Multi-Backup-Brochure-IT.pdf)
- [Release channels](docs/RELEASE_CHANNELS.md)
- [Plugin architecture](docs/PLUGIN_API.md)
- [Security engineering](docs/SECURITY.md)
- [Development status and handoff](docs/DEVELOPMENT_STATUS.md)

## Security and limitations

- GitHub Secrets API **does not return the original secret values**. External storage, registries, and services need separate backup policies.
- Network and file errors are reported and archived only when safe; verify the completion report and restore a sample backup periodically.
- CI artifact deletions are **irreversible**, always opt-in, and not provably safe for external systems with undeclared dependencies. Keep a dry run first.
- Dropbox support only backs up file data and basic revision metadata in this RC, not complete Dropbox team/admin settings or sharing ACLs.
- Do not store encryption identities beside backup ciphertext; keep a separate encrypted/offline copy.
- Native GUI and installers are RC builds, not security-audited production software. See [security](docs/SECURITY.md).

### Development

```bash
python3 -m compileall -q .
python3 -m unittest discover -s tests -v
```

**License:** [EUPL-1.2](LICENSE). Contributions: see repository issues and the development handoff.

### Production Gate and macOS drag-and-drop installation (RC)

Every push first runs a single **Production Gate**, including security scanning; native jobs and publication run **only after it passes**. A commit with `[full-ci]` on `main` triggers early-release binaries, while an ordinary version tag targets stable (still blocked pending platform signing policies). You can manually trigger a full release from **Actions → Multi Backup - Production CI → Run workflow**.

The corrected macOS DMG contains only **`Multi Backup.app`**. Drag it into **Applications** and launch from Finder. The application bundle includes the CLI and cryptography helpers; `git` must be available on macOS. The macOS package **will not be published** unless Apple Developer ID signing and both app+DMG notarizations pass, followed by a mounted-DMG relocation and Gatekeeper test. See [Release channels and required GitHub secrets](docs/RELEASE_CHANNELS.md). Previously published, unsigned RC DMGs are not guaranteed to open and should not be installed. Do not bypass Gatekeeper.

### Dropbox integrity verification (r009)

Each newly downloaded Dropbox revision must match its listing file ID, revision, byte count and official Dropbox content hash before encrypted archive creation. A damaged pre-existing ciphertext is preserved and reported, never silently replaced. Dropbox objects without a valid content hash are not accepted.
