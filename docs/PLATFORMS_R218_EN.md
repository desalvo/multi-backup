# Multi Backup r218 — platforms and capability matrix

Date: 18 November 2026. r217 full CI #44246558178 completed SUCCESS at source b085abc. r218 adds independent mobile and cloud development paths, not a signed public release.

| Area | Platforms | Access | Status |
|---|---|---|---|
| Rust server and agents | Linux, OCI container, Kubernetes | Authenticated HTTPS web and REST | r217 acceptance passing; real physical three-host HA not certified |
| Rust native desktop | macOS Apple Silicon/Intel, Windows x64, Ubuntu 24 x86_64 | Native installed apps | Four installed package tests passed in r217, unsigned |
| Additional Linux targets | Rocky/AlmaLinux 8/9/10, Debian 12/13 | RPM/DEB/service | Signed cross-platform install/rollback matrix open |
| Web | Desktop, tablet and phone | r218 responsive console | Source and CI tests; field devices not yet certified |
| iPhone/iPad | iOS/iPadOS 16+ | SwiftUI+WKWebView universal app | Unsigned device and Simulator CI builds passed; actual device installation needs signing |
| Android | Android 8+/API 26+ | Java+WebView | Source and optional unsigned APK CI being qualified |
| Cloud explorer | Dropbox, Google Drive, iCloud Drive, OneDrive, ownCloud, Nextcloud, Box | Remote browser and opt-in upload/download/delete; desktop FUSE/WinFsp mount | Synthetic provider tests; field integrations not certified |
| iCloud backup | iCloud Drive with externally configured rclone+2FA | Exact file age B-In / encrypted B-Out | Real age integration + synthetic transport PASS; provider Preview |

Backup plugin catalog: **34 entries**, of which **16** have at least one tightly bounded READY operation. Box is available only as a generic cloud explorer backend, not a qualified backup plugin. Browsing the cloud drive is not the same as browsing inside a verified encrypted backup; the automatic remote archive → verified manifest → selective restore pipeline remains open.

See corresponding Italian guides: MOBILE_CLIENTS_R218_IT.md, CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md, ICLOUD_B_IN_OUT_R218_IT.md.
