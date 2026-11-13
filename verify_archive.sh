#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Uso: $0 ARCHIVIO.tar.gz.age [PERCORSO_CHIAVE_PRIVATA_age]" >&2
  exit 2
fi
archive="$1"
identity="${2:-}"
[[ -f "$archive" ]] || { echo "Archivio non trovato: $archive" >&2; exit 1; }
[[ -f "${archive}.sha256" ]] || { echo "Checksum mancante: ${archive}.sha256" >&2; exit 1; }
python3 - "$archive" <<'PY'
import hashlib, pathlib, sys
p = pathlib.Path(sys.argv[1]); checksum = pathlib.Path(str(p)+'.sha256')
expected = checksum.read_text().split()[0].lower()
h = hashlib.sha256()
with p.open('rb') as f:
    for b in iter(lambda:f.read(8*1024*1024), b''): h.update(b)
if h.hexdigest() != expected:
    sys.exit('SHA-256 NON corrisponde: possibile corruzione del backup')
print('SHA-256: OK')
PY
if [[ -n "$identity" ]]; then
  age -d -i "$identity" "$archive" | tar -tzf - >/dev/null
else
  age -d "$archive" | tar -tzf - >/dev/null
fi
echo "Decifratura e struttura tar.gz: OK"
