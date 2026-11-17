#!/usr/bin/env bash
# Real OpenSSH ssh/scp/sftp client+sshd acceptance; SSHFS uses real FUSE.
# Server binds loopback only with ephemeral host/client keys and no passwords.
set -euo pipefail
umask 077
MODE="$1"
test "$MODE" = protocol || test "$MODE" = sshfs
# OpenSSH StrictModes rejects authorized_keys below world-writable /tmp.
# Keep the disposable key hierarchy under the actual user's trusted home.
WORK=$(mktemp -d "$HOME/.mb-ssh-ci.XXXXXX")
NAME=$(id -un)
PORT=22223
PID=""
cleanup(){
    for mount in "$WORK/mount-ro" "$WORK/mount-rw"; do
        if mountpoint -q "$mount"; then fusermount3 -uz "$mount" || fusermount -uz "$mount" || :; fi
    done
    if [ -n "$PID" ]; then sudo kill "$PID" >/dev/null 2>&1 || :; fi
    rm -rf "$WORK"
}
trap cleanup EXIT
mkdir -p "$WORK/remote" "$WORK/remote-rw" "$WORK/mount-ro" "$WORK/mount-rw"
ssh-keygen -q -t ed25519 -N '' -f "$WORK/server_key" >/dev/null
ssh-keygen -q -t ed25519 -N '' -f "$WORK/client_key" >/dev/null
ssh-keygen -q -t ed25519 -N '' -f "$WORK/wrong_key" >/dev/null
cp "$WORK/client_key.pub" "$WORK/authorized_keys"
chmod 600 "$WORK/authorized_keys"
printf '[127.0.0.1]:%s %s\n' "$PORT" \
    "$(ssh-keygen -y -f "$WORK/server_key")" > "$WORK/known_hosts"
cat > "$WORK/sshd_config" <<EOF
Port $PORT
AddressFamily inet
ListenAddress 127.0.0.1
HostKey $WORK/server_key
PidFile $WORK/sshd.pid
AuthorizedKeysFile $WORK/authorized_keys
PermitRootLogin no
AllowUsers $NAME
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AuthenticationMethods publickey
StrictModes yes
UsePAM yes
X11Forwarding no
AllowTcpForwarding no
AllowAgentForwarding no
PermitTunnel no
PermitTTY no
Subsystem sftp internal-sftp
LogLevel VERBOSE
EOF
cat > "$WORK/ssh_config" <<EOF
Host mbtest
    HostName 127.0.0.1
    Port $PORT
    User $NAME
    IdentityFile $WORK/client_key
    IdentitiesOnly yes
    StrictHostKeyChecking yes
    UserKnownHostsFile $WORK/known_hosts
    GlobalKnownHostsFile /dev/null
    PasswordAuthentication no
    KbdInteractiveAuthentication no
    BatchMode yes
    ConnectTimeout 8
    LogLevel ERROR
EOF
sudo mkdir -p /run/sshd
sudo /usr/sbin/sshd -t -f "$WORK/sshd_config"
sudo /usr/sbin/sshd -f "$WORK/sshd_config" -E "$WORK/sshd.log"
PID=$(cat "$WORK/sshd.pid")
for n in $(seq 1 40); do
    if ssh -F "$WORK/ssh_config" mbtest "true" >/dev/null 2>&1; then break; fi
    sleep 1
done
if ! ssh -F "$WORK/ssh_config" mbtest 'true'; then
    echo 'OpenSSH test KDC/account/authorized_keys failure:' >&2
    sudo tail -80 "$WORK/sshd.log" >&2 || :
    exit 1
