#!/usr/bin/env bash
# Genuine Lustre guest Rust adapter acceptance. Called only from
# test-lustre-ltvm-hosted.sh AFTER the pinned QEMU Lustre guest is running.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(id -u)" -ne 0
REPO_ROOT="$1"
test -f "$REPO_ROOT/tests/lustre/ltvm-guest-rust-r182.sh"
test -x "$REPO_ROOT/rust/target/x86_64-unknown-linux-musl/release/multi-backup-native"
command -v age && command -v age-keygen && command -v sg
# Static guest-native binary only. No ABI/glibc drift masked by a script.
BIN="$REPO_ROOT/rust/target/x86_64-unknown-linux-musl/release/multi-backup-native"
file "$BIN" | grep -Eqi '(static|statically linked)'
STAGE="$(mktemp -d)"
PID=""
cleanup(){
  set +e
  test -z "$PID" || kill "$PID" >/dev/null 2>&1 || :
  rm -rf -- "$STAGE"
}
trap cleanup EXIT
cp "$BIN" "$STAGE/multi-backup-native"
cp "$(command -v age)" "$STAGE/age"
cp "$(command -v age-keygen)" "$STAGE/age-keygen"
# Go age helpers on Ubuntu are usually static; refuse cross-distro glibc.
for name in age age-keygen;do
  file "$STAGE/$name" | grep -Eqi '(static|statically linked)' ||
     { echo "FAIL: $name not guest-architecture static";exit 1; }
done
PORT="$(python3 - <<'PY'
import socket
with socket.socket() as s:
    s.bind(('0.0.0.0',0))
    print(s.getsockname()[1])
PY
)"
# Serve only executable bytes on this short-lived hosted runner/bridge.
python3 -m http.server "$PORT" --bind 0.0.0.0 --directory "$STAGE" \
  >"$STAGE/http.log" 2>&1 &
PID=$!
sleep 2
kill -0 "$PID"
BINARY_SHA="$(sha256sum "$STAGE/multi-backup-native" | awk '{print $1}')"
AGE_SHA="$(sha256sum "$STAGE/age" | awk '{print $1}')"
KEYGEN_SHA="$(sha256sum "$STAGE/age-keygen" | awk '{print $1}')"
SCRIPT_B64="$(base64 -w0 "$REPO_ROOT/tests/lustre/ltvm-guest-rust-r182.sh")"
sg ltvm -c "LTVM_NO_UPDATE=1 ./ltvm vm exec co1-single bash -lc 'printf %s $SCRIPT_B64 | base64 -d | MB_LUSTRE_HTTP_PORT=$PORT MB_LUSTRE_BINARY_SHA=$BINARY_SHA MB_LUSTRE_AGE_SHA=$AGE_SHA MB_LUSTRE_KEYGEN_SHA=$KEYGEN_SHA bash'"
echo 'PASS: QEMU genuine Lustre guest ran exact Rust static native CLI and read back encrypted remote archive'
