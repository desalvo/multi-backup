#!/usr/bin/env bash
# Normalize GitHub Actions' retained Windows installer directory without
# changing a single installer byte. The original artifact remains intact.
set -euo pipefail
[[ "$#" -eq 2 ]] || { echo 'Expected GUI and agent artifact collections' >&2; exit 2; }
gui="$1/rust-only-windows-x64-preview"
agent="$2/rust-agent-windows-x64-preview"
normalize() {
  local folder="$1" filename="$2"
  local source="$folder/installers/$filename"
  local target="$folder/$filename"
  [[ -d "$folder" && ! -L "$folder" ]] || { echo 'Missing trusted artifact folder' >&2; exit 2; }
  [[ -f "$source" && ! -L "$source" ]] || { echo "Missing real Windows installer $filename" >&2; exit 2; }
  [[ ! -e "$target" && ! -L "$target" ]] || { echo "Refusing to overwrite $target" >&2; exit 2; }
  cp -- "$source" "$target"
  cmp -s -- "$source" "$target" || { echo 'Windows installer changed during normalization' >&2; exit 2; }
  [[ "$(sha256sum "$source" | cut -d' ' -f1)" == \
     "$(sha256sum "$target" | cut -d' ' -f1)" ]] || { echo 'Windows installer SHA mismatch' >&2; exit 2; }
  echo "PASS normalized unmodified native $filename"
}
normalize "$gui" "multi-backup-rust-preview-windows-x64.exe"
normalize "$agent" "multi-backup-agent-preview-windows-x64.exe"
