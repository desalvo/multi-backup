#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Uso: $0 DIRECTORY_BACKUP [PERCORSO_CHIAVE_PRIVATA_age]" >&2
  exit 2
fi
root="$1"
identity="${2:-}"
[[ -d "$root" ]] || { echo "Directory inesistente: $root" >&2; exit 1; }
this_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
count=0
failed=0
while IFS= read -r -d '' archive; do
  count=$((count + 1))
  echo "[$count] Verifica: $archive"
  if [[ -n "$identity" ]]; then
    if ! "$this_dir/verify_archive.sh" "$archive" "$identity"; then
      echo "ERRORE: $archive" >&2
      failed=$((failed + 1))
    fi
  else
    if ! "$this_dir/verify_archive.sh" "$archive"; then
      echo "ERRORE: $archive" >&2
      failed=$((failed + 1))
    fi
  fi
done < <(find "$root" -type f -name '*.tar.gz.age' -print0)
echo "Archivi controllati: $count, falliti: $failed"
if [[ $count -eq 0 || $failed -ne 0 ]]; then exit 1; fi
