#!/usr/bin/env bash
# READ ONLY: IBM cluster, CES, exports and licensed-usage information.
set -euo pipefail
for bin in mmlscluster mmgetstate mmlslicense mmces mmnfs; do
  command -v "$bin" >/dev/null || { echo "Missing IBM command: $bin" >&2; exit 1; }
done
echo '=== IBM cluster identity ==='
mmlscluster
echo '=== GPFS cluster status ==='
mmgetstate -a
echo '=== Developer Edition licensed usage ==='
mmlslicense --licensed-usage
echo '=== CES services ==='
mmces service list -a
echo '=== CES NFS exports ==='
mmnfs export list
echo '=== CES SMB exports ==='
if command -v mmsmb >/dev/null; then mmsmb export list; else echo 'SMB not installed; not yet certified'; fi
echo '=== GPFS filesystem mounts ==='
findmnt -t gpfs -o TARGET,FSTYPE,OPTIONS
echo 'Report does not itself certify license acceptance, snapshot consistency or metadata fidelity.'
