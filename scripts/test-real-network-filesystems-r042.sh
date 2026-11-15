#!/usr/bin/env bash
# Authentic NFS/Samba/GlusterFS filesystem acceptance on disposable hosted CI.
set -euo pipefail
umask 077
MODE="$1"
case "$MODE" in nfs|smb|glusterfs) ;; *) exit 2;; esac
test "$RUNNER_ENVIRONMENT" = github-hosted || { echo "Refusing to install lab servers outside hosted CI" >&2; exit 1; }
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
for bin in jq age-keygen cmp sha256sum mountpoint findmnt openssl; do command -v "$bin" >/dev/null; done
WORK=$(mktemp -d "$HOME/.mb-r042-fs.XXXXXXXX")
ROOT="$WORK/export"; RO="$WORK/ro"; RW="$WORK/rw"; STORE="$WORK/store"; KEY="$WORK/identity"
VOL="mb$(openssl rand -hex 5)"
EXPORTS="/etc/exports.d/$VOL.exports"
mkdir -p "$ROOT/nested" "$RO" "$RW"
cleanup() {
  set +e
  for m in "$RW" "$RO"; do
    if mountpoint -q "$m"; then sudo umount "$m" || sudo umount -l "$m"; fi
  done
  if [ "$MODE" = nfs ]; then sudo rm -f "$EXPORTS"; sudo exportfs -ra >/dev/null 2>&1; fi
  if [ "$MODE" = glusterfs ]; then
    sudo gluster --mode=script volume stop "$VOL" force >/dev/null 2>&1
    sudo gluster --mode=script volume delete "$VOL" >/dev/null 2>&1
  fi
  if [ "$MODE" = smb ] && [ -f "$WORK/smbd.pid" ]; then
    sudo kill "$(cat "$WORK/smbd.pid")" >/dev/null 2>&1
  fi
  sudo rm -rf -- "$WORK"
}
trap cleanup EXIT
printf 'old version\n' > "$ROOT/nested/data.txt"
printf 'deleted later\n' > "$ROOT/delete.txt"
dd if=/dev/urandom of="$ROOT/payload.bin" bs=1M count=3 status=none
HASH=$(sha256sum "$ROOT/payload.bin" | awk '{print $1}')
case "$MODE" in
nfs)
  sudo modprobe nfsd
  sudo systemctl start rpcbind || true
  sudo systemctl start nfs-server || sudo systemctl start nfs-kernel-server
  sudo mkdir -p /etc/exports.d
  printf '%s 127.0.0.1(rw,sync,no_subtree_check,no_root_squash,insecure)\n' "$ROOT" | sudo tee "$EXPORTS" >/dev/null
  sudo exportfs -ra
  sudo exportfs -v | grep -F "$ROOT" >/dev/null
  mount_ro() { sudo mount -t nfs -o vers=3,proto=tcp,ro "127.0.0.1:$ROOT" "$RO"; }
  mount_rw() { sudo mount -t nfs -o vers=3,proto=tcp,rw "127.0.0.1:$ROOT" "$RW"; }
  expected=nfs
  ;;
smb)
  cat > "$WORK/smb.conf" <<EOF
[global]
  server role = standalone server
  security = user
  map to guest = Never
  passdb backend = tdbsam
  interfaces = lo
  bind interfaces only = yes
  server min protocol = SMB3_00
  smb ports = 445
  pid directory = $WORK
  lock directory = $WORK
  state directory = $WORK
  cache directory = $WORK
  private dir = $WORK
  log file = $WORK/smb-%m.log
  load printers = no
  disable spoolss = yes
[test]
  path = $ROOT
  read only = no
  guest ok = no
  valid users = $(id -un)
  force user = $(id -un)
