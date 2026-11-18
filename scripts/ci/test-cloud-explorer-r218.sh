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
    file=locate(args[0])
    sys.stdout.buffer.write(file.read_bytes())
    # Deterministic concurrent index-replacement fault injection, triggered
    # only AFTER the volume was read so the postflight reread must detect it.
    if os.environ.get("FAKE_CLOUD_MUTATE_INDEX_ON_VOLUME")=="1" and file.name=="part-000001.zip":
        index=file.parent/"bundle.index.age"
        with index.open("ab") as output:
            output.write(b"concurrent-catalog-replacement")
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
# Full cloud portable bundle acceptance: real AGE, real XZ compression, real
# chunk parity/manifest and ZIP64. Only the rclone transport is synthetic.
mkdir -p "$TMP/private/source" "$TMP/private/work" "$TMP/mock/nextcloud/Backups"
printf 'Multi Backup r218 authenticated XZ recovery\n' > "$TMP/private/source/restore.txt"
age-keygen -o "$TMP/private/key.txt" >/dev/null 2>&1
RECIPIENT="$(age-keygen -y "$TMP/private/key.txt")"
"$BIN" storage init --root "$TMP/private/store" --recipient "$RECIPIENT" \
  --compressor xz --chunk-mib 1
printf '[{"namespace":"docs","directory":"%s"}]\n' "$TMP/private/source" \
  > "$TMP/private/sources.json"
"$BIN" storage backup --root "$TMP/private/store" --identity "$TMP/private/key.txt" \
  --label "Cloud xz smoke" --sources-json "$TMP/private/sources.json" \
  > "$TMP/private/snapshot.json"
ID="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"])' "$TMP/private/snapshot.json")"
"$BIN" storage export-zip --root "$TMP/private/store" --identity "$TMP/private/key.txt" \
  --id "$ID" --destination "$TMP/private/bundle" --volume-mib 8
cp -a "$TMP/private/bundle" "$TMP/mock/nextcloud/Backups/snap1"
"$BIN" plugins cloud-backup-inspect --config "$TMP/private/rclone.conf" \
  --provider nextcloud --directory Backups/snap1 --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --max-bytes 16777216 --json \
  > "$TMP/private/verified-index.json"
python3 - "$TMP/private/verified-index.json" "$ID" <<'PY'
import json,sys
record=json.load(open(sys.argv[1]))
assert record["age_authenticated"] and record["sha256_verified"]
assert record["remote_index_revalidated"] and not record["remote_version_atomic"]
assert len(record["remote_index_sha256"])==64
assert record["backup_id"]==sys.argv[2]
assert record["files"]==[{"path":"docs/restore.txt","size":len(b"Multi Backup r218 authenticated XZ recovery\n")}]
PY
"$BIN" plugins cloud-backup-restore --config "$TMP/private/rclone.conf" \
  --provider nextcloud --directory Backups/snap1 --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --destination "$TMP/private/recovered" \
  --selection docs/restore.txt --max-bytes 16777216 \
  --confirm RESTORE:nextcloud:Backups/snap1 --json > "$TMP/private/restore.json"
cmp "$TMP/private/source/restore.txt" "$TMP/private/recovered/docs/restore.txt"
# Explicit refusal of existing destinations and wrong restore confirmations.
if "$BIN" plugins cloud-backup-restore --config "$TMP/private/rclone.conf" \
  --provider nextcloud --directory Backups/snap1 --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --destination "$TMP/private/recovered" \
  --confirm RESTORE:nextcloud:Backups/snap1 >/dev/null 2>&1; then
  echo 'ERROR remote restore overwrote an existing local destination' >&2;exit 1
fi
if "$BIN" plugins cloud-backup-restore --config "$TMP/private/rclone.conf" \
  --provider nextcloud --directory Backups/snap1 --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --destination "$TMP/private/unauthorized" \
  --confirm BAD >/dev/null 2>&1; then
  echo 'ERROR cloud recovery accepted incorrect explicit confirmation' >&2;exit 1
fi
test ! -e "$TMP/private/unauthorized"
# Detect a catalog swap after the first ZIP volume has been streamed. The
# synthetic mock mutates only the remote ciphertext index; restore MUST fail
# before creating any selected plaintext target.
mkdir -p "$TMP/mock/nextcloud/Backups/snap-swap"
cp -a "$TMP/mock/nextcloud/Backups/snap1/." "$TMP/mock/nextcloud/Backups/snap-swap/"
if FAKE_CLOUD_MUTATE_INDEX_ON_VOLUME=1 "$BIN" plugins cloud-backup-restore \
  --config "$TMP/private/rclone.conf" --provider nextcloud \
  --directory Backups/snap-swap --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --destination "$TMP/private/replaced-catalog-out" \
  --confirm RESTORE:nextcloud:Backups/snap-swap --max-bytes 16777216 \
  >/dev/null 2>&1; then
  echo 'ERROR concurrent encrypted index replacement was accepted' >&2;exit 1
fi
test ! -e "$TMP/private/replaced-catalog-out"
# Fail on verified ciphertext corruption; never create a plaintext target.
printf 'tampered' >> "$TMP/mock/nextcloud/Backups/snap1/part-000001.zip"
if "$BIN" plugins cloud-backup-restore --config "$TMP/private/rclone.conf" \
  --provider nextcloud --directory Backups/snap1 --identity "$TMP/private/key.txt" \
  --work-dir "$TMP/private/work" --destination "$TMP/private/corrupt-out" \
  --confirm RESTORE:nextcloud:Backups/snap1 >/dev/null 2>&1; then
  echo 'ERROR tampered encrypted volume accepted' >&2;exit 1
fi
test ! -e "$TMP/private/corrupt-out"
echo 'PASS remote AGE/XZ ZIP64 multi-step authenticated selective restore with tamper negatives'

echo 'PASS cloud browser list, download, create-new, delete and fail-closed negative tests (fake rclone only)'
