#!/usr/bin/env bash
# Executes only in the disposable *real* 3-OSD Rook cluster client pod.
set -Eeuo pipefail
umask 077
CLI=/usr/local/bin/multi-backup-native
for bin in ceph rbd age-keygen jq sha256sum cmp;do command -v "$bin" >/dev/null;done
D="$(mktemp -d /tmp/mb-rbd-r195.XXXXXXXX)"
trap 'rm -rf "$D"' EXIT
POOL=mb-ci-rbd
ceph osd pool create "$POOL" 32 >/dev/null
rbd pool init "$POOL"
dd if=/dev/urandom of="$D/source.raw" bs=1M count=2 status=none
rbd import --image-format 2 "$D/source.raw" "$POOL/source" >/dev/null
rbd snap create "$POOL/source@frozen"
SNAP="$(rbd snap ls --format json "$POOL/source" | jq -er '.[] | select(.name=="frozen") | .id')"
test "$SNAP" -gt 0
SIZE="$(wc -c < "$D/source.raw" | tr -d ' ')"
HASH="$(sha256sum "$D/source.raw" | awk '{print $1}')"
rbd resize --size 4M "$POOL/source" >/dev/null
rbd export --no-progress "$POOL/source@frozen" "$D/pinned.raw" >/dev/null
cmp "$D/source.raw" "$D/pinned.raw"
jq -n --arg pool "$POOL" --arg hash "$HASH" --argjson size "$SIZE" \
  --argjson snap "$SNAP" \
  '{pool:$pool,image:"source",snapshot:"frozen",snapshot_id:$snap,bytes:$size,sha256:$hash}' > "$D/snapshot.json"
chmod 0600 "$D/snapshot.json"
age-keygen -o "$D/identity" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$D/identity")"
"$CLI" storage init --root "$D/store" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins rbd-snapshot-backup --snapshot-json "$D/snapshot.json" \
  --root "$D/store" --identity "$D/identity" --label real-rbd-snapshot \
  --json > "$D/backup.json"
jq -e '.provider=="rbd" and .encrypted and .bytes == 2097152' "$D/backup.json" >/dev/null
ID="$(jq -er .backup_id "$D/backup.json")"
"$CLI" storage verify-chain --root "$D/store" --identity "$D/identity" --id "$ID" >/dev/null
"$CLI" plugins rbd-restore-new --root "$D/store" --identity "$D/identity" \
  --id "$ID" --pool "$POOL" --image recovered-new \
  --confirm-new-image "$POOL/recovered-new" --json > "$D/restore.json"
jq -e '.provider=="rbd" and .created_new_image and .bytes==2097152' "$D/restore.json" >/dev/null
rbd export --no-progress "$POOL/recovered-new" "$D/recovered.raw" >/dev/null
cmp "$D/source.raw" "$D/recovered.raw"
if "$CLI" plugins rbd-restore-new --root "$D/store" --identity "$D/identity" \
  --id "$ID" --pool "$POOL" --image recovered-new \
  --confirm-new-image "$POOL/recovered-new" >/dev/null 2>&1;then
  echo 'FAIL: RBD allowed replacing existing output image' >&2;exit 1
fi
jq '.sha256="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' \
  "$D/snapshot.json" > "$D/bad-snapshot.json"
chmod 0600 "$D/bad-snapshot.json"
if "$CLI" plugins rbd-snapshot-backup --snapshot-json "$D/bad-snapshot.json" \
  --root "$D/store" --identity "$D/identity" --label incorrect-sha >/dev/null 2>&1;then
  echo 'FAIL: RBD accepted the wrong independently pinned SHA256' >&2;exit 1
fi
echo 'PASS real RBD snapshot ID+hash pinned Rust age B-In, new-image import and independent RBD export restore with negative tests'
