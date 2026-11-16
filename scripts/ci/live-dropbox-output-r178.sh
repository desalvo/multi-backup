#!/usr/bin/env bash
# Manual-only REAL Dropbox B-Out field acceptance on a disposable test account.
set -Eeuo pipefail
umask 077
[ "${GITHUB_ACTIONS:-}" = true ] &&
[ "${RUNNER_ENVIRONMENT:-}" = github-hosted ] &&
[ "${FIELD_CI_APPROVAL:-}" = ISOLATED-GITHUB-DROPBOX-TEST-ACCOUNTS ] || {
  echo "REFUSED: dedicated test account approval required" >&2; exit 3;
}
: "${MULTI_BACKUP_DROPBOX_TOKEN:?Scoped disposable token required}"
: "${MULTI_BACKUP_DROPBOX_TEST_ACCOUNT_ID:?Pinned disposable account ID required}"
: "${GITHUB_RUN_ID:?Unique CI run required}"
: "${GITHUB_RUN_ATTEMPT:?CI attempt required}"
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
TEMP="$(mktemp -d)"
trap 'rm -rf -- "$TEMP"' EXIT
chmod 700 "$TEMP"
python3 - <<'PY'
import json,os,urllib.request
r=urllib.request.Request("https://api.dropboxapi.com/2/users/get_current_account",
    data=b"null",headers={"Authorization":"Bearer "+os.environ["MULTI_BACKUP_DROPBOX_TOKEN"],
    "Content-Type":"application/json"},method="POST")
with urllib.request.urlopen(r,timeout=20) as response:
    a=json.load(response)
if a.get("account_id")!=os.environ["MULTI_BACKUP_DROPBOX_TEST_ACCOUNT_ID"]:
    raise SystemExit("REFUSED: Dropbox CI account mismatch")
print("PASS: disposable Dropbox account identity confirmed")
PY
mkdir -p "$TEMP/source/nested"
printf 'dropbox Rust B-Out encrypted test\n' > "$TEMP/source/nested/hello.txt"
age-keygen -o "$TEMP/key.age" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TEMP/key.age")"
REMOTE="/mb-ci-r178-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}.tar.gz.age"
"$CLI" plugins dropbox-backup --source "$TEMP/source" \
   --remote-path "$REMOTE" --recipient "$RECIPIENT" --json > "$TEMP/receipt.json"
export MB_TEST_REMOTE="$REMOTE" MB_TEST_TMP="$TEMP"
python3 - <<'PY'
import hashlib,json,os,urllib.request
from pathlib import Path
folder=Path(os.environ["MB_TEST_TMP"])
receipt=json.loads((folder/"receipt.json").read_text())
assert receipt["provider"]=="dropbox" and receipt["create_only"] is True
assert receipt["overwrite"] is False and receipt["metadata_readback_verified"] is True
assert receipt["path"]==os.environ["MB_TEST_REMOTE"]
file_id=receipt["id"]
r=urllib.request.Request("https://content.dropboxapi.com/2/files/download",
  data=b"",headers={"Authorization":"Bearer "+os.environ["MULTI_BACKUP_DROPBOX_TOKEN"],
                     "Dropbox-API-Arg":json.dumps({"path":file_id})},method="POST")
with urllib.request.urlopen(r,timeout=120) as response:
    meta=json.loads(response.headers["Dropbox-API-Result"])
    assert meta["id"]==file_id and meta["rev"]==receipt["rev"]
    with (folder/"download.tar.gz.age").open("xb") as f:
        while True:
            chunk=response.read(1024*1024)
            if not chunk:break
            f.write(chunk)
payload=(folder/"download.tar.gz.age").read_bytes()
sha=hashlib.sha256(payload).hexdigest()
blocks=[hashlib.sha256(payload[i:i+4*1024*1024]).digest()
        for i in range(0,len(payload),4*1024*1024)]
block_hash=hashlib.sha256(b"".join(blocks)).hexdigest()
if sha!=receipt["sha256"] or block_hash!=receipt["content_hash"] or len(payload)!=receipt["bytes"]:
    raise SystemExit("Dropbox content differs from encrypted receipt")
print("PASS: Dropbox download and digest confirmed")
PY
age -d -i "$TEMP/key.age" "$TEMP/download.tar.gz.age" | tar -tzf - > "$TEMP/members.txt"
grep -q 'nested/hello.txt' "$TEMP/members.txt"
echo "PASS real Dropbox B-Out: new encrypted object, metadata readback and age recovery"
echo "Disposable remote object intentionally retained; no delete/move API used"