EOF
  testparm -s "$WORK/smb.conf" >/dev/null
  SMB_PASSWORD=$(openssl rand -hex 24)
  printf '%s\n%s\n' "$SMB_PASSWORD" "$SMB_PASSWORD" | sudo smbpasswd -a -s -c "$WORK/smb.conf" "$(id -un)" >/dev/null
  printf 'username=%s\npassword=%s\n' "$(id -un)" "$SMB_PASSWORD" > "$WORK/smb-credentials"
  chmod 0600 "$WORK/smb-credentials"
  unset SMB_PASSWORD
  sudo smbd -D -s "$WORK/smb.conf"
  for n in $(seq 1 20); do
    if sudo ss -lnt | grep -q '127.0.0.1:445'; then break; fi
    sleep 1
  done
  mount_ro() { sudo mount -t cifs //127.0.0.1/test "$RO" -o "credentials=$WORK/smb-credentials,vers=3.0,ro,noperm,file_mode=0644,dir_mode=0755,uid="$(id -u)",gid=$(id -g)"; }
  mount_rw() { sudo mount -t cifs //127.0.0.1/test "$RW" -o "credentials=$WORK/smb-credentials,vers=3.0,rw,noperm,file_mode=0644,dir_mode=0755,uid="$(id -u)",gid=$(id -g)"; }
  expected=cifs
  ;;
glusterfs)
  sudo systemctl start glusterd
  sudo mkdir -p "$WORK/brick"
  GLUSTER_HOST=$(hostname -f)
  test -n "$GLUSTER_HOST"
  GLUSTER_ADDR=$(getent ahostsv4 "$GLUSTER_HOST" | awk '$1 !~ /^127\\./ {print $1;exit}')
  if [ -z "$GLUSTER_ADDR" ]; then GLUSTER_ADDR=$(hostname -I | awk '{print $1}'); fi
  test -n "$GLUSTER_ADDR"
  sudo gluster --mode=script volume create "$VOL" "$GLUSTER_HOST:$WORK/brick" force
  sudo gluster --mode=script volume start "$VOL"
  sudo gluster volume status "$VOL"
  # Use an actual non-loopback address: cloud-hosted FQDN resolution varies.
  # Retry only while the newly started brick/volfile server initializes.
  gluster_mount() {
    local opts="$1" target="$2"
    for attempt in $(seq 1 12); do
      if sudo mount -t glusterfs -o "$opts" "$GLUSTER_ADDR:/$VOL" "$target"; then return 0; fi
      sleep 2
    done
    echo "FAIL: genuine GlusterFS $opts mount could not be established" >&2
    return 1
  }
  mount_ro() { gluster_mount ro "$RO"; }
  mount_rw() { gluster_mount rw "$RW"; }
  expected=fuse.glusterfs
  mount_rw
  sudo cp -a "$ROOT/." "$RW/"
  sudo rm -rf "$ROOT"
  ROOT="$RW"
  ;;
esac
mount_ro
test "$(findmnt -rn -T "$RO" -o FSTYPE)" = "$expected"
"$CLI" plugins probe --plugin "$MODE" --mount "$RO" --json > "$WORK/mount.json"
test "$(jq -r .fs_type "$WORK/mount.json")" = "$expected"
test "$(jq -r .rw_mount_option "$WORK/mount.json")" = false
# Require actual source readability, not just a plausible fs type.
stat "$RO/nested/data.txt"
head -c 1 "$RO/nested/data.txt" >/dev/null
# An exact mount, but NO assertion of source consistency, must fail.
if "$CLI" plugins backup --input "$MODE" --output posix --source "$RO" \
  --root "$STORE" --identity "$KEY" --label unsafe >/dev/null 2>&1; then
  echo "FAIL: missing consistency attestation was accepted" >&2; exit 1
fi
age-keygen -o "$KEY" >/dev/null 2>&1
RECIPIENT=$(sed -n 's/^# public key: //p' "$KEY" | head -1)
"$CLI" storage init --root "$STORE" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins backup --input "$MODE" --output posix --source "$RO" \
  --root "$STORE" --identity "$KEY" --label full --namespace files \
  --source-snapshot-attested --json > "$WORK/full.json"
FULL=$(jq -er '.id | select(length==64)' "$WORK/full.json")
"$CLI" storage verify --root "$STORE" --identity "$KEY" --id "$FULL" >/dev/null
# Remount the quiescent test fixture before the next point-in-time view.
sudo umount "$RO"
printf 'new version\n' > "$ROOT/nested/data.txt"
rm "$ROOT/delete.txt"
printf 'new file\n' > "$ROOT/add.txt"
sync
mount_ro
"$CLI" plugins backup --input "$MODE" --output posix --source "$RO" \
  --root "$STORE" --identity "$KEY" --label incremental --namespace files \
  --mode incremental --parent "$FULL" --source-snapshot-attested \
  --json > "$WORK/inc.json"
INC=$(jq -er '.id | select(length==64)' "$WORK/inc.json")
test "$INC" != "$FULL"
"$CLI" storage verify-chain --root "$STORE" --identity "$KEY" --id "$INC" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$STORE" \
  --identity "$KEY" --id "$FULL" --destination "$WORK/restored-full" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$STORE" \
  --identity "$KEY" --id "$INC" --destination "$WORK/restored-inc" >/dev/null
cmp <(printf 'old version\n') "$WORK/restored-full/files/nested/data.txt"
cmp <(printf 'new version\n') "$WORK/restored-inc/files/nested/data.txt"
test -e "$WORK/restored-full/files/delete.txt"
test ! -e "$WORK/restored-inc/files/delete.txt"
cmp "$ROOT/add.txt" "$WORK/restored-inc/files/add.txt"
cmp "$ROOT/payload.bin" "$WORK/restored-full/files/payload.bin"
cmp "$ROOT/payload.bin" "$WORK/restored-inc/files/payload.bin"
test "$HASH" = "$(sha256sum "$WORK/restored-inc/files/payload.bin" | awk '{print $1}')"
age-keygen -o "$WORK/wrong-key" >/dev/null 2>&1
if "$CLI" storage verify --root "$STORE" --identity "$WORK/wrong-key" --id "$INC" >/dev/null 2>&1; then
  echo "FAIL: unrelated private key accepted" >&2; exit 1
fi
if "$CLI" plugins restore --input posix --output posix --root "$STORE" \
  --identity "$KEY" --id "$INC" --destination "$WORK/restored-inc" >/dev/null 2>&1; then
  echo "FAIL: existing restore target overwritten" >&2; exit 1
fi
if [ "$MODE" != smb ]; then
  if "$CLI" plugins restore --input posix --output "$MODE" --root "$STORE" \
    --identity "$KEY" --id "$INC" --destination "$RO/no-write" >/dev/null 2>&1; then
    echo "FAIL: restore to read-only mount succeeded" >&2; exit 1
  fi
  if ! mountpoint -q "$RW"; then mount_rw; fi
  "$CLI" plugins restore --input posix --output "$MODE" --root "$STORE" \
    --identity "$KEY" --id "$INC" --destination "$RW/new-recovery" >/dev/null
  cmp "$ROOT/add.txt" "$RW/new-recovery/files/add.txt"
  cmp "$ROOT/payload.bin" "$RW/new-recovery/files/payload.bin"
  if "$CLI" plugins restore --input posix --output "$MODE" --root "$STORE" \
    --identity "$KEY" --id "$INC" --destination "$RW/new-recovery" >/dev/null 2>&1; then
    echo "FAIL: existing remote restore target overwritten" >&2; exit 1
  fi
else
  # CIFS restore direction is NOT yet implemented; fail closed, no fake success.
  if "$CLI" plugins restore --input posix --output smb --root "$STORE" \
    --identity "$KEY" --id "$INC" --destination "$RO/no-write" >/dev/null 2>&1; then
    echo "FAIL: unsupported CIFS restore direction accepted" >&2; exit 1
  fi
fi
echo "PASS: genuine $MODE server+kernel/FUSE client, Rust FULL/INC age encrypted verify-chain and byte-identical recovery"
echo "NOT CERTIFIED: snapshot orchestration, complete ACL/xattr/UID/GID/mtime, network failover or production fencing"
