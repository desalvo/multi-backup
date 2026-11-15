#!/usr/bin/env bash
# READ ONLY: IBM Storage Scale Developer Edition Linux prerequisite audit.
set -euo pipefail
test -r /etc/os-release
. /etc/os-release
test "$ID" = rhel || { echo "FAIL: requires supported RHEL, found $ID" >&2; exit 1; }
test "$(uname -m)" = x86_64
for cmd in rpm dnf lsblk findmnt getent ssh; do command -v "$cmd" >/dev/null; done
echo "OS=$PRETTY_NAME; kernel=$(uname -r); hostname=$(hostname -f)"
echo "CPUs=$(nproc); memory KiB=$(awk '/MemTotal/{print $2}' /proc/meminfo)"
test "$(nproc)" -ge 4
awk '/MemTotal/ { if ($2 < 8*1024*1024) exit 1 }' /proc/meminfo
getent hosts "$(hostname -f)" >/dev/null
echo "READ-ONLY disk inventory; NEVER autoformat or activate NSDs:"
lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS,WWN,SERIAL
echo "Enabled repository inventory:"
dnf repolist --enabled
if command -v timedatectl >/dev/null; then timedatectl status | grep -E 'NTP service|System clock synchronized' || :; fi
if command -v firewall-cmd >/dev/null; then firewall-cmd --state || :; fi
if command -v getenforce >/dev/null; then getenforce; fi
if command -v mmgetstate >/dev/null; then
  mmgetstate -a
  mmlslicense --licensed-usage || :
fi
echo 'PASS OS read-only preflight. Human must confirm exact IBM kernel compatibility, product license and NSD device identities.'
