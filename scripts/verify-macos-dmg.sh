#!/usr/bin/env bash
# Acceptance test the *downloadable* disk image on its native architecture.
set -euo pipefail
: "${MAC_ARCH:?Missing architecture}"
DMG="${1:?Expected DMG file}"
[[ -f "$DMG" ]] || exit 1
TMP="$(mktemp -d "${RUNNER_TEMP:-/tmp}/multi-backup-verify.XXXXXX")"
MOUNT="$TMP/mounted"
cleanup() {
  if mount | grep -F "$MOUNT" >/dev/null; then
    hdiutil detach "$MOUNT" -quiet || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT
mkdir "$MOUNT" "$TMP/Applications"
hdiutil attach "$DMG" -readonly -nobrowse -mountpoint "$MOUNT" -quiet
# Exactly two visible items: real application and the standard Applications link.
COUNT="$(find "$MOUNT" -mindepth 1 -maxdepth 1 ! -name '.DS_Store' ! -name '.Trashes' ! -name '.fseventsd' | wc -l | tr -d ' ')"
[[ "$COUNT" == 2 && -d "$MOUNT/Multi Backup.app" &&
   -L "$MOUNT/Applications" && "$(readlink "$MOUNT/Applications")" == /Applications ]] || {
  echo "DMG must contain Multi Backup.app and Applications -> /Applications (found $COUNT entries)" >&2
  find "$MOUNT" -mindepth 1 -maxdepth 1 -print >&2
  exit 1
}
ditto "$MOUNT/Multi Backup.app" "$TMP/Applications/Multi Backup.app"
APP="$TMP/Applications/Multi Backup.app"
[[ -x "$APP/Contents/MacOS/Multi Backup" && -x "$APP/Contents/MacOS/multi-backup" ]] || {
  echo 'Missing GUI or embedded CLI binary' >&2; exit 1
}
PLIST_EXEC="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
[[ "$PLIST_EXEC" == 'Multi Backup' ]] || { echo 'Wrong app bundle entrypoint' >&2; exit 1; }
file "$APP/Contents/MacOS/Multi Backup" | grep -q "$MAC_ARCH" || {
  echo 'Wrong app executable architecture' >&2; exit 1;
}
codesign --verify --deep --strict --verbose=2 "$APP"
xcrun stapler validate "$APP"
spctl --assess --type execute --verbose=2 "$APP"
"$APP/Contents/MacOS/Multi Backup" --self-test
"$APP/Contents/MacOS/Multi Backup" --gui-smoke
"$APP/Contents/MacOS/multi-backup" --version
"$APP/Contents/Resources/bin/age" --version
printf 'Signed, notarized, installable app verified after extracting DMG (%s)\n' "$MAC_ARCH"
