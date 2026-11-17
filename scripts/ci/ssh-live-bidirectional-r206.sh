#!/usr/bin/env bash
# Genuine OpenSSH server + SSH native B-In/B-Out; disposable loopback fixture.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
W=$(mktemp -d /tmp/mb-ssh-real.XXXXXXXX)
PORT=22999
cleanup(){
 set +e
 if test -r "$W/sshd.pid";then sudo kill "$(cat "$W/sshd.pid")" >/dev/null 2>&1 || :;fi
 sudo rm -rf -- "$W"
}
trap cleanup EXIT
ssh-keygen -q -t ed25519 -N '' -f "$W/client"
ssh-keygen -q -t ed25519 -N '' -f "$W/host"
cp "$W/client.pub" "$W/authorized_keys"
chmod 0600 "$W/client" "$W/host" "$W/authorized_keys"
mkdir -p "$W/remote" "$W/source/nested"
printf 'Native SSH backup confidential input\n' > "$W/source/nested/secret.txt"
dd if=/dev/urandom of="$W/source/blob.bin" bs=1024 count=512 status=none
cp "$W/source/blob.bin" "$W/remote-input.bin"
DATA_SHA=$(sha256sum "$W/remote-input.bin" | awk '{print $1}')
DATA_BYTES=$(wc -c < "$W/remote-input.bin")
RUNNER_NAME=$(id -un)
cat >"$W/sshd.conf" <<EOF
Port $PORT
ListenAddress 127.0.0.1
HostKey $W/host
PidFile $W/sshd.pid
AuthorizedKeysFile $W/authorized_keys
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
StrictModes no
PermitRootLogin no
AllowUsers $RUNNER_NAME
UsePAM no
Subsystem sftp internal-sftp
LogLevel ERROR
EOF
sudo mkdir -p /run/sshd
sudo /usr/sbin/sshd -f "$W/sshd.conf" -E "$W/sshd.log"
printf '[127.0.0.1]:%s %s\n' "$PORT" "$(ssh-keygen -y -f "$W/host")" > "$W/known_hosts"
chmod 0600 "$W/known_hosts"
for attempt in $(seq 1 20);do
 if ssh -T -o BatchMode=yes -o StrictHostKeyChecking=yes \
   -o UserKnownHostsFile="$W/known_hosts" -o GlobalKnownHostsFile=/dev/null \
   -o IdentitiesOnly=yes -i "$W/client" -p "$PORT" \
   "$RUNNER_NAME@127.0.0.1" "test -d $W/remote" >/dev/null 2>&1;then break;fi
 sleep 1
done
ssh -T -o BatchMode=yes -o StrictHostKeyChecking=yes \
 -o UserKnownHostsFile="$W/known_hosts" -o GlobalKnownHostsFile=/dev/null \
 -o IdentitiesOnly=yes -i "$W/client" -p "$PORT" \
 "$RUNNER_NAME@127.0.0.1" "test -d $W/remote"
jq -n --arg host 127.0.0.1 --arg user "$RUNNER_NAME" \
 --arg key "$W/client" --arg known "$W/known_hosts" \
 --arg dir "$W/remote" --argjson port "$PORT" \
 '{host:$host,user:$user,port:$port,identity_file:$key,known_hosts_file:$known,remote_directory:$dir}' \
 >"$W/ssh-output.json"
age-keygen -o "$W/agekey" >/dev/null 2>&1
RECIPIENT=$(age-keygen -y "$W/agekey")
"$CLI" plugins ssh-output-backup --config "$W/ssh-output.json" \
 --source "$W/source" --filename job-0001.tar.gz.age \
 --recipient "$RECIPIENT" --identity "$W/agekey" --json >"$W/ssh-bout.json"
