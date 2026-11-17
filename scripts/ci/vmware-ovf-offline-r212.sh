#!/usr/bin/env bash
# Real VMDK data encrypted and restored byte-for-byte, NOT actual ESXi.
set -Eeuo pipefail
umask 077
test "$GITHUB_ACTIONS" = true && test "$RUNNER_ENVIRONMENT" = github-hosted
NATIVE="$(cd "$(dirname "$0")/../.." && pwd)/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
TMP="$(mktemp -d)"
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/staging/package" "$TMP/store"
chmod 0700 "$TMP" "$TMP/staging" "$TMP/staging/package" "$TMP/store"
qemu-img create -f raw "$TMP/original.raw" 4M >/dev/null
printf 'MB-VMWARE-BLOCK-BYTE-PROOF\n' | dd of="$TMP/original.raw" bs=1 conv=notrunc status=none
qemu-img convert -f raw -O vmdk -o subformat=streamOptimized "$TMP/original.raw" "$TMP/staging/package/disk.vmdk"
printf '<?xml version="1.0"?>\n<Envelope><References><File href="disk.vmdk"/></References></Envelope>\n' > "$TMP/staging/package/machine.ovf"
export PACKAGE="$TMP/staging/package"
PIN="$(python3 - <<'PY'
import os,hashlib
from pathlib import Path
h=hashlib.sha256()
for f in sorted(Path(os.environ["PACKAGE"]).iterdir()):
 h.update(f.name.encode());h.update(b"\0")
 h.update(f.stat().st_size.to_bytes(8,"little"))
 h.update(hashlib.sha256(f.read_bytes()).digest())
print(h.hexdigest())
PY
)"
age-keygen -o "$TMP/identity" >/dev/null 2>&1
age-keygen -o "$TMP/wrong" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TMP/identity")"
"$NATIVE" storage init --root "$TMP/store" --recipient "$RECIPIENT" >/dev/null
if "$NATIVE" plugins vmware-ovf-package-backup --root "$TMP/store" --identity "$TMP/identity" \
 --stage-root "$TMP/staging" --source-ovf-dir "$PACKAGE" --source-moref vm-4242 \
 --expected-fingerprint "$(printf '0%.0s' {1..64})" --label bad >/dev/null 2>&1;then
 echo 'FAIL bad independent fingerprint accepted' >&2;exit 1
fi
"$NATIVE" plugins vmware-ovf-package-backup --root "$TMP/store" --identity "$TMP/identity" \
 --stage-root "$TMP/staging" --source-ovf-dir "$PACKAGE" --source-moref vm-4242 \
 --expected-fingerprint "$PIN" --label genuine-vmdk-age-roundtrip --json > "$TMP/receipt.json"
jq -e '.encrypted==true and .source_provenance_attested==false and .vmdk_count==1' "$TMP/receipt.json" >/dev/null
ID="$(jq -er .id "$TMP/receipt.json")"
"$NATIVE" storage verify --root "$TMP/store" --identity "$TMP/identity" --id "$ID" >/dev/null
"$NATIVE" storage restore --root "$TMP/store" --identity "$TMP/identity" --id "$ID" --destination "$TMP/recovered" >/dev/null
mkdir "$TMP/extracted"
tar -xf "$TMP/recovered/vmware/vm/vm-4242/ovf.tar" -C "$TMP/extracted"
cmp "$PACKAGE/disk.vmdk" "$TMP/extracted/disk.vmdk"
cmp "$PACKAGE/machine.ovf" "$TMP/extracted/machine.ovf"
qemu-img convert -f vmdk -O raw "$TMP/extracted/disk.vmdk" "$TMP/decoded.raw"
cmp "$TMP/original.raw" "$TMP/decoded.raw"
if "$NATIVE" storage restore --root "$TMP/store" --identity "$TMP/wrong" \
 --id "$ID" --destination "$TMP/wrong-recovery" >/dev/null 2>&1;then
 echo 'FAIL wrong age identity accepted' >&2;exit 1
fi
if "$NATIVE" storage restore --root "$TMP/store" --identity "$TMP/identity" \
 --id "$ID" --destination "$TMP/recovered" >/dev/null 2>&1;then
 echo 'FAIL restore overwrite accepted' >&2;exit 1
fi
ln -s "$TMP/original.raw" "$PACKAGE/hostile.vmdk"
if "$NATIVE" plugins vmware-ovf-package-backup --root "$TMP/store" --identity "$TMP/identity" \
 --stage-root "$TMP/staging" --source-ovf-dir "$PACKAGE" --source-moref vm-4242 \
 --expected-fingerprint "$PIN" --label unsafe >/dev/null 2>&1;then
 echo 'FAIL symlink source accepted' >&2;exit 1
fi
echo 'PASS real VMDK and OVF into native age store, restored original raw bytes, negative digest/key/overwrite/symlink.'
echo 'NOT VMware vCenter/ESXi field, VADP/CBT or guest content boot acceptance.'