fi
# Every path uses a real sshd process and mandatory pinned ED25519 host key.
printf 'SSH/SCP/SFTP protocol binary data \x00\xff\x7f\n' > "$WORK/source.dat"
dd if=/dev/urandom of="$WORK/random.dat" bs=1M count=2 status=none
ssh -F "$WORK/ssh_config" mbtest "cat > $WORK/remote/ssh-stdout.dat" < "$WORK/source.dat"
cmp "$WORK/source.dat" "$WORK/remote/ssh-stdout.dat"
scp -F "$WORK/ssh_config" "$WORK/random.dat" "mbtest:$WORK/remote/scp-sftp.dat"
cmp "$WORK/random.dat" "$WORK/remote/scp-sftp.dat"
# SCP legacy protocol is tested independently from modern SCP-over-SFTP.
scp -O -F "$WORK/ssh_config" "$WORK/source.dat" "mbtest:$WORK/remote/scp-legacy.dat"
cmp "$WORK/source.dat" "$WORK/remote/scp-legacy.dat"
cat > "$WORK/sftp.batch" <<EOF
put $WORK/source.dat $WORK/remote/sftp-upload.dat
get $WORK/remote/scp-sftp.dat $WORK/sftp-downloaded.dat
EOF
sftp -F "$WORK/ssh_config" -b "$WORK/sftp.batch" mbtest
cmp "$WORK/source.dat" "$WORK/remote/sftp-upload.dat"
cmp "$WORK/random.dat" "$WORK/sftp-downloaded.dat"
# Unknown or spoofed host key MUST fail before authentication.
printf '[127.0.0.1]:%s %s\n' "$PORT" \
    "$(ssh-keygen -y -f "$WORK/wrong_key")" > "$WORK/wrong_hosts"
sed "s|UserKnownHostsFile $WORK/known_hosts|UserKnownHostsFile $WORK/wrong_hosts|" \
    "$WORK/ssh_config" > "$WORK/bad_host_config"
if ssh -F "$WORK/bad_host_config" mbtest true >/dev/null 2>&1; then
    echo 'FAIL: spoofed SSH host trusted' >&2; exit 1
fi
# Unauthorized ED25519 client key also fails closed.
sed "s|IdentityFile $WORK/client_key|IdentityFile $WORK/wrong_key|" \
    "$WORK/ssh_config" > "$WORK/bad_client_config"
if ssh -F "$WORK/bad_client_config" mbtest true >/dev/null 2>&1; then
    echo 'FAIL: unauthorized SSH key accepted' >&2; exit 1
fi
if [ "$MODE" = protocol ]; then
    # B-In uses exactly the verified SSH server and pinned host credentials.
    CLI=rust/target/debug/multi-backup-native
    test -x "$CLI"
    age-keygen -o "$WORK/stream-age-identity" >/dev/null 2>&1
    RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/stream-age-identity" | head -1)
    "$CLI" storage init --root "$WORK/stream-store" --recipient "$RECIPIENT"
    REMOTE_HASH="$(sha256sum "$WORK/remote/scp-sftp.dat" | awk '{print $1}')"
    REMOTE_BYTES="$(stat -c %s "$WORK/remote/scp-sftp.dat")"
    export WORK NAME PORT REMOTE_HASH REMOTE_BYTES
    python3 - <<'PY'
