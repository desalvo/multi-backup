# Multi Backup 0.1.0 — RC candidate scope r055

NON PUBBLICABILE finché i gate finali non saranno superati. PR #21 resta draft.

## Migrazione Rust

Core cifrato, CLI multi-backup-native, server/API rust-server, GUI eframe rust-desktop e agente multi-backup-agent sono implementati in Rust e hanno superato build native su macOS/Windows/Linux in una full CI precedente (43819587305). Questi esiti attestano i build, non equivalenza completa di tutte le funzioni legacy o degli oltre trenta provider registrati. Python legacy resta come compatibilità/migrazione e test; gli installer Rust sono Python-free ma la parità di tutti i plugin NON è completata.

Il plugin S3 WORM è un comando Rust dedicato (s3-vault), non un esecutore generico s3->arbitrary. MinIO e RustFS sono testati sul profilo S3 WORM single-server. Ceph RGW necessita attestazione indipendente. I plugin solo PLANNED/PREVIEW restano non abilitati all'esecuzione generica.

## Gate necessari prima di RC

1. Ceph RGW: 3 monitor in quorum, 3 OSD BlueStore UP+IN su worker diversi, RGW reale, Object Lock COMPLIANCE, backup/restore e rifiuto delete versione.
2. Nuova full Production CI sull'ultimo SHA, senza scambiare successi di SHA vecchi per la qualifica finale.
3. Restore disaster su destinazione isolata, evidenza firmata di integrità e controllo manifest/delete marker.
4. GitHub e Dropbox: usare account realmente disposable con credenziali di test autorizzate, non dati personali. Certificare l'intero ciclo e API rate limit.
5. Validare installazione e auto-update/rollback su dispositivi reali, firma e notarizzazione richieste per release distribuite.
6. Escludere esplicitamente dalla prima RC i plugin pianificati e GPFS sperimentale (quest'ultimo non bloccante).

Non pubblicare stable, tag, release o Docker image senza evidenze e autorizzazioni/credenziali di firma appropriate. Non inventare certificazioni.

Dettaglio prove: docs/PLUGIN_CERTIFICATION_R055_IT.md.
