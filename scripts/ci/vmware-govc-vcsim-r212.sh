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
"$NATIVE" plugins vmware-ovf-backup --profile-json "$TMP/source.json" \
 --root "$TMP/store" --identity "$TMP/private" --label "VMware sim OVF" \
 --stage-root "$TMP/staging" --json >"$TMP/backup.json"
ID="$(jq -er .id "$TMP/backup.json")"
jq -e '.encrypted==true and .offline_only==true and .vadp_cbt_certified==false' "$TMP/backup.json" >/dev/null
NAME=mb-sim-new-r212
"$NATIVE" plugins vmware-ovf-restore-new --profile-json "$TMP/target.json" \
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
echo 'PASS official govc, trusted TLS vSphere simulator, age OVF/VMDK and new-VM restore with role, key, duplicate and power negative tests'
echo 'NOT a genuine ESXi/vCenter/VADP/CBT/guest boot or hardware acceptance'
