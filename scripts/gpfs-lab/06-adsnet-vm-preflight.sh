#!/usr/bin/env bash
# READ ONLY: no packages, kernel updates, mounts, disk writes or SSH modifications.
set -euo pipefail
ROLE="$1"
case "$ROLE" in 01|02|03|04) ;; *) echo "Usage: bash $0 01|02|03|04" >&2; exit 2;; esac
EXPECTED="github-runner-$ROLE.vm.adsnet.it"
test "$(hostname -f)" = "$EXPECTED" || { echo "FAIL hostname mismatch: $(hostname -f)" >&2; exit 1; }
test "$(uname -m)" = x86_64
. /etc/os-release
if [ "$ROLE" = 04 ]; then
  test "$ID" = ubuntu && test "$VERSION_ID" = 24.04 || { echo "FAIL expected Ubuntu 24.04" >&2; exit 1; }
  MINCPU=4; MINRAM=$((7*1024*1024))
else
  test "$ID" = rhel && test "$VERSION_ID" = 9.8 || { echo "FAIL expected RHEL 9.8" >&2; exit 1; }
  MINCPU=6; MINRAM=$((13*1024*1024))
fi
CPU=$(nproc)
RAM=$(awk '/MemTotal/{print $2}' /proc/meminfo)
test "$CPU" -ge "$MINCPU" && test "$RAM" -ge "$MINRAM" || { echo 'FAIL CPU/RAM below VM profile' >&2; exit 1; }
echo "PASS identity=$EXPECTED OS=$PRETTY_NAME cpu=$CPU RAMKiB=$RAM kernel=$(uname -r)"
getent ahostsv4 "$EXPECTED" | head -3
timedatectl status | grep -E 'NTP service|System clock synchronized' || :
if [ "$ROLE" != 04 ]; then
  echo 'RHEL registration and release pin (read-only):'
  subscription-manager identity || :
  subscription-manager release --show || :
  echo 'Exact installed kernel-devel:'
  rpm -q "kernel-devel-$(uname -r)" || :
  for pkg in ksh gcc gcc-c++ cpp binutils which; do rpm -q "$pkg" || :; done
  lsblk -b -dn -o NAME,SIZE,TYPE | awk '$3=="disk" && $2>=130000000000 && $2<=180000000000 {ok=1} END{exit !ok}' || {
    echo 'FAIL no disk with approximately 150GB found; inspect hypervisor device mapping' >&2; exit 1;
  }
fi
echo 'Disk inventory, never auto-select a data/NSD device:'
lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS,SERIAL,WWN
echo 'PASS read-only VM preflight, not IBM license/kernel/NSD certification'
