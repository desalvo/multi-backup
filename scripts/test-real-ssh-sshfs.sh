#!/usr/bin/env bash
# Real OpenSSH ssh/scp/sftp client+sshd acceptance; SSHFS uses real FUSE.
# Server binds loopback only with ephemeral host/client keys and no passwords.
set -euo pipefail
umask 077
MODE="$1"
test "$MODE" = protocol || test "$MODE" = sshfs
WORK=$(mktemp -d)
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
UsePAM no
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
ssh -F "$WORK/ssh_config" mbtest 'true'
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
echo "PASS genuine FUSE SSHFS RO source -> Rust age store -> isolated RW SSHFS restore, strict mount type and no overwrite"
