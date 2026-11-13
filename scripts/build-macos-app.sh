#!/usr/bin/env bash
# Build one relocatable macOS GUI application, including its CLI and helper tools.
set -euo pipefail
# Ad-hoc signed test builds can be tried locally, but are NOT notarized or trusted releases.
# macOS ships Bash 3.2, where an empty array expansion fails under set -u.
# Always retain a harmless PyInstaller option in this argument array.
SIGN_ARGS=(--log-level WARN)
if [[ "${MAC_UNSIGNED_SMOKE:-0}" != 1 ]]; then
  : "${MAC_SIGNING_ID:?Set MAC_SIGNING_ID to a Developer ID Application identity}"
  : "${MULTI_BACKUP_KEYCHAIN:?Set MULTI_BACKUP_KEYCHAIN to the temporary CI keychain}"
  SIGN_ARGS=(--codesign-identity "$MAC_SIGNING_ID" --osx-entitlements-file packaging/macos/entitlements.plist)
fi
: "${MAC_ARCH:?Specify arm64 or x86_64}"
[[ "$MAC_ARCH" == arm64 || "$MAC_ARCH" == x86_64 ]] || { echo 'Unsupported CPU architecture' >&2; exit 1; }
[[ "$(uname -m)" == "$MAC_ARCH" ]] || { echo "Runner architecture $(uname -m) != $MAC_ARCH" >&2; exit 1; }
APP='dist/Multi Backup.app'
python -m pip install -r requirements.txt 'pyinstaller>=6.11,<7'
for command_name in brew iconutil otool codesign git age age-keygen gh git-lfs; do
  command -v "$command_name" >/dev/null || { echo "Missing helper: $command_name" >&2; exit 1; }
done
rm -rf assets/MultiBackup.iconset
mkdir -p assets/MultiBackup.iconset
for size in 16 32 64 128 256 512; do
  cp "assets/icon-${size}.png" "assets/MultiBackup.iconset/icon_${size}x${size}.png"
done
cp assets/icon-32.png assets/MultiBackup.iconset/icon_16x16@2x.png
cp assets/icon-64.png assets/MultiBackup.iconset/icon_32x32@2x.png
cp assets/icon-256.png assets/MultiBackup.iconset/icon_128x128@2x.png
cp assets/icon-512.png assets/MultiBackup.iconset/icon_256x256@2x.png
cp assets/icon-1024.png assets/MultiBackup.iconset/icon_512x512@2x.png
iconutil -c icns assets/MultiBackup.iconset -o assets/multi-backup.icns

# The GUI uses an onedir .app: it is an actual macOS application bundle, not
# a Python file or a self-extracting executable that can break after relocation.
python -m PyInstaller --onefile --clean --noconfirm --name multi-backup \
  "${SIGN_ARGS[@]}" --add-data assets:assets multi_backup.py
python -m PyInstaller --windowed --onedir --clean --noconfirm \
  --name 'Multi Backup' --osx-bundle-identifier 'io.github.desalvo.multi-backup' \
  "${SIGN_ARGS[@]}" --icon assets/multi-backup.icns --add-data assets:assets gui_launcher.py
[[ -d "$APP/Contents/MacOS" && -f "$APP/Contents/Info.plist" ]] || {
  echo 'PyInstaller did not create a valid application bundle' >&2; exit 1;
}
# Required to be able to copy the application into /Applications and launch it.
plutil -replace LSMinimumSystemVersion -string '13.0' "$APP/Contents/Info.plist"
plutil -replace LSApplicationCategoryType -string 'public.app-category.productivity' "$APP/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string '0.1.0' "$APP/Contents/Info.plist"
plutil -replace CFBundleVersion -string '10' "$APP/Contents/Info.plist"
mkdir -p "$APP/Contents/Resources/bin"
install -m 644 LICENSE "$APP/Contents/Resources/LICENSE"
install -m 644 README.md "$APP/Contents/Resources/README.md"
install -m 755 dist/multi-backup "$APP/Contents/MacOS/multi-backup"
for tool in age age-keygen gh git-lfs; do
  # Follow Homebrew symlinks: distributable real executables, never shims.
  install -m 755 "$(command -v "$tool")" "$APP/Contents/Resources/bin/$tool"
done

# Fail closed on dependencies referring to a Homebrew or runner-specific path.
for tool in "$APP/Contents/MacOS/multi-backup" "$APP"/Contents/Resources/bin/*; do
  if otool -L "$tool" | awk 'NR>1 {print $1}' | grep -E '^(/opt/homebrew/|/usr/local/)' ; then
    echo "Nonportable dynamic dependency in $tool" >&2; exit 1
  fi
  if [[ "${MAC_UNSIGNED_SMOKE:-0}" != 1 ]]; then
    codesign --force --options runtime --timestamp --keychain "$MULTI_BACKUP_KEYCHAIN" \
      --entitlements packaging/macos/entitlements.plist --sign "$MAC_SIGNING_ID" "$tool"
  else
    codesign --force --sign - "$tool"
  fi
done
# Only sign *after* all embedded executables and resources are in place.
if [[ "${MAC_UNSIGNED_SMOKE:-0}" != 1 ]]; then
  codesign --deep --force --options runtime --timestamp --keychain "$MULTI_BACKUP_KEYCHAIN" \
    --entitlements packaging/macos/entitlements.plist --sign "$MAC_SIGNING_ID" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
else
  codesign --deep --force --sign - "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
  echo 'Ad-hoc signed TEST build only: no Developer ID, no notarization.'
fi
plutil -lint "$APP/Contents/Info.plist"

# Exercise the actual bundled GUI executable and CLI, rather than plain Python.
"$APP/Contents/MacOS/Multi Backup" --self-test
"$APP/Contents/MacOS/multi-backup" --version
"$APP/Contents/Resources/bin/age" --version
"$APP/Contents/Resources/bin/age-keygen" --version
"$APP/Contents/Resources/bin/gh" --version
"$APP/Contents/Resources/bin/git-lfs" version
