#!/usr/bin/env bash
# Disposable real Linux kernel target/initiator acceptance, not a fake block file.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
WORK=$(mktemp -d)
IQN=iqn.2026-11.example.invalid:mb-ci-lun
DEVLINK="/dev/disk/by-path/ip-127.0.0.1:3260-iscsi-$IQN-lun-1"
cleanup(){
 set +e
 sudo iscsiadm -m node -T "$IQN" -p 127.0.0.1:3260 --logout >/dev/null 2>&1 || :
 sudo tgtadm --lld iscsi --mode target --op delete --tid 1 >/dev/null 2>&1 || :
 sudo rm -rf -- "$WORK"
}
trap cleanup EXIT
sudo modprobe iscsi_tcp
sudo systemctl start tgt
sudo dd if=/dev/urandom of="$WORK/lun.img" bs=1M count=8 status=none
sudo tgtadm --lld iscsi --mode target --op new --tid 1 --targetname "$IQN"
sudo tgtadm --lld iscsi --mode logicalunit --op new --tid 1 --lun 1 -b "$WORK/lun.img"
sudo tgtadm --lld iscsi --mode target --op bind --tid 1 -I 127.0.0.1
sudo iscsiadm -m discovery -t sendtargets -p 127.0.0.1:3260
sudo iscsiadm -m node -T "$IQN" -p 127.0.0.1:3260 --login
sudo udevadm settle
for i in $(seq 1 35);do test -L "$DEVLINK" && break;sleep 1;done
test -L "$DEVLINK"
DEV=$(readlink -f "$DEVLINK")
test -b "$DEV"
sudo blockdev --setro "$DEV"
test "$(sudo blockdev --getro "$DEV")" = 1
SERIAL=$(sudo udevadm info --query=property --name "$DEV" | awk -F= '$1=="ID_SERIAL" {print $2; exit}')
test -n "$SERIAL"
HASH=$(sudo sha256sum "$WORK/lun.img" | awk '{print $1}')
SIZE=$(sudo blockdev --getsize64 "$DEV")
test "$SIZE" -eq 8388608
jq -n --arg device "$DEVLINK" --arg iqn "$IQN" --arg serial "$SERIAL" \
 --arg hash "$HASH" --argjson bytes "$SIZE" \
 '{device:$device,target_iqn:$iqn,lun:1,expected_serial:$serial,bytes:$bytes,sha256:$hash}' \
 >"$WORK/source.json"
chmod 0600 "$WORK/source.json"
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT=$(age-keygen -y "$WORK/identity")
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
sudo "$CLI" plugins iscsi-snapshot-backup --snapshot-json "$WORK/source.json" \
 --root "$WORK/store" --identity "$WORK/identity" --label real-iscsi-ro \
 --json >"$WORK/backup.json"
jq -e '.provider=="iscsi" and .encrypted==true and .verified==true and .bytes==8388608' "$WORK/backup.json"
SNAP=$(jq -er .backup_id "$WORK/backup.json")
sudo "$CLI" storage verify-chain --root "$WORK/store" --identity "$WORK/identity" --id "$SNAP"
sudo "$CLI" plugins iscsi-restore-image --root "$WORK/store" --identity "$WORK/identity" \
 --id "$SNAP" --destination "$WORK/recovered" --json >"$WORK/restore.json"
jq -e '.create_only_local_image==true and .iscsi_lun_restore_certified==false' "$WORK/restore.json"
HEX=$(printf '%s' "$IQN" | xxd -p -c 1000)
sudo cmp "$WORK/lun.img" "$WORK/recovered/iscsi/$HEX/lun_1.raw"
if sudo "$CLI" plugins iscsi-restore-image --root "$WORK/store" \
 --identity "$WORK/identity" --id "$SNAP" --destination "$WORK/recovered" >/dev/null 2>&1;then
 echo "FAIL: overwrite accepted" >&2;exit 1
fi
sudo blockdev --setrw "$DEV"
if sudo "$CLI" plugins iscsi-snapshot-backup --snapshot-json "$WORK/source.json" \
 --root "$WORK/store" --identity "$WORK/identity" --label bad-writable >/dev/null 2>&1;then
 echo "FAIL: writable LUN accepted" >&2;exit 1
fi
sudo blockdev --setro "$DEV"
echo "PASS real iSCSI target and kernel initiator, pinned read-only LUN and age backup, offline NEW-image restore, negative tests"
