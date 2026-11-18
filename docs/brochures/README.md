# Brochure illustrative bilingui — Multi Backup r218

**Pubblicazione corrente verificata — 18 novembre 2026.**
La CI [#44295235864](https://github.com/desalvo/multi-backup/actions/runs/44295235864)
ha acquisito l'interfaccia desktop Rust egui reale, il frontend server attuale
con dati sintetici marcati e la viewport responsive, poi ha rigenerato, validato
e versionato entrambi i PDF. L'impaginazione è stata riesaminata visivamente:
5 pagine ciascuna senza pagine orfane.

- [Brochure illustrativa italiana — 5 pagine](Multi-Backup-Brochure-IT.pdf)
- [Illustrative brochure English — 5 pages](Multi-Backup-Brochure-EN.pdf)

**Fonte esatta:** `1ea5c44b4c6c447c3c36ad7ea4de2421fe993042`;
pubblicazione automatica `10ace3d15cfb3136ad3cf5a4a5738a477de77979`.
Immagini riproducibili: `../screenshots-current/`.
Artefatto GitHub Actions: `multi-backup-r218-current-rust-manuals-brochures-screenshots`,
include i quattro PDF, screenshot e `SHA256SUMS`.

**Rigenerazione:**
`python3 scripts/build-brochure-r218.py --screenshots generated-screenshots --out generated-publications`
dopo cattura nativa nel workflow `.github/workflows/brochures-r218.yml`.
Il generatore non accetta schermate Python/Tk né fallback: fallisce se
mancano le nuove screenshot Rust.

**Ambito:** r218 resta una preview di sviluppo. I 16 percorsi READY (su un
catalogo di 34 plugin) sono delimitati per operazione, non costituiscono
certificazione universale dei provider. Mancano ancora alcune firme,
notarizzazione, collaudi cloud con account reali, dispositivi fisici mobili
e HA fisica completa. Le brochure delle generazioni precedenti appartengono
all'archivio storico; non utilizzare le vecchie immagini come corrente UI.
Licenza EUPL-1.2.
