#!/usr/bin/env bash
# Real Linux POSIX encrypted backup/restore and negative acceptance.
set -euo pipefail
umask 077
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/source/sub/one" "$WORK/source/unicodé"
printf 'POSIX full snapshot\n' > "$WORK/source/README.txt"
printf '\x00\xff\x01\x7f' > "$WORK/source/sub/one/binary.dat"
: > "$WORK/source/empty.dat"
printf 'UTF-8\n' > "$WORK/source/unicodé/é.txt"
dd if=/dev/urandom of="$WORK/source/sub/one/random.dat" bs=1M count=10 status=none
age-keygen -o "$WORK/key" 2>/dev/null
RECIPIENT=$(sed -n 's/^# public key: //p' "$WORK/key" | head -1)
"$CLI" storage init --root "$WORK/store" --recipient "$RECIPIENT"
"$CLI" plugins backup --input posix --output posix \
    --source "$WORK/source" --root "$WORK/store" \
    --identity "$WORK/key" --label real-posix-full \
    --namespace files --top-namespace project --json > "$WORK/full.json"
FULL=$(jq -r .id "$WORK/full.json")
test "$(printf %s "$FULL" | wc -c)" -eq 64
test "$(jq '.entries | length' "$WORK/full.json")" -eq 5
"$CLI" storage verify --root "$WORK/store" --identity "$WORK/key" --id "$FULL"
"$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$FULL" --destination "$WORK/recovered-full" --json
for rel in README.txt sub/one/binary.dat sub/one/random.dat empty.dat unicodé/é.txt; do
    cmp "$WORK/source/$rel" "$WORK/recovered-full/project/files/$rel"
done
test "$(stat -c %s "$WORK/recovered-full/project/files/sub/one/random.dat")" = 10485760
test "$(stat -c %s "$WORK/recovered-full/project/files/empty.dat")" = 0
if "$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$FULL" --destination "$WORK/recovered-full"; then
    echo 'FAIL: existing directory overwritten' >&2; exit 1
fi
"$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$FULL" --destination "$WORK/select" \
    --selection project/files/sub/one/binary.dat --json
cmp "$WORK/source/sub/one/binary.dat" "$WORK/select/project/files/sub/one/binary.dat"
test ! -e "$WORK/select/project/files/README.txt"
if "$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$FULL" --destination "$WORK/no-selection" \
    --selection project/files/never-exists; then
    echo 'FAIL: missing selection accepted' >&2; exit 1
fi
test ! -e "$WORK/no-selection"
printf 'POSIX incremented\n' > "$WORK/source/README.txt"
printf 'created after baseline\n' > "$WORK/source/sub/one/new.dat"
rm "$WORK/source/sub/one/binary.dat"
"$CLI" plugins backup --input posix --output posix \
    --source "$WORK/source" --root "$WORK/store" --identity "$WORK/key" \
    --label real-posix-incremental --namespace files --top-namespace project \
    --mode incremental --parent "$FULL" --json > "$WORK/inc.json"
INC=$(jq -r .id "$WORK/inc.json")
test "$(jq -r .kind "$WORK/inc.json")" = incremental
test "$(jq -r .parent_id "$WORK/inc.json")" = "$FULL"
"$CLI" storage verify-chain --root "$WORK/store" --identity "$WORK/key" --id "$INC" --json
"$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$INC" --destination "$WORK/recovered-inc" --json
cmp "$WORK/source/README.txt" "$WORK/recovered-inc/project/files/README.txt"
cmp "$WORK/source/sub/one/new.dat" "$WORK/recovered-inc/project/files/sub/one/new.dat"
test ! -e "$WORK/recovered-inc/project/files/sub/one/binary.dat"
if "$CLI" plugins backup --input posix --output posix \
    --source "$WORK/source" --root "$WORK/store" --identity "$WORK/key" \
    --label wrong-ancestor --namespace files --top-namespace other \
    --mode incremental --parent "$FULL"; then
    echo 'FAIL: cross-namespace incremental accepted' >&2; exit 1
fi
BEFORE=$("$CLI" storage list --root "$WORK/store" --identity "$WORK/key" --json | jq length)
ln -s /etc/passwd "$WORK/source/host-file-link"
if "$CLI" plugins backup --input posix --output posix --source "$WORK/source" \
    --root "$WORK/store" --identity "$WORK/key" --label symlink-not-allowed; then
    echo 'FAIL: source symlink accepted' >&2; exit 1
fi
rm "$WORK/source/host-file-link"
mkfifo "$WORK/source/unsafe-fifo"
if "$CLI" plugins backup --input posix --output posix --source "$WORK/source" \
    --root "$WORK/store" --identity "$WORK/key" --label fifo-not-allowed; then
    echo 'FAIL: source FIFO accepted' >&2; exit 1
fi
rm "$WORK/source/unsafe-fifo"
test "$BEFORE" = "$("$CLI" storage list --root "$WORK/store" --identity "$WORK/key" --json | jq length)"
age-keygen -o "$WORK/wrong" 2>/dev/null
if "$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/wrong" --id "$FULL" --destination "$WORK/wrong-output"; then
    echo 'FAIL: bad age identity restored' >&2; exit 1
fi
test ! -e "$WORK/wrong-output"
FIRST=$(jq -r '.entries[] | select(.path=="project/files/sub/one/random.dat") | .chunks[0].id' "$WORK/full.json")
test "$(printf %s "$FIRST" | wc -c)" -eq 64
FIRST_A=$(printf %s "$FIRST" | cut -c1-2)
FIRST_B=$(printf %s "$FIRST" | cut -c3-4)
ENC="$WORK/store/.multi-backup-v1/chunks/$FIRST_A/$FIRST_B/$FIRST.age"
python3 - "$ENC" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1]); b=bytearray(p.read_bytes())
assert len(b)>256
b[len(b)//2] ^= 0x51
p.write_bytes(b)
PY
"$CLI" plugins restore --input posix --output posix --root "$WORK/store" \
    --identity "$WORK/key" --id "$FULL" --destination "$WORK/parity-restore" --json
cmp "$WORK/source/sub/one/random.dat" "$WORK/parity-restore/project/files/sub/one/random.dat"
"$CLI" storage verify --root "$WORK/store" --identity "$WORK/key" --id "$FULL"
echo "PASS POSIX E2E: full + incremental, 10MiB binary/zero/UTF-8, selection, parity repair, blocked overwrite/symlink/FIFO/wrong key"