jq -e '.provider=="ssh" and .encrypted and .create_only and .independent_remote_readback and .restored_members>0' "$W/ssh-bout.json"
test -s "$W/remote/job-0001/job-0001.tar.gz.age"
test -s "$W/remote/job-0001/job-0001.tar.gz.age.sha256"
test "$(sha256sum "$W/remote/job-0001/job-0001.tar.gz.age" | awk '{print $1}')" = "$(jq -r .sha256 "$W/ssh-bout.json")"
"$CLI" plugins ssh-output-restore --config "$W/ssh-output.json" \
 --filename job-0001.tar.gz.age --identity "$W/agekey" \
 --destination "$W/recovered" --json >"$W/ssh-restore.json"
jq -e '.provider=="ssh" and .restored_members>0 and .verified' "$W/ssh-restore.json"
cmp "$W/source/blob.bin" "$W/recovered/multi-backup/ssh-native/blob.bin"
cmp "$W/source/nested/secret.txt" "$W/recovered/multi-backup/ssh-native/nested/secret.txt"
if "$CLI" plugins ssh-output-backup --config "$W/ssh-output.json" \
 --source "$W/source" --filename job-0001.tar.gz.age \
 --recipient "$RECIPIENT" --identity "$W/agekey" >/dev/null 2>&1;then
 echo 'FAIL SSH remote ciphertext overwritten' >&2;exit 1
fi
if "$CLI" plugins ssh-output-restore --config "$W/ssh-output.json" \
 --filename job-0001.tar.gz.age --identity "$W/agekey" \
 --destination "$W/recovered" >/dev/null 2>&1;then
 echo 'FAIL SSH local restore overwritten' >&2;exit 1
fi
ssh-keygen -q -t ed25519 -N '' -f "$W/untrusted"
cp "$W/untrusted.pub" "$W/wrong-known"
chmod 0600 "$W/wrong-known"
jq --arg key "$W/wrong-known" '.known_hosts_file=$key' \
 "$W/ssh-output.json" > "$W/invalid-config.json"
if "$CLI" plugins ssh-output-restore --config "$W/invalid-config.json" \
 --filename job-0001.tar.gz.age --identity "$W/agekey" \
 --destination "$W/not-created" >/dev/null 2>&1;then
 echo 'FAIL SSH accepted untrusted host key' >&2;exit 1
fi
jq -n --arg host 127.0.0.1 --arg user "$RUNNER_NAME" --arg key "$W/client" \
 --arg known "$W/known_hosts" --arg remote "$W/remote-input.bin" \
 --arg sha "$DATA_SHA" --argjson bytes "$DATA_BYTES" --argjson port "$PORT" \
 '{host:$host,user:$user,port:$port,identity_file:$key,known_hosts_file:$known,
 remote_file:$remote,expected_sha256:$sha,expected_bytes:$bytes}' > "$W/ssh-in.json"
"$CLI" storage init --root "$W/store" --recipient "$RECIPIENT" >/dev/null
"$CLI" plugins ssh-backup --config "$W/ssh-in.json" \
 --root "$W/store" --identity "$W/agekey" --label real-ssh-b-in \
 --json > "$W/ssh-in-result.json"
ID=$(jq -er .id "$W/ssh-in-result.json")
"$CLI" storage verify-chain --root "$W/store" --identity "$W/agekey" --id "$ID" >/dev/null
"$CLI" plugins restore --input posix --output posix --root "$W/store" \
 --identity "$W/agekey" --id "$ID" --destination "$W/ssh-b-in-restored" >/dev/null
RECOVERED=$(find "$W/ssh-b-in-restored" -type f -name '*.bin' -print -quit)
test -n "$RECOVERED"
cmp "$W/remote-input.bin" "$RECOVERED"
echo 'PASS real OpenSSH B-In with SHA-pinned source and encrypted age chunk recovery'
echo 'PASS real OpenSSH B-Out create-only ciphertext and SHA marker, independent remote GET and age restore'
echo 'NOT CERTIFIED SFTP-only shell-less servers, large transfer resume or physical HA'
