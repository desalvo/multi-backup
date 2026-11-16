#!/usr/bin/env bash
# Manual read-only Rust GitHub/Dropbox acceptance on isolated test accounts.
# Never use production/personal accounts; no remote write, delete, or restores.
set -Eeuo pipefail
umask 077
[ "${GITHUB_ACTIONS:-}" = true ] && [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ] &&
[ "${FIELD_CI_APPROVAL:-}" = "ISOLATED-GITHUB-DROPBOX-TEST-ACCOUNTS" ] || {
  echo "REFUSED: approved disposable CI test accounts required" >&2; exit 3;
}
mode="${1:?mode github or dropbox}"
case "$mode" in github|dropbox) ;; *) exit 2;; esac
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
chmod 700 "$work"
age-keygen -o "$work/id.age" >/dev/null 2>&1
recipient="$(age-keygen -y "$work/id.age")"
mkdir -m 700 "$work/backup"
if [ "$mode" = github ];then
  : "${GH_TOKEN:?Dedicated GitHub account token required}"
  : "${MULTI_BACKUP_GITHUB_TEST_ACCOUNT:?Dedicated GitHub CI account required}"
  gh api user > "$work/user.json"
  gh api 'user/repos?affiliation=owner,collaborator,organization_member&per_page=100' > "$work/repos.json"
  python3 - "$work/user.json" "$work/repos.json" <<'PY'
import json,os,sys
u=json.load(open(sys.argv[1]))
repos=json.load(open(sys.argv[2]))
expected=os.environ["MULTI_BACKUP_GITHUB_TEST_ACCOUNT"]
if u.get("login")!=expected or not expected.startswith("mb-ci-") or len(repos)!=1:
    raise SystemExit("REFUSED: GitHub source must be ONE isolated mb-ci- test repository")
repo=repos[0]
if repo.get("full_name")!=expected+"/"+repo.get("name","") or not repo.get("name","").startswith("mb-ci-"):
    raise SystemExit("REFUSED: GitHub test account has unrelated repository")
print("PASS dedicated GitHub CI account preflight")
PY
  "$CLI" github backup --output-dir "$work/backup" --recipient "$recipient" \
    --max-runs 1 > "$work/result.json"
else
  : "${MULTI_BACKUP_DROPBOX_TOKEN:?Dedicated Dropbox account token required}"
  : "${MULTI_BACKUP_DROPBOX_TEST_ACCOUNT_ID:?Approved Dropbox account ID required}"
  python3 - <<'PY'
import json,os,urllib.request
req=urllib.request.Request("https://api.dropboxapi.com/2/users/get_current_account",
    data=b"null",headers={"Authorization":"Bearer "+os.environ["MULTI_BACKUP_DROPBOX_TOKEN"],
        "Content-Type":"application/json"},method="POST")
with urllib.request.urlopen(req,timeout=15) as response: account=json.load(response)
if account.get("account_id")!=os.environ["MULTI_BACKUP_DROPBOX_TEST_ACCOUNT_ID"]:
    raise SystemExit("REFUSED: Dropbox token belongs to wrong account")
print("PASS dedicated Dropbox CI account preflight")
PY
  "$CLI" dropbox backup --output-dir "$work/backup" --recipient "$recipient" \
    --remote-path /MultiBackupCI-Disposable > "$work/result.json"
fi
# Require a REAL encrypted archive and fully authenticated decrypt+tar listing.
find "$work/backup" -type f -name '*.tar.gz.age' -print0 > "$work/archives"
test -s "$work/archives"
while IFS= read -r -d '' archive;do
  age -d -i "$work/id.age" "$archive" | tar -tzf - >/dev/null
done < "$work/archives"
echo "PASS $mode: genuine read-only provider backup and authenticated age archive"
echo "NOT CERTIFIED: provider remote write/cross-account restoration"
