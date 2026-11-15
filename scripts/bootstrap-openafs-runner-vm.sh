#!/usr/bin/env bash
# Run ONLY on a clean sacrificial Ubuntu 22.04 GA kernel VM; root required.
set -euo pipefail
umask 077
[ "$(id -u)" -eq 0 ] || { echo 'Root required' >&2; exit 2; }
[ "$(uname -m)" = x86_64 ] || { echo 'x86_64 required' >&2; exit 2; }
. /etc/os-release
[ "$ID" = ubuntu ] && [ "$VERSION_ID" = 22.04 ] || { echo 'Use Ubuntu 22.04' >&2; exit 2; }
case "$(uname -r)" in 5.15.*-generic) ;; *) echo 'Use new Ubuntu 22.04 VM with GA kernel 5.15-generic' >&2; exit 2;; esac
if [ -e /etc/openafs/ThisCell ] || [ -e /var/lib/krb5kdc/principal ] ||
   [ -e /etc/multi-backup-ci/allow-disposable-afs ] ||
   [ -e /var/lib/multi-backup-ci/afs-run-used ]; then
  echo 'Old AFS/Kerberos/runner state detected; refusing to run' >&2; exit 2
fi
printf 'This must be a dedicated disposable VM with NO production data or network access.\n'
read -r -p 'Type DISPOSABLE-OPENAFS-VM to confirm: ' ACK
[ "$ACK" = DISPOSABLE-OPENAFS-VM ] || exit 2
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y --no-install-recommends ca-certificates curl git jq tar gzip \
 sudo build-essential python3-venv python3-pip libicu70 libssl3 \
 dkms "linux-headers-$(uname -r)" age krb5-user krb5-config
test -d "/lib/modules/$(uname -r)/build"
if ! apt-get install -y --no-install-recommends openafs-modules-dkms; then
  echo 'OpenAFS DKMS failed: discard VM, choose known compatible kernel' >&2
  find /var/lib/dkms/openafs -name make.log -exec tail -65 '{}' ';' 2>/dev/null || :
  exit 1
fi
modprobe openafs
grep -q '^openafs ' /proc/modules
if id mb-afs-runner >/dev/null 2>&1; then
  echo 'Existing runner account: refuse reuse' >&2; exit 2
fi
useradd --create-home --user-group --shell /bin/bash mb-afs-runner
# This NOPASSWD grant is acceptable ONLY on a fresh VM destroyed after 1 job.
printf 'mb-afs-runner ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/90-mb-openafs-ci
chmod 0440 /etc/sudoers.d/90-mb-openafs-ci
visudo -cf /etc/sudoers.d/90-mb-openafs-ci
install -d -m 0755 /etc/multi-backup-ci
echo DEDICATED_DISPOSABLE_OPENAFS_CELL_VM > /etc/multi-backup-ci/allow-disposable-afs
chmod 0644 /etc/multi-backup-ci/allow-disposable-afs
echo 'PASS: real OpenAFS DKMS/module; next use dedicated mb-afs-runner account.'
