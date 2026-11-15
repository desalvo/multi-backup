#!/usr/bin/env bash
# Real OpenAFS/Kerberos acceptance, NEVER fake vos/ssh commands.
# Dedicated ephemeral cell only. Requires root-configured VLDB, PTDB, volserver,
# actual Kerberos KDC, AFS admin keytab and AFS client mount.
set -euo pipefail
CELL=example.com
REALM=EXAMPLE.COM
SERVER=$(hostname -I | awk '{print $1}')
VOLUME=test
# Reusable isolated VM: each GitHub job/attempt restores into a NEW AFS volume.
RUN_SUFFIX=$(printf '%s' "$GITHUB_RUN_ID" | tail -c 10)
ATTEMPT=${GITHUB_RUN_ATTEMPT:-1}
case "$ATTEMPT" in *[!0-9]*|'') echo 'Invalid run attempt' >&2; exit 2;; esac
NEW_VOLUME="mbrestore${RUN_SUFFIX}${ATTEMPT}"
test "${#NEW_VOLUME}" -le 22
ROOT_DIR=$(mktemp -d)
umask 077
# Only this run's temporary AFS RW source mount is removed on exit.
# The actual AFS test volume, files, restored volume and keytabs are preserved.
SOURCE_MOUNT=""
SOURCE_MOUNT_CREATED=0
cleanup() {
  rc=$?
  trap - EXIT
  if [ "$SOURCE_MOUNT_CREATED" = 1 ]; then
    if ! fs rmmount -dir "$SOURCE_MOUNT" >/dev/null 2>&1; then
      echo "ERROR: could not remove this run's AFS source mount: $SOURCE_MOUNT" >&2
      [ "$rc" -ne 0 ] || rc=1
    fi
  fi
  rm -rf -- "$ROOT_DIR"
  exit "$rc"
}
trap cleanup EXIT
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
command -v vos
command -v aklog
command -v kinit
command -v fs
command -v age-keygen
test -d /afs/example.com/test
ADMIN_KEYTAB=$(find "$HOME/keytabs" -type f -iname '*admin*' -print -quit)
test -n "$ADMIN_KEYTAB"
# No administrator passwords or KDC secrets are printed.
kinit -k -t "$ADMIN_KEYTAB" "admin@$REALM"
klist -s
aklog -cell "$CELL"
tokens | grep -F "$CELL"
# Real fileserver endpoints and VLDB are mandatory (no simulated binaries).
vos listvol -server "$SERVER" -partition /vicepa -cell "$CELL" | grep -F "$VOLUME"
vos examine "$VOLUME" -cell "$CELL"
# /afs/<cell> selects the released RO path when an RO replica exists.
# The genuine RW root.cell path is /afs/.<cell>. A per-run mount with -rw
# MUST be used for the source volume to seed and update real RW contents.
SOURCE_PARENT="/afs/.$CELL"
test -d "$SOURCE_PARENT" || {
  echo "Missing root.cell read-write mount: $SOURCE_PARENT" >&2
  exit 1
}
# The test cell is TEST-ONLY. The upstream playbook grants broad read ACLs
# on root.cell; even an authenticated AFS admin then lacks the i (insert)
# right required by fs mkmount. OpenAFS requires BOTH i and a on the parent.
# Grant those rights solely to the lab's system:administrators group. This is
# additive, idempotent, and restricted to the isolated example.com cell.
test "$CELL" = example.com
test "$SOURCE_PARENT" = /afs/.example.com
echo 'AFS RW parent ACL before lab-only administrator repair:'
fs listacl -path "$SOURCE_PARENT"
echo 'AFS caller rights before repair:'
fs getcalleraccess -path "$SOURCE_PARENT"
fs setacl -dir "$SOURCE_PARENT" -acl system:administrators all
echo 'AFS caller rights after lab-only administrator repair:'
fs getcalleraccess -path "$SOURCE_PARENT"
fs listacl -path "$SOURCE_PARENT"
SOURCE_MOUNT="$SOURCE_PARENT/mb-source-$RUN_SUFFIX-$ATTEMPT"
if [ -e "$SOURCE_MOUNT" ]; then
  echo "Refusing to reuse existing AFS mountpoint $SOURCE_MOUNT" >&2
  exit 1
fi
fs mkmount -dir "$SOURCE_MOUNT" -vol "$VOLUME" -cell "$CELL" -rw
SOURCE_MOUNT_CREATED=1
fs lsmount -dir "$SOURCE_MOUNT"
BASE="$SOURCE_MOUNT"
test -d "$BASE"
printf 'OpenAFS real-cell integration, source data\n' > "$BASE/mb-backup-data.txt"
printf 'AFS full payload %s\n' "$(date -u +%s)" >> "$BASE/mb-backup-data.txt"
SINCE=$(date -u +'%m/%d/%Y %H:%M')
age-keygen -o "$ROOT_DIR/identity" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$ROOT_DIR/identity" | head -1)
test -n "$RECIPIENT"
mkdir -p "$ROOT_DIR/store"
"$CLI" storage init --root "$ROOT_DIR/store" --recipient "$RECIPIENT"
printf '{"id":"real-afs-lab","cell":"%s","volume":"%s","auth":{"type":"existing_token"},"execution":{"type":"local"},"mode":{"kind":"full"}}\n' \
  "$CELL" "$VOLUME" > "$ROOT_DIR/job.json"
