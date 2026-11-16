#!/usr/bin/env bash
set -euo pipefail
umask 077
: "${MULTI_BACKUP_SNMP_ENABLED:=false}"
: "${MULTI_BACKUP_SNMP_BIND:=127.0.0.1}"
: "${MULTI_BACKUP_SNMP_PORT:=161}"
: "${MULTI_BACKUP_SNMP_VERSIONS:=3}"
: "${MULTI_BACKUP_SNMP_COMMUNITY:=public}"
: "${MULTI_BACKUP_SNMP_V3_USER:=mbmonitor}"
case "$MULTI_BACKUP_SNMP_ENABLED" in true|false);;*)echo "Invalid SNMP enabled" >&2;exit 2;;esac
case "$MULTI_BACKUP_SNMP_VERSIONS" in 3|1,2c|1,2c,3);;*)
  echo "Allowed SNMP versions: 3, 1,2c or 1,2c,3" >&2;exit 2;;esac
[[ "$MULTI_BACKUP_SNMP_PORT" =~ ^[0-9]+$ ]] &&
  [ "$MULTI_BACKUP_SNMP_PORT" -ge 1 ] && [ "$MULTI_BACKUP_SNMP_PORT" -le 65535 ]
[[ "$MULTI_BACKUP_SNMP_BIND" =~ ^[0-9a-fA-F.:]+$ ]]
case "$MULTI_BACKUP_SNMP_BIND" in 0.0.0.0|::|::0) echo "SNMP wildcard bind forbidden" >&2;exit 3;;esac
[[ "$MULTI_BACKUP_SNMP_V3_USER" =~ ^[a-zA-Z][a-zA-Z0-9_-]{2,31}$ ]]
[[ "$MULTI_BACKUP_SNMP_COMMUNITY" =~ ^[a-zA-Z0-9_.-]{4,128}$ ]]
output="${1:-/etc/multi-backup/snmpd.conf}"
if [ "$MULTI_BACKUP_SNMP_ENABLED" != true ];then
  echo "SNMP disabled: daemon not configured";exit 0
fi
if [[ "$MULTI_BACKUP_SNMP_VERSIONS" == *2c* ]] &&
   [ "$MULTI_BACKUP_SNMP_BIND" != 127.0.0.1 ] &&
   [ "$MULTI_BACKUP_SNMP_BIND" != ::1 ];then
   echo "Community SNMP v1/v2c restricted to loopback; use v3 for remote monitoring" >&2
   exit 3
fi
if [[ "$MULTI_BACKUP_SNMP_VERSIONS" == *2c* ]] &&
   [ "$MULTI_BACKUP_SNMP_COMMUNITY" = public ];then
   echo "WARNING: public is a legacy read-only loopback-only community" >&2
fi
install -d -m 0700 "$(dirname "$output")"
tmp="$(mktemp "$(dirname "$output")/.snmpd-XXXXXXXX")"
trap 'rm -f -- "$tmp"' EXIT
{
  printf 'agentAddress udp:%s:%s\n' "$MULTI_BACKUP_SNMP_BIND" "$MULTI_BACKUP_SNMP_PORT"
  printf 'sysLocation MultiBackup\n'
  printf 'view mbstatus included .1.3.6.1.4.1.8072.1.3.2\n'
  if [[ "$MULTI_BACKUP_SNMP_VERSIONS" == *3* ]];then
    printf 'rouser %s priv -V mbstatus\n' "$MULTI_BACKUP_SNMP_V3_USER"
  fi
  if [[ "$MULTI_BACKUP_SNMP_VERSIONS" == *2c* ]];then
    printf 'rocommunity %s %s -V mbstatus\n' "$MULTI_BACKUP_SNMP_COMMUNITY" "$MULTI_BACKUP_SNMP_BIND"
  fi
  printf 'extend mbStatus /usr/bin/multi-backup-server monitor-snmp-status\n'
} >"$tmp"
chmod 0600 "$tmp"
mv -f -- "$tmp" "$output"
echo "SNMP configuration ready; provision v3 authPriv credentials before enabling service"
