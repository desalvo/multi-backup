# Multi Backup r038 — indice documentazione e stato di verifica

**Aggiornamento:** 14 novembre 2026. **Branch:** `feature/r038-rust-primary-plugin-executor` (PR #10, draft). **Ultimo pacchetto pubblicato:** Python/Tk r018; Rust r038 è un'anteprima compilata, non una RC certificata. La migrazione dei vecchi archivi Python è facoltativa.

## Dove trovare ogni procedura aggiornata

| Argomento | Procedura operativa | Architettura/contratti | Stato |
|---|---|---|---|
| Inizializzazione store e chiavi age | [IT](R038_OPERATIONS_IT.md#2-preparazione-di-un-laboratorio-cifrato) · [EN](R038_OPERATIONS_EN.md#2-preparing-encrypted-test-storage) | [Security](SECURITY.md), [Protection](BACKUP_PROTECTION.md) | Anteprima Rust; vault storage ancora parziale |
| Backup FULL/INC manuale | [IT](R038_OPERATIONS_IT.md#3-backup-manuale-totale-e-incrementale) · [EN](R038_OPERATIONS_EN.md#3-manual-full-and-incremental-backups) | [CHUNK_STORAGE_V1](CHUNK_STORAGE_V1.md) | Implementato in codice, qualificazione E2E aperta |
| Numero file e quattro metriche di dimensione | [IT](R038_OPERATIONS_IT.md#4-informazioni-dei-backup-e-catena) · [EN](R038_OPERATIONS_EN.md#4-backup-information-sizes-and-verification) | [CHUNK_STORAGE_V1](CHUNK_STORAGE_V1.md) | Calcolo Rust disponibile; test su dataset reali mancanti |
| Verifica catena completa (default) | [IT](R038_OPERATIONS_IT.md#5-verifica-manuale-o-programmata) · [EN](R038_OPERATIONS_EN.md#4-backup-information-sizes-and-verification) | [Maintenance](MAINTENANCE_POLICY.md) | Manuale/scheduler Rust; allarmi e UI completi da certificare |
| Restore totale/parziale | [IT](R038_OPERATIONS_IT.md#6-restore-integrale-e-parziale) · [EN](R038_OPERATIONS_EN.md#5-restore-and-cross-account-operation) | [Protection](BACKUP_PROTECTION.md) | Restore locale Rust, target solo nuovo |
| Restore su utente GitHub/Dropbox diverso | [IT](R038_OPERATIONS_IT.md#6-restore-integrale-e-parziale) · [EN](R038_OPERATIONS_EN.md#5-restore-and-cross-account-operation) | [Plugin API](PLUGIN_API.md), [Security](SECURITY.md) | Un repository nuovo/un file nuovo in preview; multi-item ancora aperto |
| Montaggi NFS/SMB/SSHFS/GPFS CES | [IT](R038_OPERATIONS_IT.md#3-backup-manuale-totale-e-incrementale) · [EN](R038_OPERATIONS_EN.md#3-manual-full-and-incremental-backups) | [Plugin matrix](PLUGIN_MATRIX.md), [Plugin API](PLUGIN_API.md) | Solo mount Linux preesistente, esatto e read-only |
| Cancellazione incrementi/full e rebase | [IT](R038_OPERATIONS_IT.md#7-cancellazione-rebase-del-successore-e-purge-logico) · [EN](R038_OPERATIONS_EN.md#6-chain-aware-logical-deletion-purge) | [Protection](BACKUP_PROTECTION.md), [Maintenance](MAINTENANCE_POLICY.md) | Manifest-only Rust; power loss non certificato |
| GC con anteprima e conferma | [IT](R038_OPERATIONS_IT.md#8-garbage-collection-recuperare-spazio-senza-perdere-dati) · [EN](R038_OPERATIONS_EN.md#7-separate-explicit-garbage-collection) | [Maintenance](MAINTENANCE_POLICY.md), [CHUNK_STORAGE](CHUNK_STORAGE_V1.md) | GC locale Rust; stripe miste trattenute; repack e GC remota aperti |
| Schedule manuale/periodica e JSON | [IT](R038_OPERATIONS_IT.md#9-esecuzione-manuale-e-scheduler-rust) · [EN](R038_OPERATIONS_EN.md#8-manual-and-scheduled-backups-and-integrity-checks) | [Maintenance](MAINTENANCE_POLICY.md) | Runner/daemon sperimentali, niente servizio OS certificato |
| Build macOS/Windows/Linux e policy release | [Release channels](RELEASE_CHANNELS.md) | [Rust inventory](RUST_PORT_INVENTORY.md) | Preview non firmata/certificata |
| Criteri di merge/RC e test di accettazione | [RC matrix](RC_CERTIFICATION_MATRIX.md) | [Development status](DEVELOPMENT_STATUS.md), [Handoff](HANDOFF.md) | Blocchi aperti, ultimo head CI da verificare |

## Stato della documentazione PDF

- Generatore: `scripts/build-publications.py`. I manuali IT/EN hanno quattro capitoli aggiunti sulla r038 Rust, ciascuno marcato *development preview*, mantenendo le immagini e procedure storiche della r018.
- **Attenzione:** i PDF tracciati in `docs/manuals` possono ancora riferirsi alla r018 finché la CI non li rigenera e la release non li pubblica. Per la r038 utilizzare **questi runbook Markdown** come riferimento attuale.
- Il job `publications-smoke` rigenera entrambi i PDF e controlla via `pdftotext` la presenza dei comandi `verify-chain`, `gc-collect` e della sezione `Rust r038:`.
- Le brochure descrivono l'offerta storica e NON certificano le funzioni sperimentali r038.

## Evidenze non sostituibili dai documenti

La compilazione del codice, la presenza di un comando, il successo di test fittizi e la generazione di un PDF non dimostrano recuperabilità reale. Servono ancora CI verde sull'ultimo head, restore su provider effettivi e utenti distinti, fault/power-loss, compattazione sicura della parità, protezione WORM, signed/notarized installer e upgrade/rollback. Non dichiarare la RC completa senza tali evidenze.

## Regola di aggiornamento

Ad ogni macroarea Rust chiusa: aggiornare nell'ordine (1) procedure IT/EN, (2) documento specialistico corrispondente, (3) inventory e RC matrix, (4) `DEVELOPMENT_STATUS.md` e `HANDOFF.md`, (5) generatore dei PDF e gate `publications-smoke`. I PDF già pubblicati non devono essere retroattivamente descritti come r038 se non rigenerati e verificati.
