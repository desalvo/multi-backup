#!/usr/bin/env bash
# READ ONLY: verify 3x3 SSH root mesh with existing keys/known_hosts, no enrollment.
set -euo pipefail
test "$(id -u)" = 0 || { echo "Run as root on GPFS node 01/02/03" >&2; exit 2; }
LOCAL=$(hostname -f)
case "$LOCAL" in github-runner-0[123].vm.adsnet.it) ;; *) echo "Not a GPFS node: $LOCAL" >&2; exit 2;; esac
test ! -L /root/.ssh/known_hosts && test -f /root/.ssh/known_hosts || {
  echo 'Missing trusted known_hosts: compare fingerprints manually before testing' >&2; exit 1;
}
N=0; FAIL=0
for ID in 01 02 03; do
  FQDN="github-runner-$ID.vm.adsnet.it"
  SHORT="github-runner-$ID"
  for DEST in "$FQDN" "$SHORT"; do
    N=$((N+1))
    if OUT=$(ssh -o BatchMode=yes -o StrictHostKeyChecking=yes \
      -o UpdateHostKeys=no -o PasswordAuthentication=no \
      -o KbdInteractiveAuthentication=no -o ConnectTimeout=8 \
      -o ForwardAgent=no -o ClearAllForwardings=yes \
      "root@$DEST" 'hostname -f' 2>&1); then
      if [ "$OUT" = "$FQDN" ]; then
        echo "PASS $LOCAL -> $DEST -> $OUT"
      else
        echo "FAIL $LOCAL -> $DEST unexpected identity: $OUT"
        FAIL=$((FAIL+1))
      fi
    else
      echo "FAIL $LOCAL -> $DEST: $OUT"
      FAIL=$((FAIL+1))
    fi
  done
done
echo "SSH checks from $LOCAL: $((N-FAIL))/$N passed"
test "$FAIL" -eq 0
