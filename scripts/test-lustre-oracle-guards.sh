#!/usr/bin/env bash
# Safe hosted-CI checks only: no Lustre mount or licensed Oracle database is impersonated.
set -euo pipefail
umask 077
CLI=rust/target/release/multi-backup-native
test -x "$CLI"
for dep in jq age-keygen; do command -v "$dep" >/dev/null; done
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
mkdir "$WORK/not-lustre"
echo fixture > "$WORK/not-lustre/sentinel.txt"
age-keygen -o "$WORK/identity" >/dev/null 2>&1
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/identity" | head -1)
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
# Genuine POSIX control backup to test the Lustre RESTORE refusal boundary.
"$CLI" plugins backup --input posix --output posix --source "$WORK/not-lustre" --root "$WORK/store" --identity "$WORK/identity" --label control --json > "$WORK/snapshot.json"
BACKUP_ID=$(jq -er .id "$WORK/snapshot.json")
if "$CLI" plugins probe --plugin lustre --mount "$WORK/not-lustre" --json >/dev/null 2>&1; then echo "FAIL: local directory accepted as Lustre" >&2; exit 1; fi
if "$CLI" plugins backup --input lustre --output posix --source "$WORK/not-lustre" --root "$WORK/store" --identity "$WORK/identity" --label forged-lustre --source-snapshot-attested >/dev/null 2>&1; then echo "FAIL: forged Lustre snapshot accepted" >&2; exit 1; fi
if "$CLI" plugins restore --input posix --output lustre --root "$WORK/store" --identity "$WORK/identity" --id "$BACKUP_ID" --destination "$WORK/not-lustre/new" >/dev/null 2>&1; then echo "FAIL: local directory accepted as Lustre restore mount" >&2; exit 1; fi
test ! -e "$WORK/not-lustre/new"
"$CLI" plugins plan --mode backup --input oracle --output posix --json | jq -e '.executable == false and .preview_executable == false' >/dev/null
"$CLI" plugins plan --mode restore --input posix --output oracle --json | jq -e '.executable == false and .preview_executable == false' >/dev/null
cat > "$WORK/oracle.json" <<EOF
{"engine":"oracle","host":"localhost","port":1521,"user":"ci","database":"MBTEST","credentials_file":"$WORK/identity"}
EOF
if "$CLI" plugins database-backup --profile-json "$WORK/oracle.json" --root "$WORK/store" --identity "$WORK/identity" --label forged-oracle >/dev/null 2>&1; then echo "FAIL: Oracle bypassed unsupported Rust adapter" >&2; exit 1; fi
# Both field scripts must refuse an unapproved lab BEFORE executing system commands.
if (unset LUSTRE_FULL_MOUNT LUSTRE_INCREMENTAL_MOUNT LUSTRE_RESTORE_MOUNT LUSTRE_SNAPSHOT_ATTESTED; bash scripts/test-real-lustre-roundtrip.sh) >/dev/null 2>&1; then echo "FAIL: Lustre accepted absent lab authorization" >&2; exit 1; fi
if (unset ORACLE_HOME ORACLE_SID MB_ORACLE_EXPECTED_DB_NAME MB_ORACLE_LAB_APPROVAL; bash scripts/test-oracle-rman-lab.sh) >/dev/null 2>&1; then echo "FAIL: Oracle accepted absent lab authorization" >&2; exit 1; fi
echo "PASS hosted negative tests: fake Lustre mounts rejected; Oracle RMAN remains non-executable; field labs require explicit authorization"
