# Manuali utente — stato dei PDF

- **Ultima applicazione distribuita:** Multi Backup r018 Python/Tk. Le schermate e le procedure storiche dei manuali PDF tracciati in questa cartella documentano principalmente tale interfaccia.
- **Sviluppo attivo:** Rust nativo r038 nel branch di PR #10; non è una release stabile o RC certificata.
- **Procedure r038 aggiornate e autorevoli:** [Guida IT](../R038_OPERATIONS_IT.md) e [English Guide](../R038_OPERATIONS_EN.md).
- Il generatore `scripts/build-publications.py` produce manuali bilingui aggiornati con **quattro capitoli r038 marcati sviluppo/preview**, oltre alle sezioni precedenti r018. La CI `publications-smoke` controlla effettivamente la presenza di tali sezioni nei PDF generati.
- Non presumere che i PDF attualmente tracciati nel repository siano stati già sostituiti: per una copia rigenerata usare gli artifact della CI `native-gui-and-about-preview` relativi a un run r038 completato positivamente. Pubblicare/sostituire i PDF ufficiali solo quando la versione e la certificazione dei pacchetti sono allineate.

Per verifiche e limiti si veda [l'indice della documentazione](../DOCUMENTATION_INDEX_R038.md).


## r218 addendum (2026-11-18)

La documentazione per l'attuale console Rust responsive, le app mobili iOS/iPadOS/Android, il mount cloud FUSE/WinFsp e il browser cloud autenticato è disponibile in [matrice piattaforme](../PLATFORMS_R218_IT.md), [client mobili](../MOBILE_CLIENTS_R218_IT.md), [Cloud Drive e restore](../CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md), [iCloud Drive B-In/B-Out](../ICLOUD_B_IN_OUT_R218_IT.md). I PDF storici di questa directory contengono schermate Python/Tk precedenti e NON descrivono queste preview.


## r218 — manuali correnti con screenshot Rust

Il generatore nuovo è scripts/build-current-manual-r218.py. Le screenshot desktop si acquisiscono dal binario Rust egui realmente avviato, con profilo temporaneo isolato e Xvfb. Le screenshot server e mobile riproducono gli asset HTML/CSS/JS attuali con API fixture sintetiche esplicitamente indicate; non provengono da account reali né da dispositivi fisici.

Il workflow GitHub Actions brochures-r218.yml cattura le immagini, genera e verifica i quattro PDF bilingui, li pubblica come artifact e tenta di sostituire i PDF versionati soltanto se tutti i controlli passano. Prima del successo della CI i vecchi PDF rimangono documenti storici, NON la manualistica corrente. Le screenshot r018 Python/Tk in docs/screenshots restano solo materiale storico.
