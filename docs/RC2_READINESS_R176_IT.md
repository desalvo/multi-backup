# Multi Backup r176 — stato di completamento verificabile (16 novembre 2026)

## Obiettivo

La RC1 già pubblicata e immutabile è
[rc-0.1.0-rc.1](https://github.com/desalvo/multi-backup/releases/tag/rc-0.1.0-rc.1),
sorgente `1f1a726a3ffed7b46a7498b99891bfb935570865`, 31 asset e full Production CI
[#43932195582](https://github.com/desalvo/multi-backup/actions/runs/43932195582).
**Non alterare, sostituire o riusare l'etichetta RC1 per codice r176**.
r176 è una revisione successiva che necessita test sul suo **nuovo SHA** e
potenziale RC2 quando pienamente qualificata.

## GUI Rust macOS/Windows

- Aggiunti i test unitari nativi del frontend `rust-desktop` ai packaging
  script per macOS e Windows; non solo build e `--self-test`.
- Nuovo workflow `r176-native-gui-field-smoke.yml`: verifica gli **SHA256
  esatti della RC1 già pubblicata**, monta un vero DMG e copia la .app su
  macOS arm64/x86_64, apre la finestra nativa con `--smoke` e attesta le
  10 pagine + About. Su Windows 2025 estrae il pacchetto ZIP **autentico**
  e avvia la GUI nativa con timeout e codice di uscita.
- Il test può essere limitato dai runner host senza sessione grafica; in tal
  caso NON equivale a successo E2E. Questi controlli provano RC1, non un
  nuovo binario r176 che richiederà rigenerazione completa.
- Integrazione del canale `rc` nella pagina Aggiornamenti e nelle
  preferenze native, mantenendo `stable` come default.

## Updater Rust e rollback

- `updater.rs`: selezione esatta dei pacchetti per macOS arm64/x86_64,
  Windows x64, Debian 12/13 amd64/arm64, Rocky/Alma EL8/9/10 e
  Ubuntu24 x64. Rifiuta il cross-distro/arch fallback, URL non GitHub e
  checksum/size scorretti.
- Cerca le vere `rc-0.1.0-rc.N` pubblicate, solo prerelease non draft.
- `activate_portable_version` e `rollback_portable_version`: cambio
  atomico tra versioni **portabili, user-owned e non in esecuzione**, con
  rollback di health-check negativo e vecchia versione conservata. CLI
  `updates activate-portable`/`rollback-portable` richiede percorsi
  espliciti, hash SHA-256 della versione verificata e non esegue codice
  remoto automaticamente. Test di rollback e rifiuto symlink/sovrascritture.
- **Non ancora completato:** auto-update dell'installazione gestita da
  Installer Windows, macOS .app in /Applications, Linux RPM/DEB con privilegi,
  riavvio, rollback post-crash e rollback in produzione. Non dichiarare
  `CI PASS` per il meccanismo auto-update *end-to-end*.

## Orchestrazione distribuita

- Il worker Rust `server_local` non può più sottrarre dalla coda
  PostgreSQL un job `ssh_agentless` o `managed_agent_pull/push` che non
  è in grado di eseguire: resta in coda al dispatcher autorizzato, invece
  di consumare tentativi e registrare falsi errori.
- Test PostgreSQL reale in `live_pg_acceptance.rs`: il job remoto
  ad alta priorità resta `queued` e con 0 tentativi, mentre il job locale
  viene correttamente assegnato e completato.
- Lease PostgreSQL, capacità per replica, affinità storage, quarantine e
  recovery manuale rimangono come in r173.
- **Ancora da completare:** effettivo dispatcher SSH/managed agent per la
  coda PG, writer fencing S3 distribuito end-to-end, limiti di banda globali,
  failover campo su macchine fisiche.

## Vault

- S3 COMPLIANCE Object Lock: dopo PUT del manifest e verifica del lock,
  il processo non ritorna successo finché una GET **della versione immutabile
  esatta** e due verifiche di retention non confermano digest, dimensione,
  conteggio file e retention. Nessuna conferma basata su un listing mutabile.
- Test reali MinIO già esistenti devono essere rilanciati su nuovo SHA,
  senza scrivere mock o duplicare la suite.
- Il vault locale append-only è esplicitamente `external_worm_verified:false`;
  non dichiarare protezione contro un root che controlli anche il backend.
  Per protezione forte usare una authority S3 indipendente e retention COMPLIANCE.

## Prossima qualificazione RC2

1. Verificare CI Rust core, server e GUI; test PG16 reale; test MinIO reale,
   runtime macOS/Windows e packaging su SHA r176. I test RC1 già verdi
   **non** certificano il codice modificato r176.
2. Solo quando tutti verdi, creare una RC2 immutabile con assets firmati
   ad-hoc se previsto. Notarizzazione non critica per RC, obbligatoria per
   distribuzione Stable.
3. Non eseguire merge su main né pubblicare una RC2 non qualificata.

**One-line English commit summary:** Add RC-aware cross-platform Rust update targeting and portable rollback, harden remote queue admission and WORM receipts, and test native GUI runtimes.
