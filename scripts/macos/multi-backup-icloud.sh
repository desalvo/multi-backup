#!/usr/bin/env bash
# Multi Backup macOS iCloud Drive: one coordinated file B-In and encrypted B-Out.
set -Eeuo pipefail
umask 077
base="$(cd "$(dirname "$0")" && pwd)"
helper="$base/multi-backup-icloud"
native="multi-backup-native"
if [ ! -x "$helper" ]; then
  echo "Build the Mac helper: swiftc -O '$base/multi-backup-icloud.swift' -o '$helper'" >&2
  exit 2
fi
command -v "$native" >/dev/null 2>&1 || {
  echo "Install native Multi Backup Rust CLI" >&2; exit 2;
}
if [ "$#" -lt 1 ]; then
  echo "Usage: $0 backup ICLOUD_RELATIVE_FILE NEW_LOCAL.tar.gz.age AGE_RECIPIENT | push LOCAL.tar.gz.age NEW_ICLOUD_RELATIVE.age AGE_IDENTITY | pull-encrypted ICLOUD_RELATIVE.age NEW_LOCAL.age" >&2
  exit 2
fi
mode="$1"; shift
case "$mode" in
  backup)
    test "$#" -eq 3 || { echo 'backup requires exactly three arguments' >&2; exit 2; }
    remote="$1"; target="$2"; recipient="$3"
    test ! -e "$target" || { echo 'Refusing overwrite of local age backup' >&2; exit 2; }
    stage="$(mktemp -d /tmp/mb-icloud.XXXXXXXX)"
    trap 'rm -rf -- "$stage"' EXIT
    name="$(basename "$remote")"
    "$helper" pull "$remote" "$stage/$name"
    "$native" archive create --source "$stage" --output "$target" --recipient "$recipient"
    echo "iCloud B-In backup locally age-encrypted; remote cloud sync status NOT certified" >&2
    ;;
  push)
    test "$#" -eq 3 || { echo 'push requires LOCAL.age REMOTE.age IDENTITY' >&2; exit 2; }
    local_age="$1"; remote="$2"; identity="$3"
    test -f "$local_age" && test -f "$identity" || {
      echo 'Authenticated local archive and age identity required' >&2; exit 2;
    }
    "$native" archive list --archive "$local_age" --identity "$identity" >/dev/null
    "$helper" push "$local_age" "$remote"
    echo "iCloud B-Out local File Provider copy verified; remote cloud replication NOT attested" >&2
    ;;
  pull-encrypted)
    test "$#" -eq 2 || { echo 'pull-encrypted requires REMOTE.age NEW_LOCAL.age' >&2; exit 2; }
    case "$1" in *.age) ;; *) echo 'Not an encrypted iCloud archive path' >&2; exit 2;; esac
    "$helper" pull "$1" "$2"
    ;;
  *) echo "Unknown operation $mode" >&2; exit 2 ;;
esac
