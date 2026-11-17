#!/usr/bin/env bash
# Real govc/vcsim binaries, verified TLS. vcsim is NOT a physical ESXi.
set -Eeuo pipefail
umask 077
test "$GITHUB_ACTIONS" = true && test "$RUNNER_ENVIRONMENT" = github-hosted
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NATIVE="$ROOT/rust/target/debug/multi-backup-native"
test -x "$NATIVE"
export GOBIN="$(mktemp -d)"
TMP="$(mktemp -d)"
PID=""
cleanup() {
 if test -n "$PID";then kill "$PID" 2>/dev/null || :;wait "$PID" 2>/dev/null || :;fi
 rm -rf -- "$GOBIN" "$TMP"
}
trap cleanup EXIT
mkdir -p "$TMP/staging" "$TMP/store"
chmod 0700 "$TMP" "$TMP/staging" "$TMP/store"
git clone --quiet --depth 1 --branch v0.56.2 https://github.com/vmware/govmomi "$TMP/govmomi"
(cd "$TMP/govmomi/govc" && go build -o "$GOBIN/govc" .)
(cd "$TMP/govmomi/vcsim" && go build -o "$GOBIN/vcsim" .)
test -x "$GOBIN/govc" && test -x "$GOBIN/vcsim"
openssl req -newkey rsa:2048 -sha256 -nodes -x509 -days 2 \
 -subj "/CN=Multi Backup temporary CA" -addext "basicConstraints=critical,CA:TRUE" \
 -keyout "$TMP/ca.key" -out "$TMP/ca.pem" >/dev/null 2>&1
openssl req -newkey rsa:2048 -sha256 -nodes -subj "/CN=127.0.0.1" \
 -keyout "$TMP/server.key" -out "$TMP/server.csr" >/dev/null 2>&1
printf 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n' > "$TMP/server.ext"
openssl x509 -req -in "$TMP/server.csr" -CA "$TMP/ca.pem" -CAkey "$TMP/ca.key" \
 -CAcreateserial -out "$TMP/server.pem" -days 2 -sha256 -extfile "$TMP/server.ext" >/dev/null 2>&1
"$GOBIN/vcsim" -l 127.0.0.1:8989 -tls=true \
 -tlscert "$TMP/server.pem" -tlskey "$TMP/server.key" \
 -username mbci -password isolated-simulator-credential \
 -dc 1 -cluster 1 -host 1 -ds 1 -vm 1 -autostart=false >"$TMP/sim.log" 2>&1 &
PID=$!
export GOVC_URL=https://127.0.0.1:8989 GOVC_USERNAME=mbci
export GOVC_PASSWORD=isolated-simulator-credential GOVC_TLS_CA_CERTS="$TMP/ca.pem"
export GOVC_INSECURE=false GOVC_PERSIST_SESSION=false
ready=0
for _ in $(seq 1 60);do
 if "$GOBIN/govc" about >/dev/null 2>&1;then ready=1;break;fi
 if ! kill -0 "$PID" 2>/dev/null;then break;fi
 sleep 1
done
if test "$ready" -ne 1;then tail -n 30 "$TMP/sim.log" >&2;exit 1;fi
SOURCE="$("$GOBIN/govc" find /DC0/vm -type m | head -1)"
MORE="$("$GOBIN/govc" vm.info -json "$SOURCE" | jq -er '.virtualMachines[0].self.value // .VirtualMachines[0].Self.Value')"
DS="$("$GOBIN/govc" ls /DC0/datastore | head -1 | xargs basename)"
POOL="$("$GOBIN/govc" find /DC0/host -type p | grep '/Resources' | head -1)"
test -n "$SOURCE" && test -n "$MORE" && test -n "$DS" && test -n "$POOL"
GOVC_SHA="$(sha256sum "$GOBIN/govc" | cut -d ' ' -f1)"
export SOURCE MORE DS POOL GOVC_SHA TMP
python3 - <<'PY'
import json,os
t=os.environ["TMP"]
base={"endpoint":"https://127.0.0.1:8989","username":"mbci",
 "password":"isolated-simulator-credential","tls_ca_file":t+"/ca.pem",
 "govc_binary":os.environ["GOBIN"]+"/govc",
 "govc_sha256":os.environ["GOVC_SHA"],"datacenter":"DC0"}
src={**base,"role":"export","vm_path":os.environ["SOURCE"],"vm_moref":os.environ["MORE"]}
dst={**base,"role":"import","destination_folder":"/DC0/vm",
 "datastore":os.environ["DS"],"resource_pool":os.environ["POOL"]}
for name,profile in [("source.json",src),("target.json",dst)]:
 with open(t+"/"+name,"w") as f:json.dump(profile,f)
 os.chmod(t+"/"+name,0o600)
