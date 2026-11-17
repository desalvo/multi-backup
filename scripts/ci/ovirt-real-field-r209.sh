#!/usr/bin/env bash
# oVirt Engine 4.x REAL provider read-only field acceptance.
# No VM snapshot or image transfer is created/deleted by this script.
# Operator must independently prepare an isolated READY backup and an
# already-TRANSFERRING imageio DOWNLOAD transfer. Do not reuse production.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = self-hosted
test "$(id -u)" -ne 0
test "$MB_OVIRT_APPROVAL" = YES_READ_ONLY_DISPOSABLE_OVIRT_BACKUP
test -f /etc/multi-backup-ci/allow-disposable-ovirt
test "$(cat /etc/multi-backup-ci/allow-disposable-ovirt)" = DEDICATED_OVIRT_VM_BACKUP_FIXTURE_ONLY
PROFILE=/etc/multi-backup-ci/ovirt-field/backup-image.json
test -f "$PROFILE" && test ! -L "$PROFILE"
test "$(stat -c '%a' "$PROFILE")" = 600
test "$(stat -c '%u' "$PROFILE")" = "$(id -u)"
NATIVE=rust/target/release/multi-backup-native
test -x "$NATIVE"
W=$(mktemp -d /tmp/mb-ovirt-field.XXXXXXXX)
trap 'rm -rf -- "$W"' EXIT
age-keygen -o "$W/age-key" >/dev/null 2>&1
PUB=$(age-keygen -y "$W/age-key")
"$NATIVE" storage init --root "$W/store" --recipient "$PUB" >/dev/null
"$NATIVE" plugins ovirt-imageio-backup --config "$PROFILE" \
 --root "$W/store" --identity "$W/age-key" --label genuine-ovirt-disposable-vm \
 --json > "$W/receipt.json"
ID=$(jq -er .id "$W/receipt.json")
"$NATIVE" storage verify-chain --root "$W/store" \
 --identity "$W/age-key" --id "$ID" >/dev/null
"$NATIVE" storage restore --root "$W/store" --identity "$W/age-key" \
 --id "$ID" --destination "$W/new-disk-recovery" >/dev/null
DISK_ID=$(jq -er .disk_id "$PROFILE")
FORMAT=$(jq -er '.expected_format|select(.=="raw" or .=="qcow2")' "$PROFILE")
RAW=$(find "$W/new-disk-recovery" -type f -name "$DISK_ID.$FORMAT" -print -quit)
test -n "$RAW"
test "$(stat -c '%s' "$RAW")" = "$(jq -er .expected_bytes "$PROFILE")"
test "$(sha256sum "$RAW" | awk '{print $1}')" = "$(jq -er .expected_sha256 "$PROFILE")"
qemu-img info --output=json "$RAW" | jq -e --arg format "$FORMAT" '.format==$format'
if test "$FORMAT" = qcow2;then qemu-img check "$RAW";fi
if "$NATIVE" storage restore --root "$W/store" --identity "$W/age-key" \
 --id "$ID" --destination "$W/new-disk-recovery" >/dev/null 2>&1;then
 echo 'FAIL oVirt offline recovery overwrote existing disk path' >&2;exit 1
fi
age-keygen -o "$W/wrong-key" >/dev/null 2>&1
if "$NATIVE" storage restore --root "$W/store" --identity "$W/wrong-key" \
 --id "$ID" --destination "$W/unauthorized" >/dev/null 2>&1;then
 echo 'FAIL oVirt accepted unrelated age identity' >&2;exit 1
fi
echo 'PASS real externally prepared oVirt Engine READY VM and IMAGEIO transfer, age encrypted disk, SHA and new qcow2 recovery'
echo 'NOT CERTIFIED oVirt VM creation/boot, multiple disks, OVF, live transfer orchestration, clean guest filesystem or HA'
