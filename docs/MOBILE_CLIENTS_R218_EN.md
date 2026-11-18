# Multi Backup r218 — remote iOS, iPadOS and Android clients (preview)

The mobile applications are **remote server frontends**, not on-device backup agents. They display the existing Rust server web console at `/`, using the same HTTPS session cookies, login, CSRF protection and server-side APIs. Backup data, age private keys, vault and plugins remain on the server.

iPhone and iPad share one native **SwiftUI + WKWebView** universal app (iOS/iPadOS 16+). Android 8+ (API 26+) uses a native Java Activity with the system Android WebView. They provide HTTPS server-origin setup and mobile/tablet responsive navigation. Plain HTTP, embedded credentials, TLS errors and mixed active content are rejected.

The independent workflow `.github/workflows/mobile-unsigned-preview.yml` creates an **unsigned** Android release APK, an **unsigned** universal iOS/iPadOS device IPA archive and an unsigned Simulator app ZIP. CI result is still pending; unsigned device artifacts cannot be installed through ordinary distribution flows without signing/provisioning. These preview jobs are deliberately absent from the r217 mandatory RC gate.

First-preview limitations: SSO through an external identity provider is not yet integrated with system-browser OAuth handoff (local HTTPS login is supported); no offline work, mobile backup engine, biometric unlock, push notifications or physical-device acceptance. Public server TLS and secure proxy configuration remain mandatory. See [Italian detailed instructions](MOBILE_CLIENTS_R218_IT.md).


## Cloud Drive browser

The shared Rust server responsive console includes a Cloud Drive browser with authenticated provider listing and directory navigation, bounded downloads and separately authorized upload/exact-file delete. iPhone, iPad and Android do not mount remote FUSE drives locally; their UI accesses the server. Browsing cloud files is not the same as browsing inside encrypted age backup manifests. See [technical cloud guide (Italian)](CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md).
