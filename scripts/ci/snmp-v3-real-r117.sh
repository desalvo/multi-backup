#!/usr/bin/env bash
# Real Net-SNMP v3 authPriv protocol regression on disposable GitHub runner.
# The status producer is a synthetic fixed-value echo, not the Rust server.
set -Eeuo pipefail
umask 077
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${RUNNER_ENVIRONMENT:-}" != github-hosted ];then
  echo "REFUSED: SNMP authPriv acceptance requires disposable hosted CI" >&2
  exit 3
fi
for cmd in snmpd snmpwalk openssl ss python3;do command -v "$cmd" >/dev/null;done
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
PID=""
cleanup(){
  if [ -n "$PID" ];then kill "$PID" 2>/dev/null || :;wait "$PID" 2>/dev/null || :;fi
  rm -rf -- "$TMP"
}
trap cleanup EXIT
chmod 0700 "$TMP"
export SNMP_PERSISTENT_DIR="$TMP/persist"
mkdir -m 0700 "$SNMP_PERSISTENT_DIR"
AUTH="$(openssl rand -hex 24)"
PRIV="$(openssl rand -hex 24)"
export MULTI_BACKUP_SNMP_ENABLED=true MULTI_BACKUP_SNMP_BIND=127.0.0.1
export MULTI_BACKUP_SNMP_PORT=11611 MULTI_BACKUP_SNMP_VERSIONS=3
export MULTI_BACKUP_SNMP_V3_USER=mbmonitor
bash "$ROOT/scripts/configure-baremetal-snmp.sh" "$TMP/snmpd.conf" >/dev/null
python3 - "$TMP/snmpd.conf" <<'PY'
from pathlib import Path
import sys
path=Path(sys.argv[1])
data=path.read_text()
assert "agentAddress udp:127.0.0.1:11611" in data
assert "rouser mbmonitor priv -V mbstatus" in data
assert "rocommunity" not in data
assert "extend mbStatus /usr/bin/multi-backup-server monitor-snmp-status" in data
# Only the native status producer is replaced. Net-SNMP/VACM/authPriv are real.
data=data.replace(
    "extend mbStatus /usr/bin/multi-backup-server monitor-snmp-status",
    "extend mbStatus /bin/echo mb_cluster_nodes_total=3")
path.write_text(data)
path.chmod(0o600)
PY
printf 'createUser mbmonitor SHA "%s" AES "%s"\n' "$AUTH" "$PRIV" >> "$TMP/snmpd.conf"
snmpd -f -Lo -C -c "$TMP/snmpd.conf" -p "$TMP/snmpd.pid" >"$TMP/snmpd.log" 2>&1 &
PID="$!"
ready=false
for n in $(seq 1 25);do
  if ! kill -0 "$PID" 2>/dev/null;then break;fi
  if snmpwalk -On -v3 -l authPriv -u mbmonitor \
    -a SHA -A "$AUTH" -x AES -X "$PRIV" -t 1 -r 0 \
    udp:127.0.0.1:11611 .1.3.6.1.4.1.8072.1.3.2 \
    >"$TMP/authorized.log" 2>"$TMP/authorized-error.log";then
    if grep -Fq "mb_cluster_nodes_total=3" "$TMP/authorized.log";then
      ready=true
      break
    fi
  fi
  sleep 1
done
if [ "$ready" != true ];then
  echo "FAIL real Net-SNMP v3 authPriv status read" >&2
  tail -n 35 "$TMP/snmpd.log" >&2 || :
  tail -n 12 "$TMP/authorized-error.log" >&2 || :
  exit 1
fi
if ! ss -lun | grep -Eq '127\.0\.0\.1:11611[[:space:]]';then
  echo "FAIL SNMP daemon is not bound to loopback" >&2;exit 1
fi
if snmpwalk -On -v3 -l authPriv -u mbmonitor \
  -a SHA -A "wrong-$(openssl rand -hex 10)" -x AES -X "$PRIV" \
  -t 1 -r 0 udp:127.0.0.1:11611 .1.3.6.1.4.1.8072.1.3.2 \
  >/dev/null 2>&1;then
  echo "FAIL SNMP accepted wrong authentication key" >&2;exit 1
fi
if snmpwalk -On -v2c -c public -t 1 -r 0 \
  udp:127.0.0.1:11611 .1.3.6.1.4.1.8072.1.3.2 \
  >/dev/null 2>&1;then
  echo "FAIL SNMP exposed legacy v2c with v3-only configuration" >&2;exit 1
fi
echo "PASS real Net-SNMP v3 SHA/AES authPriv read; wrong-key and v2c denied; loopback-only"
echo "Scope: real Net-SNMP transport with synthetic read-only extend fixture"
