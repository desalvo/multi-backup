#!/usr/bin/env bash
# Real eframe/egui Linux GUI creation and rendering under disposable Xvfb.
set -Eeuo pipefail
umask 077
if [[ "$GITHUB_ACTIONS" != true || "$RUNNER_ENVIRONMENT" != github-hosted || "$EUID" -eq 0 ]];then
  echo "REFUSED: genuine native GUI smoke only on unprivileged GitHub-hosted CI" >&2
  exit 3
fi
BIN="rust-desktop/target/debug/multi-backup-desktop"
if [[ ! -x "$BIN" ]];then
  echo "FAIL: actual native Rust desktop executable missing" >&2
  exit 1
fi
for executable in xvfb-run timeout;do command -v "$executable" >/dev/null;done
ROOT="$(mktemp -d)"
trap 'rm -rf -- "$ROOT"' EXIT
mkdir -m 0700 "$ROOT/home" "$ROOT/config"
export HOME="$ROOT/home"
export XDG_CONFIG_HOME="$ROOT/config"
export LIBGL_ALWAYS_SOFTWARE=1
export NO_AT_BRIDGE=1
"$BIN" --self-test > "$ROOT/selftest.log"
grep -Fqx "Native Rust GUI self-test OK" "$ROOT/selftest.log"
if ! xvfb-run -a -s "-screen 0 1440x900x24" timeout 50 \
    "$BIN" --smoke >"$ROOT/stdout.log" 2>"$ROOT/stderr.log";then
  echo "FAIL: Rust eframe window did not create, render and close" >&2
  tail -n 25 "$ROOT/stderr.log" >&2
  exit 1
fi
# One process must render all 10 distinct pages and About without crashes.
# The log contains labels only, never credentials or source content.
actual="$(grep -c '^RUST_GUI_PAGE_SMOKE=' "$ROOT/stdout.log" || true)"
if [[ "$actual" != 11 ]];then
  echo "FAIL: expected ten page renders plus About, saw $actual" >&2
  cat "$ROOT/stdout.log" >&2
  exit 1
fi
for page in Dashboard Backup Archivi Ripristina 'Storage chunk' Plugin 'Server e agenti' 'Vault e chiavi' Aggiornamenti Impostazioni About;do
  grep -Fqx "RUST_GUI_PAGE_SMOKE=$page" "$ROOT/stdout.log" || {
    echo "FAIL: missing rendered page $page" >&2; exit 1;
  }
done
if [[ -e "$ROOT/config/multi-backup/keys/default.agevault" ]];then
  echo "FAIL: smoke created an unexpected encryption vault" >&2
  exit 1
fi
echo "PASS: native Rust egui all 10 pages and About rendered in one Xvfb process"
echo "Scope: Linux GUI runtime only, not macOS/Windows installer, pixel fidelity or backup acceptance"
