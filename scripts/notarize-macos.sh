#!/usr/bin/env bash
# Notarize and staple the signed app, then notarize and staple the final DMG.
set -euo pipefail
: "${MAC_SIGNING_ID:?Missing Developer ID identity}"
: "${APPLE_NOTARY_KEY_ID:?Missing notary API key ID}"
: "${APPLE_NOTARY_ISSUER_ID:?Missing notary API issuer ID}"
: "${MULTI_BACKUP_NOTARY_KEY:?Missing notary API private key file}"
: "${MAC_ARCH:?Missing architecture}"
APP='dist/Multi Backup.app'
DMG="multi-backup-macos-${MAC_ARCH}.dmg"
[[ -d "$APP" ]] || { echo 'No macOS application to notarize' >&2; exit 1; }
TMP="$(mktemp -d "${RUNNER_TEMP:-/tmp}/multi-backup-notarize.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

# Notarize the APP itself, so dragging it into /Applications preserves its ticket.
ditto -c -k --keepParent "$APP" "$TMP/Multi-Backup.app.zip"
xcrun notarytool submit "$TMP/Multi-Backup.app.zip" \
  --key "$MULTI_BACKUP_NOTARY_KEY" --key-id "$APPLE_NOTARY_KEY_ID" \
  --issuer "$APPLE_NOTARY_ISSUER_ID" --wait --timeout 45m --output-format json \
  > "$TMP/app-notarization.json"
python - "$TMP/app-notarization.json" <<'PY'
import json,sys
result=json.load(open(sys.argv[1],encoding='utf-8'))
if result.get('status') != 'Accepted':
    raise SystemExit('Apple app notarization did not return Accepted: '+str(result.get('status')))
print('Apple accepted the application notarization')
PY
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
spctl --assess --type execute --verbose=2 "$APP"

# One and only one visible item in DMG: the signed and notarized Multi Backup.app.
mkdir "$TMP/dmg-root"
ditto "$APP" "$TMP/dmg-root/Multi Backup.app"
[[ "$(find "$TMP/dmg-root" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')" == 1 ]] || {
  echo 'DMG staging has extra user-visible items' >&2; exit 1;
}
hdiutil create -volname 'Multi Backup' -srcfolder "$TMP/dmg-root" \
  -ov -format UDZO "$DMG"
codesign --force --timestamp --sign "$MAC_SIGNING_ID" "$DMG"
xcrun notarytool submit "$DMG" \
  --key "$MULTI_BACKUP_NOTARY_KEY" --key-id "$APPLE_NOTARY_KEY_ID" \
  --issuer "$APPLE_NOTARY_ISSUER_ID" --wait --timeout 45m --output-format json \
  > "$TMP/dmg-notarization.json"
python - "$TMP/dmg-notarization.json" <<'PY'
import json,sys
result=json.load(open(sys.argv[1],encoding='utf-8'))
if result.get('status') != 'Accepted':
    raise SystemExit('Apple DMG notarization did not return Accepted: '+str(result.get('status')))
print('Apple accepted the DMG notarization')
PY
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
hdiutil verify "$DMG"
shasum -a 256 "$DMG" > "checksums-macos-${MAC_ARCH}.txt"
