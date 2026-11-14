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
# Execute real encrypted create -> verify -> restored-byte comparison on
# the same native macOS architecture used by the final preview app.
cargo test --manifest-path rust/Cargo.toml --test native_cli_restore_matrix
cargo test --manifest-path rust/Cargo.toml --test plugin_dispatch_roundtrip
APP="dist-rust/Multi Backup Rust Preview.app"
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
  <key>CFBundleName</key><string>Multi Backup Rust Preview</string>
  <key>CFBundleDisplayName</key><string>Multi Backup Rust Preview</string>
  <key>CFBundleIdentifier</key><string>net.adsnet.multibackup.rust-preview</string>
  <key>CFBundleExecutable</key><string>Multi Backup</string>
  <key>CFBundleIconFile</key><string>MultiBackup</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>38</string>
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
STAGE="$(mktemp -d -t multi-backup-rust-stage.XXXXXXXX)"
MOUNT="$(mktemp -d -t multi-backup-rust-mount.XXXXXXXX)"
# Never create hdiutil logs inside STAGE: -srcfolder would package them as
# visible files next to the app and the Applications shortcut.
CREATE_LOG="$(mktemp -t multi-backup-rust-create-log.XXXXXXXX)"
ATTACH_LOG="$(mktemp -t multi-backup-rust-attach-log.XXXXXXXX)"
cleanup() {
  if mount | grep -F "$MOUNT" >/dev/null; then hdiutil detach -quiet "$MOUNT" || true; fi
  rm -rf "$STAGE"
  rm -f "$CREATE_LOG" "$ATTACH_LOG"
  rmdir "$MOUNT" 2>/dev/null || true
}
trap cleanup EXIT
# Exactly one native app plus the familiar Applications drag-install shortcut.
ditto "$APP" "$STAGE/Multi Backup Rust Preview.app"
ln -s /Applications "$STAGE/Applications"
test -L "$STAGE/Applications"
# Validate the distribution source is exactly app + symlink before imaging.
test "$(find "$STAGE" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d '[:space:]')" = 2
# macOS Intel runners can collide with diskimages services. Retry only
# a confirmed transient busy error, never bypass a signing or integrity check.
for attempt in 1 2 3 4; do
  if hdiutil create -fs HFS+ -volname "Multi Backup Rust Preview" \
    -srcfolder "$STAGE" -format UDZO "$DMG" >"$CREATE_LOG" 2>&1; then
    break
  fi
  cat "$CREATE_LOG" >&2
  if ! grep -Eqi 'Resource busy|Resource temporarily unavailable' "$CREATE_LOG" ||
     [[ "$attempt" == 4 ]]; then exit 1; fi
  rm -f "$DMG"
  sleep "$((attempt*3))"
done
test -f "$DMG"
echo "[DMG] Created $(ls -lh "$DMG" | awk '{print $5}') image for $ARCH"
for attempt in 1 2 3 4; do
  if hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$DMG" \
       >"$ATTACH_LOG" 2>&1; then break; fi
  cat "$ATTACH_LOG" >&2
  if ! grep -Eqi 'Resource busy|Resource temporarily unavailable' "$ATTACH_LOG" ||
     [[ "$attempt" == 4 ]]; then exit 1; fi
  sleep "$((attempt*3))"
done
echo '[DMG] Mounted root entries (including system metadata):'
find "$MOUNT" -mindepth 1 -maxdepth 1 -print | sort
[[ -d "$MOUNT/Multi Backup Rust Preview.app" ]] || { echo '::error::DMG missing the Rust Preview.app bundle' >&2; exit 1; }
[[ -L "$MOUNT/Applications" ]] || { echo '::error::DMG missing Applications shortcut' >&2; exit 1; }
[[ "$(readlink "$MOUNT/Applications")" == "/Applications" ]] || { echo '::error::Applications shortcut points somewhere unexpected' >&2; exit 1; }
# The HFS+ image may grow extra hidden macOS filesystem metadata entries.
# Validate the exact two user-facing install items, not incidental dotfiles.
visible_items="$(find "$MOUNT" -mindepth 1 -maxdepth 1 ! -name '.*' -print | wc -l | tr -d '[:space:]')"
[[ "$visible_items" == 2 ]] || { echo "::error::Expected app+Applications, found $visible_items visible DMG items" >&2; exit 1; }
echo '[DMG] Running installed app self-test'
"$MOUNT/Multi Backup Rust Preview.app/Contents/MacOS/Multi Backup" --self-test
echo '[DMG] Verifying installed app signature'
codesign --verify --deep --strict --verbose=2 "$MOUNT/Multi Backup Rust Preview.app"
shasum -a 256 "$DMG" > "$DMG.sha256"
echo "Rust-only native $ARCH DMG preview verified."