"$CLI" plugins afs-backup --job-json "$ROOT_DIR/job.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --label real-afs-full --json > "$ROOT_DIR/full.json"
FULL_ID=$(jq -r '.id' "$ROOT_DIR/full.json")
test "$FULL_ID" != null
"$CLI" storage verify --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --id "$FULL_ID"
# Incremental uses a wall-clock anchor before the full. Cross a minute boundary
# to guarantee that the subsequent modifications are within the incremental.
sleep 65
printf 'AFS incremental payload %s\n' "$(date -u +%s)" >> "$BASE/mb-backup-data.txt"
printf 'new-file-after-full\n' > "$BASE/mb-new-after-full.txt"
jq -n --arg cell "$CELL" --arg vol "$VOLUME" --arg since "$SINCE" --arg parent "$FULL_ID" \
  '{id:"real-afs-lab",cell:$cell,volume:$vol,auth:{type:"existing_token"},execution:{type:"local"},mode:{kind:"incremental",since:$since,parent_backup_id:$parent}}' > "$ROOT_DIR/inc-job.json"
"$CLI" plugins afs-backup --job-json "$ROOT_DIR/inc-job.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --label real-afs-incremental --json > "$ROOT_DIR/inc.json"
INC_ID=$(jq -r '.id' "$ROOT_DIR/inc.json")
test "$INC_ID" != null
"$CLI" storage verify-chain --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --id "$INC_ID"
jq -n --arg cell "$CELL" --arg vol "$NEW_VOLUME" --arg server "$SERVER" \
  '{cell:$cell,volume:$vol,server:$server,partition:"/vicepa",auth:{type:"existing_token"}}' > "$ROOT_DIR/target.json"
if "$CLI" plugins afs-restore-new --target-json "$ROOT_DIR/target.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --id "$INC_ID" --confirm-target wrong; then
    echo "SECURITY FAIL: wrong AFS confirmation was accepted" >&2; exit 1
fi
CONFIRM="$CELL:$SERVER:/vicepa:$NEW_VOLUME"
"$CLI" plugins afs-restore-new --target-json "$ROOT_DIR/target.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --id "$INC_ID" \
  --confirm-target "$CONFIRM" --json > "$ROOT_DIR/restore.json"
test "$(jq -r '.restored_dumps' "$ROOT_DIR/restore.json")" = 2
vos examine "$NEW_VOLUME" -cell "$CELL" | grep -F "$NEW_VOLUME"
# Use the RW root.cell directory, not /afs/<cell>, which can be RO.
# This target has a new volume name per run+attempt; no existing mount is reused.
RESTORE_MOUNT="$SOURCE_PARENT/$NEW_VOLUME"
if [ -e "$RESTORE_MOUNT" ]; then
  echo "Refusing to overwrite existing AFS restore mount: $RESTORE_MOUNT" >&2
  exit 1
fi
fs mkmount -dir "$RESTORE_MOUNT" -vol "$NEW_VOLUME" -cell "$CELL" -rw
fs lsmount -dir "$RESTORE_MOUNT"
cmp "$BASE/mb-backup-data.txt" "$RESTORE_MOUNT/mb-backup-data.txt"
cmp "$BASE/mb-new-after-full.txt" "$RESTORE_MOUNT/mb-new-after-full.txt"
if "$CLI" plugins afs-restore-new --target-json "$ROOT_DIR/target.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --id "$INC_ID" \
  --confirm-target "$CONFIRM"; then
    echo "SECURITY FAIL: existing AFS volume overwritten" >&2; exit 1
fi
cmp "$BASE/mb-backup-data.txt" "$RESTORE_MOUNT/mb-backup-data.txt"
# Prove that invalid real VLDB volume returns nonzero and publishes no backup.
jq '.volume="volume_does_not_exist_987654"' "$ROOT_DIR/job.json" > "$ROOT_DIR/missing.json"
BEFORE=$("$CLI" storage list --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --json | jq 'length')
if "$CLI" plugins afs-backup --job-json "$ROOT_DIR/missing.json" \
  --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --label must-fail; then
    echo "SECURITY FAIL: invalid real vos source published backup" >&2; exit 1
fi
AFTER=$("$CLI" storage list --root "$ROOT_DIR/store" --identity "$ROOT_DIR/identity" --json | jq 'length')
test "$BEFORE" = "$AFTER"
echo "PASS: authentic KDC + AFS token + VLDB/volserver + encrypted full/incremental + isolated byte-identical new-volume restore"
