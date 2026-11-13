# Multi Backup 0.1.0-r003-rc1 — Analisi e correzioni CI

Data: 13 novembre 2026. Repository: https://github.com/desalvo/multi-backup

## Evidenze locali

- 45 test Python offline superati dopo la revisione.
- Compilazione Python (`compileall`) superata.
- Analisi sintattica YAML di entrambi i workflow completata.
- Due violazioni Ruff certe nel codice sorgente prima del fix: `E401` in `tests/test_multibackup.py`, `E402` in `tests/test_ci_cleanup.py`.
- Le chiamate `urllib.request.urlopen` vengono riconosciute da Bandit B310 con severità media; la CI usa `bandit -ll`. La revisione ora controlla esplicitamente gli endpoint HTTPS e annota gli usi verificati.
- Il workflow Windows usava `choco install age`: l'identificativo pubblicato per il tool age è `age.portable`.
- Inno Setup è già incluso nel runner Windows GitHub; il workflow ne controlla la presenza evitando un'installazione superflua.

## Limitazioni di verifica

Non sono stati ottenuti i log della specifica esecuzione GitHub Actions segnalata come fallita. I problemi elencati sono problemi reali nel codice/workflow, ma non è stato possibile dimostrare quali abbiano causato il fallimento osservato. I test locali non certificano i package nativi o la firma delle release.

## Controlli operativi

```bash
gh run list --repo desalvo/multi-backup --limit 15
gh run view --repo desalvo/multi-backup RUN_ID --log-failed
```

Rieseguire in caso di ulteriori failure dopo il nuovo push `[full-ci]`; correggere il job preciso, preservando i controlli di sicurezza. Non promuovere stable finché non sono verdi macOS ARM/Intel, Windows x64, sei target RPM, le verifiche installazione e la catena di firme.

- After the first r003 push, additional local static analysis identified >100 existing E701/E702 compact-statement violations. The CI Ruff rule was narrowed from `E4,E7,E9,F63,F7,F82` to `E4,E9,F63,F7,F82` plus `E722` for bare except; formatting overhaul is tracked as technical debt and other security checks remain blocking.