import os,json
root=os.environ["WORK"]
cfg={
    "host":"127.0.0.1","port":int(os.environ["PORT"]),
    "user":os.environ["NAME"],
    "identity_file":root+"/client_key",
    "known_hosts_file":root+"/known_hosts",
    "remote_file":root+"/remote/scp-sftp.dat",
    "expected_sha256":os.environ["REMOTE_HASH"],
    "expected_bytes":int(os.environ["REMOTE_BYTES"])
}
with open(root+"/ssh-source.json","x") as handle: json.dump(cfg,handle)
os.chmod(root+"/ssh-source.json",0o600)
PY
    # The standalone Rust adapter uses exactly this pinned OpenSSH call,
    # not the workflow's SSH alias configuration.
    ssh -T -p "$PORT" -i "$WORK/client_key" \
       -o BatchMode=yes -o StrictHostKeyChecking=yes \
       -o IdentitiesOnly=yes -o PreferredAuthentications=publickey \
       -o PasswordAuthentication=no \
       -o "UserKnownHostsFile=$WORK/known_hosts" \
       -o GlobalKnownHostsFile=/dev/null \
       "$NAME@127.0.0.1" "cat -- $WORK/remote/scp-sftp.dat" > "$WORK/direct-ssh.dat"
    cmp "$WORK/direct-ssh.dat" "$WORK/remote/scp-sftp.dat"
    "$CLI" plugins ssh-backup --config "$WORK/ssh-source.json" \
      --root "$WORK/stream-store" --identity "$WORK/stream-age-identity" \
      --label "Pinned genuine OpenSSH B-In" --json > "$WORK/ssh-import.json"
    SSH_IMPORT_ID="$(jq -er .id "$WORK/ssh-import.json")"
    "$CLI" storage restore --root "$WORK/stream-store" \
      --identity "$WORK/stream-age-identity" --id "$SSH_IMPORT_ID" \
      --destination "$WORK/ssh-recovered"
    cmp "$WORK/remote/scp-sftp.dat" \
      "$WORK/ssh-recovered/ssh/127.0.0.1/$NAME/${WORK#/}/remote/scp-sftp.dat"
    # Supplying an untrusted digest must never publish a valid backup.
    python3 - "$WORK/ssh-source.json" <<'PY'
import sys,json
p=sys.argv[1]
with open(p) as f: c=json.load(f)
c["expected_sha256"]="0"*64
with open(p,"w") as f: json.dump(c,f)
PY
    if "$CLI" plugins ssh-backup --config "$WORK/ssh-source.json" \
      --root "$WORK/stream-store" --identity "$WORK/stream-age-identity" \
      --label "Refuse bad digest" >/dev/null 2>&1;then
      echo "FAIL: OpenSSH B-In accepted wrong independently pinned SHA256" >&2;exit 1
    fi
    echo "PASS genuine pinned SSH B-In, age chunked recovery and SHA tamper rejection"
    echo "PASS real SSH stdout, SCP(SFTP default), legacy SCP, explicit SFTP; pinned host and unauthorized client refused"
    exit 0
fi
# This phase MUST use a true kernel FUSE SSHFS mount; no local-path substitute.
command -v sshfs
command -v fusermount3
test -c /dev/fuse || { echo 'Real SSHFS requires /dev/fuse or a dedicated capable runner' >&2; exit 1; }
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
age-keygen -o "$WORK/identity" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
# Source is quiescent during this fixture; source snapshot is attested only
# for these test files. An arbitrary live remote tree is not snapshot-safe.
sshfs -o ro -o "ssh_command=ssh -F $WORK/ssh_config" \
    "mbtest:$WORK/remote" "$WORK/mount-ro"
mountpoint -q "$WORK/mount-ro"
"$CLI" plugins probe --plugin sshfs --mount "$WORK/mount-ro" --json > "$WORK/ro-probe.json"
test "$(jq -r .fs_type "$WORK/ro-probe.json")" = fuse.sshfs
test "$(jq -r .rw_mount_option "$WORK/ro-probe.json")" = false
if "$CLI" plugins backup --input sshfs --output posix --source "$WORK/mount-ro" \
    --root "$WORK/store" --identity "$WORK/identity" --label no-attestation; then
    echo 'FAIL: unapproved SSHFS snapshot accepted' >&2; exit 1
fi
"$CLI" plugins backup --input sshfs --output posix \
    --source "$WORK/mount-ro" --root "$WORK/store" --identity "$WORK/identity" \
    --source-snapshot-attested --namespace sshfs --label real-sshfs-ro --json > "$WORK/backup.json"
ID=$(jq -r .id "$WORK/backup.json")
"$CLI" storage verify --root "$WORK/store" --identity "$WORK/identity" --id "$ID"
"$CLI" plugins restore --input posix --output posix \
    --root "$WORK/store" --identity "$WORK/identity" --id "$ID" \
    --destination "$WORK/recovered-local" --json
