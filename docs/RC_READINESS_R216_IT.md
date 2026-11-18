# Multi Backup r216 — installable native RC-preview and narrowly qualified providers

Updated 18 November 2026. Development branch feature/r216-installable-rc-and-provider-field, based on r215. NOT merged to main, NOT tagged, NOT a published stable or public GitHub release.

## Confirmed baseline

r215 has 9 narrowly scoped READY IDs and a genuine 18/18 full green software/provider gate, run https://github.com/desalvo/multi-backup/actions/runs/44202991407. The Linux CLI/agent pre-RC artifact is present on that run (artifact archive SHA256 0f3c981e93518ee6797c8ac2cb0d65b41b33418fccd8b6cf7f8c943cf20845b5). Do not confuse this baseline with uncompleted r216.

## READY r216 proposals — exact source acceptance REQUIRED

The 9 r215 qualified paths are POSIX content-only, pinned MinIO S3 single object, CephFS attested regular bytes, RBD pinned full snapshot to NEW image, kernel iSCSI pinned RO source to NEW local TGT LUN, MariaDB/MySQL one-database logical FULL to new DB, PostgreSQL16 one-DB full logical, Oracle precreated RMAN .bkp media to new directory (NOT DB auto-restore), MinIO WORM COMPLIANCE vault under two independent nonadmin IAM principals (NOT root-immune).

Four new r216 qualified candidates, bringing the catalog to **13 scoped READY / 33 IDs**, ONLY after same-source CI:

| Plugin | B-In | B-Out | Restore input | Restore output | Explicit narrow promise |
|---|---|---|---|---|---|
| ssh | READY | Planned | Planned | Planned | One remote file, pinned known_hosts, independent SHA+size; local age recovery. NO recursive SFTP, remote B-Out/restore or host compromise immunity |
| sshfs | READY | READY | READY | READY | Genuine SSHFS mounted FUSE RO/RW, quiesced regular-file bytes, new remote RW path restore and encrypted create-only archive B-Out. NO metadata ACL/xattr, remote snapshots, symlinks |
| webdav | READY | READY | READY | Planned | Authenticated HTTPS davfs2 RO frozen byte-source; native conditional ciphertext PUT + independent GET/digest and restore to NEW LOCAL path. NO plaintext restore-to-WebDAV, ACL/ETag/resumable large |
| lustre | READY | READY | READY | READY | Genuine Rocky9 KVM Lustre MDT+OST quiesced regular-file FULL + age restore new path and encrypted archive B-Out. NO online coordinated MDT/OST snapshot, metadata/stripe fidelity, HA/fencing |

The remaining 20 provider IDs are NOT Ready. READY does NOT imply a generic data mover or a whole-vendor certified integration. Only POSIX->POSIX uses the actual compiled generic dispatcher; dedicated adapters have specific commands and fail-closed safety checks.

## Current-SHA tests and infrastructure

.github/workflows/rc-preflight-r216.yml is the one source-SHA fan-in for:
- Full Rust core/server/agent/GUI Xvfb, genuine PostgreSQL and 3-node Kubernetes kind / OCI gateway readiness.
- Nine genuine provider labs: MySQL8.4 InnoDB, CephFS/RBD 3 OSD, Linux iSCSI TGT, Oracle Free RMAN CDB/PDB/SCN PITR, MinIO two-principal WORM COMPLIANCE, pinned MinIO S3 object, OpenSSH plus SSHFS real FUSE, authenticated real WebDAV HTTPS, real Lustre Rocky9 KVM MDT+OST.
- Native Rust catalog with 13 exact scoped Ready role sets, Python policy negatives, Ruff/Bandit/pip-audit, provenance rejection.
- .github/workflows/native-installers-r216.yml installs actual packages on the exact revision: Ubuntu24 amd64 DEB (GUI/Xvfb, real age backup+restore, deny overwrite, uninstall), macOS ARM64 and x86_64 native Rust DMGs (ad-hoc codesign, mount, copied app native GUI smoke), Windows 2025 x64 genuine native Rust Inno (fresh silent installation, GUI start, CLI and age). No Python/PyInstaller is in these installed artifacts.
- scripts/ci/verify-platform-artifacts-r216.py validates all four original installer hashes and matching GitHub source commit after download. scripts/ci/rc-preflight-r216.py checks the 9 real provider lab result fields and 4 installer gate result fields, not arbitrary user-supplied PASS claims.
- Only an all-green exact-SHA run authorizes an Actions-only RC0-preview bundle, independent individual OS artifacts and SHA256 checksums. No mutable tag, GitHub release or stable repository publication.

## Installation guidance once and ONLY once r216 is green

Open the Actions run, download the appropriate uploaded OS artifact and verify its SHA256 and SOURCE_SHA file.

- Ubuntu 24.04 amd64: install the package multi-backup-rust-rc-0.1.0~rc0.r216+ubuntu24.04-amd64.deb with sudo apt install ./filename.deb, launch multi-backup-rust. Desktop, Rust CLI and agent are native, installed package can be purged cleanly.
- macOS arm64 / x86_64: mount the corresponding DMG and copy Multi Backup Rust Preview.app to Applications. This is AD-HOC signed only, NOT Gatekeeper notarized. Do not bypass system security on production machines.
- Windows x64: install the preview Inno EXE on a test machine. It is UNSIGNED Authenticode; SmartScreen may block it, so this is not an approved trusted production distribution.

## Honest remaining RED/ORANGE certification gates

- macOS Apple Developer ID signing/notarization, Windows Authenticode, signed RPM/DEB repositories (including Rocky/Alma 9/10 and Debian12/13, both arches), SBOM/provenance and tested auto-update/rollback.
- Genuine VMware vCenter/ESXi VADP+CBT + guest boot, oVirt Engine full-VM restore/boot, PVE/PBS direct VM/CT restore, three PHYSICALLY distinct hosts with HA, network/storage fault/fencing.
- Oracle RAC/ASM/TDE, MySQL GTID/binlog PITR, remote SAN multipath/CHAP and coordinated online MDT/OST snapshots.
- Host root compromise, cross-vendor physical failure and tamper-proof retention beyond the independently verified MinIO COMPLIANCE IAM scope.

Handoff files: docs/HANDOFF.md and docs/DEVELOPMENT_STATUS.md. Last confirmed all-green pre-r216: https://github.com/desalvo/multi-backup/actions/runs/44202991407 . Current r216 CI must not be called green without checking its final conclusion.

One-line English commit summary: Certify bounded SSH SSHFS WebDAV Lustre paths and installable Rust Linux Mac Windows RC0 artifacts with exact-SHA field gates.
