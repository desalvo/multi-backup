# Analisi CI Multi Backup - r004

Run: https://github.com/desalvo/multi-backup/actions/runs/43491071561  
Commit: `bd1fcca9f5b4bf028780d3af1302aef53f68b0b5`  
Job fallito: Security checks (`132112094249`)  
Step: Audit runtime Python dependencies (`python -m pip_audit -r requirements.txt`)

## Diagnosi verificata

- Passati: Python 3.11, 3.12, 3.13, Ruff, Bandit.
- `pip-audit` ha individuato 42 advisory su `pillow 12.3.0`, con fix dichiarato `13.0.0`.
- Le dipendenze runtime (`requirements.txt`) e documentazione (`requirements-docs.txt`) limitavano Pillow a `<13`; lo stesso vincolo era codificato nelle tre ricette di build native.

## Risoluzione

- Nuovo intervallo: `Pillow>=13.0.0,<14`, runtime e documentazione.
- Allineate tutte le installazioni Pillow in macOS (Apple Silicon/Intel), Windows 11 e RPM Rocky/AlmaLinux EL8/9/10.
- Aggiunto test di regressione che verifica allineamento vincoli e mantenimento del gate `pip-audit`.
- Pubblicazione anticipata con messaggio monoriga `[full-ci]` e nuova verifica remota necessaria.

**Non considerare il job risolto finché la nuova GitHub Actions run non risulta verde.**
