#!/usr/bin/env bash
# Preview acceptance with a local synthetic rclone executable; NO live provider.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/rust/target/debug/multi-backup-native"
test -x "$BIN"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"
mkdir -p "$TMP/mock/dropbox/Backup" "$TMP/mock/nextcloud/Progetti" \
         "$TMP/mock/icloud/Documenti" "$TMP/bin" "$TMP/private"
chmod 700 "$TMP/private"
cat > "$TMP/bin/rclone" <<'PY'
#!/usr/bin/env python3
import json, os, pathlib, sys, hashlib
from urllib.parse import unquote
args=sys.argv[1:]
assert args[:2]==["--config",args[1]]
args=args[2:]
assert args[:3]==["--retries","1","--disable-http2"]
args=args[3:]
command=args.pop(0)
root=pathlib.Path(os.environ["FAKE_CLOUD_ROOT"])
def locate(arg):
    provider,sep,path=arg.partition(":")
    assert sep and provider in {"dropbox","nextcloud","icloud","google-drive"}
    assert ".." not in path.split("/")
    return root/provider/path
if command=="lsjson":
    assert args[:2]==["--max-depth","1"]
    directory=locate(args[2])
    assert directory.is_dir()
    print(json.dumps([{"Name":v.name,"IsDir":v.is_dir(),
        "Size":v.stat().st_size if v.is_file() else -1,
        "ModTime":"2026-11-18T00:00:00Z"} for v in sorted(directory.iterdir())]))
elif command=="cat":
    sys.stdout.buffer.write(locate(args[0]).read_bytes())
elif command=="rcat":
    assert args[0]=="--ignore-existing"
    file=locate(args[1])
    assert file.parent.is_dir()
    with file.open("xb") as f:
        f.write(sys.stdin.buffer.read())
elif command=="deletefile":
    file=locate(args[0])
    assert file.is_file()
    file.unlink()
else:
    raise AssertionError("Unexpected rclone command "+command)
PY
chmod 700 "$TMP/bin/rclone"
export PATH="$TMP/bin:$PATH" FAKE_CLOUD_ROOT="$TMP/mock"
cat > "$TMP/private/rclone.conf" <<'CFG'
[dropbox]
type=dropbox
[google-drive]
type=drive
[icloud]
type=iclouddrive
service=drive
[nextcloud]
type=webdav
vendor=nextcloud
CFG
chmod 600 "$TMP/private/rclone.conf"
echo 'Test data for real native cloud adapter.' > "$TMP/mock/nextcloud/Progetti/readme.txt"
echo 'Cloud Drive upload' > "$TMP/private/upload.txt"
"$BIN" plugins cloud-list --provider nextcloud --config "$TMP/private/rclone.conf" \
  --directory Progetti --json > "$TMP/private/list.json"
python3 - "$TMP/private/list.json" <<'PY'
import json,sys
data=json.load(open(sys.argv[1]))
assert data["provider"]=="nextcloud"
assert data["entries"][0]["name"]=="readme.txt"
PY
"$BIN" plugins cloud-download --provider nextcloud --config "$TMP/private/rclone.conf" \
  --path Progetti/readme.txt --destination "$TMP/private/download.txt" --json >/dev/null
cmp "$TMP/private/download.txt" "$TMP/mock/nextcloud/Progetti/readme.txt"
"$BIN" plugins cloud-upload --provider dropbox --config "$TMP/private/rclone.conf" \
  --directory Backup --file "$TMP/private/upload.txt" --json > "$TMP/private/receipt.json"
python3 - "$TMP" <<'PY'
import json,sys,pathlib,hashlib
root=pathlib.Path(sys.argv[1])
item=json.loads((root/"private/receipt.json").read_text())
assert item["verified"] and not item["atomic_create_guaranteed"]
assert item["new_path"].startswith("Backup/mb-new-")
raw=(root/"mock/dropbox"/item["new_path"]).read_bytes()
assert raw==(root/"private/upload.txt").read_bytes()
PY
if "$BIN" plugins cloud-list --provider nextcloud --config "$TMP/private/rclone.conf" \
    --directory ../root --json >/dev/null 2>&1; then
  echo 'ERROR cloud traversal accepted' >&2; exit 1
fi
if "$BIN" plugins cloud-list --provider local --config "$TMP/private/rclone.conf" \
    --json >/dev/null 2>&1; then
  echo 'ERROR local backend masquerading as cloud' >&2; exit 1
fi
HASH="$(sha256sum "$TMP/mock/nextcloud/Progetti/readme.txt" | awk '{print $1}')"
if "$BIN" plugins cloud-delete --provider nextcloud --config "$TMP/private/rclone.conf" \
    --path Progetti/readme.txt --expected-sha256 "$HASH" --confirm WRONG >/dev/null 2>&1; then
  echo 'ERROR delete without explicit confirmation accepted' >&2; exit 1
fi
test -f "$TMP/mock/nextcloud/Progetti/readme.txt"
"$BIN" plugins cloud-delete --provider nextcloud --config "$TMP/private/rclone.conf" \
    --path Progetti/readme.txt --expected-sha256 "$HASH" \
    --confirm DELETE:nextcloud:Progetti/readme.txt --json >/dev/null
test ! -e "$TMP/mock/nextcloud/Progetti/readme.txt"
echo 'PASS cloud browser list, download, create-new, delete and fail-closed negative tests (fake rclone only)'
