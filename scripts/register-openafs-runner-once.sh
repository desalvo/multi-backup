#!/usr/bin/env bash
# Persist the isolated Debian lab VM, use a NEW GitHub --ephemeral runner each job.
# Reuse only SHA256-verified upstream runner downloads and the Rust toolchain.
set -euo pipefail
umask 077
[ "$(id -u)" -ne 0 ] && [ "$(id -un)" = mb-afs-runner ] || {
  echo 'Use non-root mb-afs-runner' >&2; exit 2
}
[ "$(sudo -n cat /etc/multi-backup-ci/allow-disposable-afs)" = DEDICATED_DISPOSABLE_OPENAFS_CELL_VM ] || {
  echo 'Missing isolated lab authorization' >&2; exit 2
}
if sudo -n test -e /var/lib/multi-backup-ci/afs-active; then
  echo 'AFS CI lease active or stale: inspect jobs before manual admin recovery' >&2; exit 2
fi
sudo -n modprobe openafs
grep -q '^openafs ' /proc/modules
command -v flock >/dev/null
CACHE="$HOME/.cache/multi-backup-afs"
SESSIONS="$HOME/.local/share/multi-backup-afs/runner-sessions"
mkdir -p -m 0700 "$CACHE" "$SESSIONS"
chmod 0700 "$CACHE" "$SESSIONS"
exec 9>"$CACHE/runner-registration.lock"
flock -n 9 || { echo 'Another runner session is active; parallel registration blocked' >&2; exit 2; }
TMP=""
SESSION=""
cleanup() {
  if [ -n "$TMP" ] && [ -f "$TMP" ]; then rm -f -- "$TMP"; fi
  if [ -n "$SESSION" ] && [ -d "$SESSION" ]; then rm -rf -- "$SESSION"; fi
}
trap cleanup EXIT
META=$(curl --fail --silent --show-error --location --retry 3 \
  -H 'Accept: application/vnd.github+json' \
  https://api.github.com/repos/actions/runner/releases/latest)
ASSET=$(jq -er '[.assets[] | select(.name | startswith("actions-runner-linux-x64-")) |
 select(.name | endswith(".tar.gz"))] | first |
 [.browser_download_url, .digest] | @tsv' <<< "$META")
IFS=$'\t' read -r URL DIGEST <<< "$ASSET"
[[ "$URL" =~ ^https://github.com/actions/runner/releases/download/ ]] || exit 2
[[ "$DIGEST" =~ ^sha256:[a-f0-9]{64}$ ]] || { echo 'Missing valid official runner SHA256' >&2; exit 2; }
SHA="$(printf '%s' "$DIGEST" | cut -c8-)"
ARCHIVE="$CACHE/actions-runner-$SHA.tar.gz"
if [ ! -f "$ARCHIVE" ]; then
  TMP=$(mktemp "$CACHE/.download.XXXXXXXX.tar.gz")
  curl --fail --silent --show-error --location --retry 3 --output "$TMP" "$URL"
  printf '%s  %s\n' "$SHA" "$TMP" | sha256sum --check --status
  mv -- "$TMP" "$ARCHIVE"
  TMP=""
fi
printf '%s  %s\n' "$SHA" "$ARCHIVE" | sha256sum --check --status
SESSION=$(mktemp -d "$SESSIONS/session.XXXXXXXXXX")
tar -xzf "$ARCHIVE" -C "$SESSION"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs |
    sh -s -- -y --profile minimal
fi
[ ! -f "$HOME/.cargo/env" ] || . "$HOME/.cargo/env"
command -v cargo >/dev/null
cd "$SESSION"
echo 'New GitHub runner registration token: repository Settings > Actions > Runners > New self-hosted runner.'
read -r -s -p 'Registration token (not saved): ' TOKEN
printf '\n'
[ -n "$TOKEN" ] || exit 2
./config.sh --unattended --ephemeral \
  --url https://github.com/desalvo/multi-backup --token "$TOKEN" \
  --name "mb-afs-$(hostname -s)-$(date +%s)" --labels openafs-ci --work _work
unset TOKEN
echo 'One-job ephemeral runner connected. VM and cached software persist.'
# Preserve EXIT cleanup: never exec here.
./run.sh
echo 'Job completed. Runner session and workspace deleted; VM retained.'
