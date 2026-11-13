#!/usr/bin/env bash
# Early-channel macOS DMG: one ad-hoc signed app, never a stable distribution.
set -euo pipefail
: "${MAC_ARCH:?Specify arm64 or x86_64}"
[[ "$MAC_ARCH" == arm64 || "$MAC_ARCH" == x86_64 ]] || exit 1
[[ "$(uname -m)" == "$MAC_ARCH" ]] || { echo 'Wrong architecture runner' >&2; exit 1; }
APP='dist/Multi Backup.app'
DMG="multi-backup-macos-${MAC_ARCH}.dmg"
[[ -d "$APP" && -x "$APP/Contents/MacOS/Multi Backup" &&
   -x "$APP/Contents/MacOS/multi-backup" ]] || {
  echo 'Missing built GUI application or CLI' >&2; exit 1
}
codesign --verify --deep --strict --verbose=2 "$APP"
SIGNER="$(codesign -dv --verbose=4 "$APP" 2>&1 || true)"
echo "$SIGNER" | grep -q 'Signature=adhoc' || {
  echo 'Refusing TEST image: expected ad-hoc app signature' >&2; exit 1
}
TMP="$(mktemp -d "${RUNNER_TEMP:-/tmp}/multi-backup-test.XXXXXX")"
MOUNT="$TMP/mounted"
cleanup() {
  if [[ -d "$MOUNT" ]] && mount | grep -F "$MOUNT" >/dev/null; then
    hdiutil detach "$MOUNT" -quiet || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT
mkdir "$TMP/root" "$MOUNT" "$TMP/Applications"
ditto "$APP" "$TMP/root/Multi Backup.app"
[[ "$(find "$TMP/root" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" == 1 ]] || exit 1
hdiutil create -volname 'Multi Backup TEST' -srcfolder "$TMP/root" -ov -format UDZO "$DMG"
hdiutil verify "$DMG"
hdiutil attach "$DMG" -readonly -nobrowse -mountpoint "$MOUNT" -quiet
COUNT="$(find "$MOUNT" -mindepth 1 -maxdepth 1 ! -name '.DS_Store' ! -name '.Trashes' ! -name '.fseventsd' | wc -l | tr -d ' ')"
[[ "$COUNT" == 1 && -d "$MOUNT/Multi Backup.app" ]] || {
  echo "TEST image must contain exactly one Multi Backup.app (found $COUNT)" >&2
  find "$MOUNT" -mindepth 1 -maxdepth 1 >&2
  exit 1
}
ditto "$MOUNT/Multi Backup.app" "$TMP/Applications/Multi Backup.app"
INSTALLED="$TMP/Applications/Multi Backup.app"
codesign --verify --deep --strict --verbose=2 "$INSTALLED"
PLIST_EXEC="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$INSTALLED/Contents/Info.plist")"
[[ "$PLIST_EXEC" == 'Multi Backup' ]] || { echo 'Wrong .app entrypoint' >&2; exit 1; }
file "$INSTALLED/Contents/MacOS/Multi Backup" | grep -q "$MAC_ARCH" || {
  echo 'Wrong GUI architecture in TEST app' >&2; exit 1
}
"$INSTALLED/Contents/MacOS/Multi Backup" --self-test
"$INSTALLED/Contents/MacOS/Multi Backup" --gui-smoke
"$INSTALLED/Contents/MacOS/multi-backup" --version
"$INSTALLED/Contents/Resources/bin/age" --version
"$INSTALLED/Contents/Resources/bin/age-keygen" --version
"$INSTALLED/Contents/Resources/bin/gh" --version
"$INSTALLED/Contents/Resources/bin/git-lfs" version
shasum -a 256 "$DMG" > "checksums-macos-${MAC_ARCH}.txt"
echo "PASS: ad-hoc signed TEST-ONLY one-app DMG for ${MAC_ARCH}"