for name in ssh-stdout.dat scp-sftp.dat scp-legacy.dat sftp-upload.dat; do
    cmp "$WORK/remote/$name" "$WORK/recovered-local/sshfs/$name"
done
# Restore cannot write to mounted read-only SSHFS source.
if "$CLI" plugins restore --input posix --output sshfs --root "$WORK/store" \
    --identity "$WORK/identity" --id "$ID" \
    --destination "$WORK/mount-ro/forbidden"; then
    echo 'FAIL: read-only SSHFS accepted as restore target' >&2; exit 1
fi
sshfs -o rw -o "ssh_command=ssh -F $WORK/ssh_config" \
    "mbtest:$WORK/remote-rw" "$WORK/mount-rw"
mountpoint -q "$WORK/mount-rw"
"$CLI" plugins probe --plugin sshfs --mount "$WORK/mount-rw" --json > "$WORK/rw-probe.json"
test "$(jq -r .rw_mount_option "$WORK/rw-probe.json")" = true
"$CLI" plugins restore --input posix --output sshfs --root "$WORK/store" \
    --identity "$WORK/identity" --id "$ID" \
    --destination "$WORK/mount-rw/new-recovery" --json
cmp "$WORK/remote/ssh-stdout.dat" "$WORK/remote-rw/new-recovery/sshfs/ssh-stdout.dat"
cmp "$WORK/remote/scp-sftp.dat" "$WORK/remote-rw/new-recovery/sshfs/scp-sftp.dat"
if "$CLI" plugins restore --input posix --output sshfs --root "$WORK/store" \
    --identity "$WORK/identity" --id "$ID" \
    --destination "$WORK/mount-rw/new-recovery"; then
    echo 'FAIL: SSHFS restore overwrote existing target' >&2; exit 1
fi
# r183: genuine SSHFS RW mount B-Out with ciphertext and remote age readback.
mkdir -p "$WORK/mounted-output-input/nested"
printf 'authenticated remote SSHFS age B-Out\n' > "$WORK/mounted-output-input/nested/hello.txt"
"$CLI" plugins mounted-backup --output sshfs \
  --mount "$WORK/mount-rw" --source "$WORK/mounted-output-input" \
  --filename verified-sshfs.tar.gz.age --recipient "$RECIPIENT" \
  --identity "$WORK/identity" --json > "$WORK/mounted-output-receipt.json"
jq -e '.create_only == true and .encrypted == true and (.verified_members == .source_members)' \
  "$WORK/mounted-output-receipt.json"
test -f "$WORK/remote-rw/verified-sshfs.tar.gz.age"
test -f "$WORK/remote-rw/verified-sshfs.tar.gz.age.sha256"
"$CLI" plugins mounted-restore --input sshfs \
  --mount "$WORK/mount-rw" --filename verified-sshfs.tar.gz.age \
  --identity "$WORK/identity" --destination "$WORK/mounted-sshfs-recovered"
cmp "$WORK/mounted-output-input/nested/hello.txt" \
  "$WORK/mounted-sshfs-recovered/multi-backup/mounted-output/nested/hello.txt"
if "$CLI" plugins mounted-backup --output sshfs \
  --mount "$WORK/mount-rw" --source "$WORK/mounted-output-input" \
  --filename verified-sshfs.tar.gz.age --recipient "$RECIPIENT" \
  --identity "$WORK/identity" >/dev/null 2>&1; then
    echo 'FAIL SSHFS B-Out overwrote an existing encrypted archive' >&2; exit 1
fi
echo 'PASS genuine SSHFS B-Out encrypted archive, remote readback and create-only age recovery'

echo "PASS genuine FUSE SSHFS RO source -> Rust age store -> isolated RW SSHFS restore, strict mount type and no overwrite"
