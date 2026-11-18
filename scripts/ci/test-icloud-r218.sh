#!/usr/bin/env bash
# Offline actual Rust + age E2E on a fake rclone transport, not an Apple test.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$ROOT/rust/target/debug/multi-backup-native"
test -x "$BIN"
command -v age
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
chmod 700 "$WORK"
mkdir -p "$WORK/bin" "$WORK/mock/Backup" "$WORK/private"
chmod 700 "$WORK/private" "$WORK/mock"
cat >"$WORK/bin/rclone" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = --config; test -r "$2"; shift 2
test "$1" = --disable-http2; shift
command="$1"; shift
case "$command" in
  cat)
    test $# -eq 1
    case "$1" in icloud:*) ;; *) exit 2;; esac
    target="$RCLONE_FAKE_ROOT/$(printf '%s' "$1" | cut -c8-)"
    test -f "$target"
    cat -- "$target"
    ;;
  copyto)
    test "$1" = --ignore-existing; shift
    test "$1" = --retries; test "$2" = 1; shift 2
    test $# -eq 2
    source="$1"; remote="$2"
    case "$remote" in icloud:*) ;; *) exit 2;; esac
    target="$RCLONE_FAKE_ROOT/$(printf '%s' "$remote" | cut -c8-)"
    test ! -e "$target"
    test -d "$(dirname "$target")"
    cp -n -- "$source" "$target"
    ;;
  *) exit 3;;
esac
SH
chmod 700 "$WORK/bin/rclone"
export PATH="$WORK/bin:$PATH" RCLONE_FAKE_ROOT="$WORK/mock"
cat >"$WORK/private/rclone.conf" <<'CONF'
[icloud]
type = iclouddrive
service = drive
apple_id = user@fake.invalid
CONF
chmod 600 "$WORK/private/rclone.conf"
age-keygen -o "$WORK/private/key.txt" >"$WORK/private/key-info" 2>&1
chmod 600 "$WORK/private/key.txt"
RECIPIENT="$(grep 'Public key:' "$WORK/private/key-info" | awk '{print $3}')"
test -n "$RECIPIENT"
printf 'Multi Backup iCloud B-In/B-Out fixture\nUnicode: 𝄞\n' > "$WORK/mock/Backup/source.txt"
EXPECTED="$(sha256sum "$WORK/mock/Backup/source.txt" | awk '{print $1}')"
"$BIN" plugins icloud-import --config "$WORK/private/rclone.conf" \
  --remote icloud:Backup/source.txt --recipient "$RECIPIENT" \
  --expected-sha256 "$EXPECTED" --destination "$WORK/private/imported.age" \
  --json >"$WORK/private/import.json"
test -s "$WORK/private/imported.age"
age -d -i "$WORK/private/key.txt" "$WORK/private/imported.age" \
  | cmp - "$WORK/mock/Backup/source.txt"
CIPHER="$(sha256sum "$WORK/private/imported.age" | awk '{print $1}')"
"$BIN" plugins icloud-upload --config "$WORK/private/rclone.conf" \
  --remote-parent icloud:Backup --archive "$WORK/private/imported.age" \
  --identity "$WORK/private/key.txt" --expected-sha256 "$CIPHER" \
  --json >"$WORK/private/upload.json"
python3 - "$WORK" <<'PY'
import json,pathlib,sys,hashlib
root=pathlib.Path(sys.argv[1])
source=json.loads((root/"private/import.json").read_text())
uploaded=json.loads((root/"private/upload.json").read_text())
assert source["direction"]=="b-in"
assert uploaded["direction"]=="b-out"
assert uploaded["verified_remote_readback"] is True
assert uploaded["provider_atomic_if_none_match"] is False
remote=uploaded["remote"]
assert remote.startswith("icloud:Backup/mb-") and remote.endswith(".age")
actual=(root/"mock"/remote.removeprefix("icloud:")).read_bytes()
assert hashlib.sha256(actual).hexdigest()==uploaded["ciphertext_sha256"]
assert actual==(root/"private/imported.age").read_bytes()
PY
if "$BIN" plugins icloud-import --config "$WORK/private/rclone.conf" \
  --remote icloud:Backup/source.txt --recipient "$RECIPIENT" \
  --expected-sha256 "$(printf '0%.0s' {1..64})" \
  --destination "$WORK/private/must-not-exist.age" >/dev/null 2>&1; then
  echo "ERROR: wrong digest accepted" >&2; exit 1
fi
test ! -e "$WORK/private/must-not-exist.age"
echo "not-age" >"$WORK/private/plain.age"
if "$BIN" plugins icloud-upload --config "$WORK/private/rclone.conf" \
  --remote-parent icloud:Backup --archive "$WORK/private/plain.age" \
  --identity "$WORK/private/key.txt" \
  --expected-sha256 "$(sha256sum "$WORK/private/plain.age" | awk '{print $1}')" >/dev/null 2>&1; then
  echo "ERROR: plain file accepted as authenticated age" >&2; exit 1
fi
echo "PASS real Rust/age and synthetic rclone B-In/B-Out; NOT an Apple live field certification"
