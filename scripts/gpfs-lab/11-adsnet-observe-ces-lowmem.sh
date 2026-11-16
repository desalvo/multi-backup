#!/usr/bin/env bash
# READ ONLY: CES NFS/SMB functionality observations on 16 GiB RHEL 9.8 lab nodes.
# No install, start/stop, memory tuning, network or storage mutation.
set -euo pipefail
ROLE="${1:-}"
case "$ROLE" in 01|02) ;; *) echo "Usage: bash $0 01|02" >&2; exit 2;; esac
HOST="github-runner-$ROLE.vm.adsnet.it"
test "$(hostname -f)" = "$HOST" || { echo "FAIL unexpected hostname" >&2; exit 2; }
. /etc/os-release
test "$ID" = rhel && test "$VERSION_ID" = 9.8 || { echo "FAIL expected RHEL 9.8" >&2; exit 2; }
echo "HOST=$HOST RHEL=$VERSION_ID KERNEL=$(uname -r)"
echo '16GiB lab below vendor CES 64GiB NFS / 128GiB SMB recommendations; functional-only trial.'
echo 'Free memory, swap and memory PSI:'
free -m
if [ -r /proc/pressure/memory ]; then cat /proc/pressure/memory; fi
echo 'Highest RSS consumers (KiB):'
ps -eo pid,comm,rss --sort=-rss | head -12
if ! command -v mmces >/dev/null 2>&1; then
  echo 'PENDING: mmces not installed. Install IBM Developer Edition first.'
  exit 1
fi
fail=0
echo 'CES node inventory:'
timeout 35s mmces node list || fail=1
echo 'CES VIP allocation:'
timeout 35s mmces address list || fail=1
echo 'CES service inventory:'
timeout 35s mmces service list || fail=1
echo 'CES status:'
timeout 35s mmces state show || fail=1
echo 'IBM node health:'
if command -v mmhealth >/dev/null 2>&1; then timeout 35s mmhealth node show || fail=1; fi
echo 'Kernel OOM events last 30min (local logs only):'
if command -v journalctl >/dev/null 2>&1; then journalctl -k --since '30 minutes ago' --no-pager -o cat 2>/dev/null | grep -Ei 'out of memory|oom.kill|killed process' | tail -20 || :; fi
if [ "$fail" -ne 0 ]; then
  echo 'INCOMPLETE: some IBM read-only CES commands failed. No automatic recovery attempted.'
  exit 1
fi
echo 'PASS CES inventory readable. NOT proof of NFS/SMB backup+restore nor hardware support.'
echo 'A total_memory_small TIP alone is informational; OOM, unhealthy CES or lost VIP require investigation.'