PY
age-keygen -o "$TMP/private" >/dev/null 2>&1
age-keygen -o "$TMP/wrong" >/dev/null 2>&1
PUB="$(age-keygen -y "$TMP/private")"
"$NATIVE" storage init --root "$TMP/store" --recipient "$PUB" >/dev/null
# vcsim does not implement VMware ExportVm NFC. Test the real govc
# import with a deterministic, independently pinned OVF/VMDK package;
# the genuine export remains protected in the physical vCenter field job.
PACKAGE="$TMP/staging/package"
mkdir -m 0700 "$PACKAGE"
qemu-img create -f raw "$TMP/disk.raw" 2M >/dev/null
printf 'MB-SIM-VMWARE-DISK-ROUNDTRIP' | dd of="$TMP/disk.raw" bs=1 conv=notrunc status=none
qemu-img convert -f raw -O vmdk -o subformat=streamOptimized \
 "$TMP/disk.raw" "$PACKAGE/disk.vmdk"
export PACKAGE
python3 - <<'PY'
import os
from pathlib import Path
d=Path(os.environ["PACKAGE"])
n=(d/"disk.vmdk").stat().st_size
xml=f'''<?xml version="1.0" encoding="UTF-8"?>
<Envelope xmlns="http://schemas.dmtf.org/ovf/envelope/1"
 xmlns:ovf="http://schemas.dmtf.org/ovf/envelope/1"
 xmlns:rasd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_ResourceAllocationSettingData"
 xmlns:vssd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_VirtualSystemSettingData">
 <References><File ovf:href="disk.vmdk" ovf:id="f1" ovf:size="{n}"/></References>
 <DiskSection><Info>VMware virtual disk</Info><Disk ovf:capacity="2"
 ovf:capacityAllocationUnits="byte * 2^20" ovf:diskId="d1" ovf:fileRef="f1"
 ovf:format="http://www.vmware.com/interfaces/specifications/vmdk.html#streamOptimized"/></DiskSection>
 <VirtualSystem ovf:id="mb-test">
  <Info>Disposable OVF virtual machine</Info><Name>mb-test</Name>
  <OperatingSystemSection ovf:id="36"><Info>Linux</Info></OperatingSystemSection>
  <VirtualHardwareSection><Info>Hardware</Info>
   <System><vssd:ElementName>mb-test</vssd:ElementName>
    <vssd:InstanceID>0</vssd:InstanceID>
    <vssd:VirtualSystemIdentifier>mb-test</vssd:VirtualSystemIdentifier>
    <vssd:VirtualSystemType>vmx-13</vssd:VirtualSystemType>
   </System>
   <Item><rasd:ElementName>CPU</rasd:ElementName><rasd:InstanceID>1</rasd:InstanceID>
    <rasd:ResourceType>3</rasd:ResourceType><rasd:VirtualQuantity>1</rasd:VirtualQuantity></Item>
   <Item><rasd:ElementName>Memory</rasd:ElementName><rasd:InstanceID>2</rasd:InstanceID>
    <rasd:ResourceType>4</rasd:ResourceType><rasd:VirtualQuantity>512</rasd:VirtualQuantity>
    <rasd:AllocationUnits>byte * 2^20</rasd:AllocationUnits></Item>
   <Item><rasd:AddressOnParent>0</rasd:AddressOnParent><rasd:ElementName>SCSI</rasd:ElementName>
    <rasd:InstanceID>3</rasd:InstanceID><rasd:ResourceSubType>lsilogic</rasd:ResourceSubType>
    <rasd:ResourceType>6</rasd:ResourceType></Item>
   <Item><rasd:AddressOnParent>0</rasd:AddressOnParent><rasd:ElementName>Disk</rasd:ElementName>
    <rasd:HostResource>ovf:/disk/d1</rasd:HostResource><rasd:InstanceID>4</rasd:InstanceID>
    <rasd:Parent>3</rasd:Parent><rasd:ResourceType>17</rasd:ResourceType></Item>
  </VirtualHardwareSection>
 </VirtualSystem>
</Envelope>
'''
(d/"mb-test.ovf").write_text(xml)
PY
python3 - <<'PY'
import os,hashlib
from pathlib import Path
d=Path(os.environ["PACKAGE"])
body=""
for f in sorted(d.iterdir()):
 body+=f"SHA256({f.name})= {hashlib.sha256(f.read_bytes()).hexdigest()}\n"
(d/"mb-test.mf").write_text(body)
PY
# Independent govc import.spec check and a disposable fixture-import probe.
# This is safe because vcsim lives only on this runner and test VM names are
# unique; it gives a meaningful upstream error before native orchestration.
if ! timeout --signal=TERM --kill-after=5s 30s "$GOBIN/govc" import.spec "$PACKAGE/mb-test.ovf" > "$TMP/ovf-import-spec.json" 2>"$TMP/ovf-spec-error";then
  echo "FAIL: official govc import.spec rejected disposable test OVF" >&2
  sed -n '1,18p' "$TMP/ovf-spec-error" >&2
  exit 1
