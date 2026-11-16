#!/usr/bin/env bash
# Read-only IBM GPFS CORE readiness audit; never requires CES, VIPs or extra RAM.
# Run AFTER installing the licensed IBM Storage Scale Developer Edition software.
set -euo pipefail
case "$(hostname -f)" in
  github-runner-01.vm.adsnet.it|github-runner-02.vm.adsnet.it|github-runner-03.vm.adsnet.it) ;;
  *) echo "FAIL: this audit may run only on three ADSNET GPFS nodes" >&2; exit 2 ;;
esac
. /etc/os-release
test "$ID" = rhel && test "$VERSION_ID" = 9.8 || { echo "FAIL: expected RHEL 9.8" >&2; exit 2; }
echo "HOST=$(hostname -f) OS=$PRETTY_NAME KERNEL=$(uname -r)"
fail=0
for command_name in mmgetstate mmlscluster mmlsnsd mmlsfs; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "PENDING: IBM tool $command_name is absent (IBM not installed/initialized yet)"
    fail=1
  fi
done
if [ "$fail" -ne 0 ]; then
  echo "NOT READY: IBM GPFS base binaries are not installed; no CES test attempted."
  exit 1
fi
echo 'IBM cluster topology (read-only):'
mmlscluster || fail=1
echo 'NSD inventory (read-only, NEVER create or modify here):'
mmlsnsd || fail=1
echo 'Node state:'
mmgetstate -a || fail=1
echo 'Filesystem inventory:'
mmlsfs all || fail=1
if command -v mmlslicense >/dev/null 2>&1; then
  echo 'Licensed usage (no secret should be uploaded to public logs):'
  mmlslicense --licensed-usage || fail=1
fi
echo 'Mounted real GPFS filesystems on this node (not NFS or SMB):'
MOUNTS=$(findmnt -rn -t gpfs -o TARGET 2>/dev/null || :)
if [ -z "$MOUNTS" ]; then
  echo 'PENDING: no locally mounted real gpfs FSTYPE filesystem'
  fail=1
else
  printf '%s\n' "$MOUNTS"
fi
if [ "$fail" -ne 0 ]; then
  echo 'NOT READY for real GPFS native Rust recovery: inspect IBM state and authentic mounts.'
  exit 1
fi
echo 'PASS core GPFS installed, inventory readable, and native gpfs mount observed.'
echo 'NOT CERTIFIED: snapshot consistency, actual backup/restore, CES, memory sizing, HA/failover or ACL/xattr fidelity.'
