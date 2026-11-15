# Manuali utente — stato dei PDF

- **Ultima applicazione distribuita:** Multi Backup r018 Python/Tk. Le schermate e le procedure storiche dei manuali PDF tracciati in questa cartella documentano principalmente tale interfaccia.
- **Sviluppo attivo:** Rust nativo r038 nel branch di PR #10; non è una release stabile o RC certificata.
- **Procedure r038 aggiornate e autorevoli:** [Guida IT](../R038_OPERATIONS_IT.md) e [English Guide](../R038_OPERATIONS_EN.md).
- Il generatore `scripts/build-publications.py` produce manuali bilingui aggiornati con **quattro capitoli r038 marcati sviluppo/preview**, oltre alle sezioni precedenti r018. La CI `publications-smoke` controlla effettivamente la presenza di tali sezioni nei PDF generati.
- Non presumere che i PDF attualmente tracciati nel repository siano stati già sostituiti: per una copia rigenerata usare gli artifact della CI `native-gui-and-about-preview` relativi a un run r038 completato positivamente. Pubblicare/sostituire i PDF ufficiali solo quando la versione e la certificazione dei pacchetti sono allineate.

Per verifiche e limiti si veda [l'indice della documentazione](../DOCUMENTATION_INDEX_R038.md).
