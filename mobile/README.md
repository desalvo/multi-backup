# Native remote clients — r218 preview

These are genuinely native mobile application shells, **not** Python wrappers or a second backend.
Both load the same HTTPS Rust server console, which is optimized for phone and tablet widths.

- `ios/`: XcodeGen-generated SwiftUI/WKWebView universal iPhone+iPad Xcode target.
- `android/`: Android Java Activity + WebView, AGP 8.7.3 / Gradle 8.10.2, API 26+.
- `.github/workflows/mobile-unsigned-preview.yml`: independent optional unsigned builds.
- `docs/MOBILE_CLIENTS_R218_IT.md`: security, platform matrix and first-preview limitations.

Security: HTTPS only, no arbitrary SSL verification bypass, no JavaScript bridge, no local age keys, no Android backups of app data, no app-side plugin credentials. Only explicit user-initiated external HTTPS links leave the WebView.

**Unsigned IPA/APK outputs are CI technical previews only, not installable production releases.**
