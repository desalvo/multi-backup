#!/usr/bin/env bash
# Read-only IBM Storage Scale GPL prerequisites and NSD signature audit.
# NEVER formats or changes disk, package, kernel, subscription or network.
set -euo pipefail
ROLE="$1"
case "$ROLE" in 01|02|03) ;; *) echo "Usage: bash $0 01|02|03" >&2; exit 2 ;; esac
EXPECTED="github-runner-$ROLE.vm.adsnet.it"
test "$(hostname -f)" = "$EXPECTED" || { echo "FAIL unexpected host $(hostname -f)" >&2; exit 1; }
. /etc/os-release
test "$ID" = rhel && test "$VERSION_ID" = 9.8 || { echo "FAIL requires installed RHEL 9.8" >&2; exit 1; }
test "$(uname -m)" = x86_64
NODE_ID="$(hostname -f)"
if [ "$ROLE" = 03 ]; then
  DEV=/dev/sda
else
  DEV=/dev/sdb
fi
echo "HOST=$NODE_ID KERNEL=$(uname -r) CANDIDATE_DISK=$DEV"
echo 'WARNING: /dev/sdX names are from the operator inventory only. Confirm by hypervisor WWN/serial immediately before ANY future write.'
FAIL=0
KERNEL="$(uname -r)"
if rpm -q "kernel-devel-$KERNEL" >/dev/null 2>&1; then
  echo "PASS installed kernel-devel exactly matches running kernel $KERNEL"
else
  echo "FAIL missing exact kernel-devel-$KERNEL"
  FAIL=1
fi
for package in ksh gcc gcc-c++ cpp make binutils which; do
  if rpm -q "$package" >/dev/null 2>&1; then
    echo "PASS installed $package"
  else
    echo "FAIL missing $package"
    FAIL=1
  fi
done
RELEASE="$(subscription-manager release --show 2>&1 || :)"
echo "subscription-manager release: $RELEASE"
if ! printf '%s\n' "$RELEASE" | grep -Eq '9[.]8'; then
  echo 'WARN subscription release does not show 9.8; do not run dnf update'
fi
if [ ! -b "$DEV" ]; then echo "FAIL missing block disk $DEV"; exit 1; fi
TYPE=$(lsblk -dn -o TYPE "$DEV")
BYTES=$(lsblk -bdn -o SIZE "$DEV")
test "$TYPE" = disk || { echo "FAIL candidate is not a whole disk: $TYPE"; exit 1; }
if [ "$BYTES" -lt 130000000000 ] || [ "$BYTES" -gt 180000000000 ]; then
  echo "FAIL disk size not approximately 150GB ($BYTES bytes)"; exit 1
fi
echo "Candidate device details: $(lsblk -dn -o NAME,SIZE,TYPE,SERIAL,WWN "$DEV")"
if [ "$(lsblk -nr -o NAME "$DEV" | wc -l)" -ne 1 ]; then
  echo "FAIL candidate device has partitions, children or mapper entries"
  FAIL=1
fi
echo "Read-only on-disk signatures via wipefs --no-act:"
if ! command -v wipefs >/dev/null; then
  echo 'FAIL wipefs is unavailable; cannot assess disk signatures'
  FAIL=1
else
  signatures=$(wipefs --no-act "$DEV")
  if [ -n "$signatures" ]; then
    printf '%s\n' "$signatures"
    echo 'FAIL existing disk signature(s); do NOT erase or use as NSD'
    FAIL=1
  else
    echo 'PASS no signatures detected by wipefs --no-act (not proof of no hidden data)'
  fi
fi
if findmnt -rn -S "$DEV" >/dev/null 2>&1; then
  echo 'FAIL candidate disk is a mount source'
  FAIL=1
fi
echo 'Stable device mapping candidates (for human cross-check):'
find -L /dev/disk/by-id -maxdepth 1 -type b -samefile "$DEV" -print 2>/dev/null || :
if [ "$ROLE" = 01 ] || [ "$ROLE" = 02 ]; then
  echo 'CES CAPACITY WARNING: 16GB RAM is BELOW IBM 64GB NFS-only and 128GB SMB/multi-protocol recommended memory.'
  echo 'This is only an experimental lab until hardware/protocol sizing is reviewed, regardless of other checks.'
fi
if [ "$FAIL" -ne 0 ]; then
  echo 'NOT READY for IBM GPL build / NSD authorization; correct the listed issues without upgrading to RHEL 9.9.'
  exit 1
fi
echo 'PASS read-only dependency and visible disk-signature audit; IBM license, exact kernel support and CES sizing not certified.'
