#!/usr/bin/env bash
# Build a true Rust desktop .app and one-app ad-hoc signed preview DMG.
# No Python, Tk, PyInstaller, or embedded Python runtime.
set -euo pipefail
cd "$(dirname "$0")/.."
ARCH="$(uname -m)"
if [[ -n "$(printenv MAC_ARCH || true)" ]]; then ARCH="$(printenv MAC_ARCH)"; fi
case "$ARCH" in arm64|x86_64) ;; *) echo "Unsupported Mac architecture" >&2; exit 2;; esac
cargo build --manifest-path rust/Cargo.toml --release
cargo build --manifest-path rust-desktop/Cargo.toml --release
CLI="rust/target/release/multi-backup-native"
GUI="rust-desktop/target/release/multi-backup-desktop"
test -x "$CLI" && test -x "$GUI"
"$CLI" --version
"$GUI" --self-test
APP="dist-rust/Multi Backup.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/bin" "dist-rust"
install -m 755 "$GUI" "$APP/Contents/MacOS/Multi Backup"
install -m 755 "$CLI" "$APP/Contents/Resources/bin/multi-backup-native"
for helper in age age-keygen gh git-lfs; do
  binary="$(command -v "$helper" || true)"
  if [[ ! -f "$binary" ]]; then echo "Missing required helper: $helper" >&2; exit 1; fi
  install -m 755 "$binary" "$APP/Contents/Resources/bin/$helper"
  "$APP/Contents/Resources/bin/$helper" --version >/dev/null
done
# LocalAuthentication/Keychain adapter is a compiled native Swift helper
# rather than Python. It is device-only and never prints the vault password
# except to the signed GUI over the explicitly requested unlock pipe.
swiftc -O packaging/macos/biometric.swift -o "$APP/Contents/Resources/bin/multi-backup-biometric"
test -x "$APP/Contents/Resources/bin/multi-backup-biometric"
mkdir -p "$APP/Contents/Resources"
ICONSET="$(mktemp -d -t multi-backup-icon.XXXXXXXX).iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -s format png -z "$size" "$size" assets/icon-1024.png \
    --out "$ICONSET/icon_$size"x"$size".png >/dev/null
  retina=$((size*2))
  sips -s format png -z "$retina" "$retina" assets/icon-1024.png \
    --out "$ICONSET/icon_$size"x"$size"@2x.png >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/MultiBackup.icns"
rm -rf "$ICONSET"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Multi Backup</string>
  <key>CFBundleDisplayName</key><string>Multi Backup Rust Preview</string>
  <key>CFBundleIdentifier</key><string>net.adsnet.multibackup.rust-preview</string>
  <key>CFBundleExecutable</key><string>Multi Backup</string>
  <key>CFBundleIconFile</key><string>MultiBackup</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>20</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# Ad-hoc signing only; notarization is mandatory for stable.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
"$APP/Contents/MacOS/Multi Backup" --self-test
"$APP/Contents/Resources/bin/multi-backup-native" --version
DMG="multi-backup-rust-preview-macos-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -quiet -fs HFS+ -volname "Multi Backup Rust Preview" \
  -srcfolder "$APP" -format UDZO "$DMG"
MOUNT="$(mktemp -d -t multi-backup-rust-mount.XXXXXXXX)"
hdiutil attach -quiet -nobrowse -mountpoint "$MOUNT" "$DMG"
cleanup() { hdiutil detach -quiet "$MOUNT" || true; rmdir "$MOUNT" || true; }
trap cleanup EXIT
test -d "$MOUNT/Multi Backup.app"
test "$(find "$MOUNT" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" = 1
"$MOUNT/Multi Backup.app/Contents/MacOS/Multi Backup" --self-test
codesign --verify --deep --strict "$MOUNT/Multi Backup.app"
shasum -a 256 "$DMG" > "$DMG.sha256"
echo "Rust-only native $ARCH DMG preview verified."
