#!/usr/bin/env bash
# One-use GitHub self-hosted OpenAFS runner, dedicated VM ONLY.
set -euo pipefail
umask 077
[ "$(id -u)" -ne 0 ] && [ "$(id -un)" = mb-afs-runner ] || { echo 'Use mb-afs-runner, not root' >&2; exit 2; }
[ "$(sudo -n cat /etc/multi-backup-ci/allow-disposable-afs)" = DEDICATED_DISPOSABLE_OPENAFS_CELL_VM ] || exit 2
if sudo -n test -e /var/lib/multi-backup-ci/afs-run-used; then echo 'Used VM: refuse' >&2; exit 2; fi
sudo -n modprobe openafs
grep -q '^openafs ' /proc/modules
META=$(curl --fail --silent --show-error --location --retry 3 \
 -H 'Accept: application/vnd.github+json' https://api.github.com/repos/actions/runner/releases/latest)
ASSET=$(jq -er '[.assets[] | select(.name | startswith("actions-runner-linux-x64-")) |
 select(.name | endswith(".tar.gz"))] | first | [.browser_download_url, .digest] | @tsv' <<< "$META")
IFS=$'\t' read -r URL DIGEST <<< "$ASSET"
[[ "$URL" =~ ^https://github.com/actions/runner/releases/download/ ]] || exit 2
[[ "$DIGEST" =~ ^sha256:[a-f0-9]{64}$ ]] || { echo 'Runner SHA256 asset digest missing: abort' >&2; exit 2; }
DIR="$HOME/actions-runner"
[ ! -e "$DIR" ] || { echo 'Runner folder exists: no reuse' >&2; exit 2; }
mkdir -m 0700 "$DIR"
FILE="$DIR/runner.tar.gz"
curl --fail --silent --show-error --location --retry 3 --output "$FILE" "$URL"
printf '%s  %s\n' "$(printf '%s' "$DIGEST" | cut -c8-)" "$FILE" | sha256sum --check --status
tar -xzf "$FILE" -C "$DIR"
rm -f "$FILE"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs |
    sh -s -- -y --profile minimal
fi
[ ! -f "$HOME/.cargo/env" ] || . "$HOME/.cargo/env"
command -v cargo >/dev/null
cd "$DIR"
echo 'Obtain short-lived registration token at repo Settings > Actions > Runners > New self-hosted runner.'
read -r -s -p 'Registration token (not saved in repo): ' TOKEN
printf '\n'
[ -n "$TOKEN" ] || exit 2
./config.sh --unattended --ephemeral --url https://github.com/desalvo/multi-backup \
 --token "$TOKEN" --name "mb-afs-$(hostname -s)-$(date +%s)" \
 --labels openafs-ci --work _work
unset TOKEN
echo 'Ephemeral runner connected for one approved job; destroy VM after job.'
exec ./run.sh
