#!/usr/bin/env bash
# READ ONLY: check forward IPv4 uniqueness, never assign VIPs.
set -euo pipefail
FAIL=0
IPS=" "
for H in github-runner-01.vm.adsnet.it github-runner-02.vm.adsnet.it github-runner-03.vm.adsnet.it github-runner-04.vm.adsnet.it github-runner-vip-01.vm.adsnet.it github-runner-vip-02.vm.adsnet.it; do
  IP=$(getent ahostsv4 "$H" | awk '$2=="STREAM" && $1 !~ /^127[.]/ {print $1;exit}' || :)
  if [ -z "$IP" ]; then echo "FAIL unresolved IPv4: $H"; FAIL=1; continue; fi
  case " $IPS " in *" $IP "*) echo "FAIL duplicate IP: $H maps to already used $IP"; FAIL=1;; *) echo "PASS $H -> $IP"; IPS="$IPS $IP";; esac
done
echo 'DNS pass does not validate CES is running or VIPs are bound.'
exit "$FAIL"
