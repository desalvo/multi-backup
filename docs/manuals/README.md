# Manuali utente — Multi Backup r218 (IT / EN)

**Versione corrente verificata: r218, documentazione generata dal codice Rust.**
I PDF seguenti sono stati realmente rigenerati e pubblicati dal workflow
[`Multi Backup r218 - current Rust UI manuals and brochures (IT EN)`](../../.github/workflows/brochures-r218.yml)
con esito verde nel [run GitHub Actions #44295235864](https://github.com/desalvo/multi-backup/actions/runs/44295235864).

- [Manuale utente italiano — 9 pagine](Multi-Backup-Manuale-Utente-IT.pdf)
- [User manual English — 9 pages](Multi-Backup-User-Manual-EN.pdf)

**Provenienza:** generati dal codice sorgente esatto
`1ea5c44b4c6c447c3c36ad7ea4de2421fe993042`, pubblicati dal commit CI
`10ace3d15cfb3136ad3cf5a4a5738a477de77979`.
Le immagini sono in `../screenshots-current/` e il corrispondente
`SOURCE_SHA` documenta la versione originaria. Sono schermate vere
dell'applicazione nativa Rust egui avviata in Xvfb, o dell'attuale frontend
server Rust HTML/CSS/JS eseguito con **API fixture sintetiche esplicitamente
marcate**. L'immagine mobile è una viewport browser responsive, **non**
una cattura da dispositivo iOS/Android fisico.

**Rigenerazione:** eseguire prima
`scripts/ci/capture-rust-desktop-r218.py` in CI con il binario Rust,
quindi `scripts/ci/capture-rust-server-r218.py`, e poi
`python3 scripts/build-current-manual-r218.py --screenshots generated-screenshots --out generated-publications`.
Il workflow acquisisce, genera, valida PDF e screenshot, e pubblica solo a test superati.
Lo ZIP con PDF, PNG e SHA256SUMS è disponibile come artefatto del run indicato.

**Limiti:** le procedure sono documentazione tecnica preview, non certificano
firmware, account cloud reali, firma/notarizzazione, app native mobili distribuite
o fault test HA su tre host fisici. I precedenti manuali r018/r038 e screenshot
Python/Tk in `../screenshots/` restano solo materiale storico, non la UI corrente.
Licenza progetto EUPL-1.2.