fi
if ! timeout --signal=TERM --kill-after=5s 50s "$GOBIN/govc" import.ovf -m=true -name="mb-sim-upstream-probe" -ds="$DS" -pool="$POOL" -folder=/DC0/vm "$PACKAGE/mb-test.ovf" \
 >"$TMP/upstream-import.log" 2>"$TMP/upstream-import.err"; then
  echo "FAIL: official govc simulator rejected OVF package, before native Multi Backup" >&2
  sed -n '1,24p' "$TMP/upstream-import.err" >&2
  exit 1
fi
PIN="$(python3 - <<'PY'
import os,hashlib
from pathlib import Path
d=Path(os.environ["PACKAGE"])
h=hashlib.sha256()
for f in sorted(d.iterdir()):
 h.update(f.name.encode());h.update(b'\0')
 h.update(f.stat().st_size.to_bytes(8,'little'))
 h.update(hashlib.sha256(f.read_bytes()).digest())
print(h.hexdigest())
PY
)"
if "$NATIVE" plugins vmware-ovf-package-backup \
 --root "$TMP/store" --identity "$TMP/private" \
 --stage-root "$TMP/staging" --source-ovf-dir "$PACKAGE" \
 --source-moref "$MORE" \
 --expected-fingerprint "$(printf '0%.0s' {1..64})" \
 --label "Wrong hash must reject" >/dev/null 2>&1;then
 echo "FAIL wrong independent OVF fingerprint accepted" >&2;exit 1
fi
"$NATIVE" plugins vmware-ovf-package-backup \
 --root "$TMP/store" --identity "$TMP/private" \
 --stage-root "$TMP/staging" --source-ovf-dir "$PACKAGE" \
 --source-moref "$MORE" --expected-fingerprint "$PIN" \
 --label "Official govc simulator OVF package" --json >"$TMP/backup.json"
ID="$(jq -er .id "$TMP/backup.json")"
jq -e '.encrypted==true and .offline_only==true and .source_provenance_attested==false and .vadp_cbt_certified==false' "$TMP/backup.json" >/dev/null
NAME=mb-sim-new-r212
timeout --signal=TERM --kill-after=5s 70s "$NATIVE" plugins vmware-ovf-restore-new --profile-json "$TMP/target.json" \
 --root "$TMP/store" --identity "$TMP/private" --id "$ID" \
 --stage-root "$TMP/staging" --new-vm "$NAME" \
 --confirm-new-vm "vmware:$NAME:$DS" --json >"$TMP/result.json"
jq -e '.imported_new_vm==true and .boot_verified==false' "$TMP/result.json" >/dev/null
"$GOBIN/govc" vm.info -json "/DC0/vm/$NAME" | \
 jq -e '((.virtualMachines // .VirtualMachines)|length)==1' >/dev/null
if "$NATIVE" plugins vmware-ovf-restore-new --profile-json "$TMP/target.json" \
 --root "$TMP/store" --identity "$TMP/private" --id "$ID" \
 --stage-root "$TMP/staging" --new-vm "$NAME" \
 --confirm-new-vm "vmware:$NAME:$DS" >/dev/null 2>&1;then
 echo 'FAIL existing VM overwritten' >&2;exit 1
fi
if "$NATIVE" plugins vmware-ovf-restore-new --profile-json "$TMP/target.json" \
 --root "$TMP/store" --identity "$TMP/wrong" --id "$ID" \
 --stage-root "$TMP/staging" --new-vm "mb-sim-wrong-key" \
 --confirm-new-vm "vmware:mb-sim-wrong-key:$DS" >/dev/null 2>&1;then
 echo 'FAIL wrong age identity accepted' >&2;exit 1
fi
if "$NATIVE" plugins vmware-ovf-backup --profile-json "$TMP/target.json" \
 --root "$TMP/store" --identity "$TMP/private" --label "wrong role" \
 --stage-root "$TMP/staging" >/dev/null 2>&1;then
 echo 'FAIL import credentials used for export' >&2;exit 1
fi
"$GOBIN/govc" vm.power -on "$SOURCE" >/dev/null
if "$NATIVE" plugins vmware-ovf-backup --profile-json "$TMP/source.json" \
 --root "$TMP/store" --identity "$TMP/private" --label "unsafe running" \
 --stage-root "$TMP/staging" >/dev/null 2>&1;then
 echo 'FAIL live VM offline export accepted' >&2;exit 1
fi
echo 'PASS official govc vSphere simulator (offline source package), age OVF/VMDK and new-VM restore with role, key, duplicate and power negative tests'
echo 'NOT a genuine ESXi/vCenter/VADP/CBT/guest boot or hardware acceptance'
