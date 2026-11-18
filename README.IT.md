# Multi Backup

## r218 — Cloud Drive, iCloud B-In/B-Out e applicazioni mobili

**RC0 r217 installabile, CI verde:** [full CI #44246558178](https://github.com/desalvo/multi-backup/actions/runs/44246558178), SHA b085abc. Collaudati i quattro installer nativi, CephFS/RBD e tutti i gate richiesti; bundle CI RC0 non firmato, non ancora stable pubblica.

**Novità r218 (Preview):** app native iOS/iPadOS e Android come frontend remoto HTTPS del server; UI del server responsive. Cloud Drive unificati: Dropbox, Google Drive, iCloud Drive, OneDrive, ownCloud, Nextcloud, Box; mount FUSE/WinFsp desktop (sola lettura default), browsing dal server e download, upload/delete previa autorizzazione esplicita. Plugin iCloud Drive B-In/B-Out su rclone con vero stream age e test trasporto sintetico. Il catalogo backup ora ha **34 plugin**, **16** con un perimetro READY limitato. Le nuove build e i cloud non bloccano la RC r217; certificazioni e firma mobile sono ancora aperte.

Documenti: [matrice piattaforme](docs/PLATFORMS_R218_IT.md), [app mobili](docs/MOBILE_CLIENTS_R218_IT.md), [cloud explorer, mount e restore](docs/CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md), [iCloud](docs/ICLOUD_B_IN_OUT_R218_IT.md), [brochure correnti IT/EN](docs/brochures/README.md).



### r039+ Plugin Rust eseguibili (anteprima non certificata)

Disponibili otto sorgenti generiche verso POSIX cifrato, ora anche CephFS, GlusterFS e Lustre con mount Linux RO, e sei restore verso directory nuova POSIX/NFS/SSHFS/CephFS/GlusterFS/Lustre su mount RW. Il plugin AFS separato realizza dump full/incremental diretti da vos a chunk age/zstd, da host locale o SSH agentless autenticato, e restore della catena verificata su volume AFS NUOVO, con conferma obbligatoria. Documentazione [IT](docs/PLUGIN_EXECUTORS_R039_IT.md) e [EN](docs/PLUGIN_EXECUTORS_R039_EN.md). Senza test su veri server AFS e mount cluster, keytab/localauth, ACL e failover non sono ancora certificati.


## r039 — Agenti Rust, policy host, server Kubernetes

Nuove procedure: [cluster e parallelizzazione](docs/CLUSTER_ORCHESTRATION_R039_IT.md), [agenti AFS](docs/AGENTS_AFS_R039_IT.md), [guida EN](docs/CLUSTER_ORCHESTRATION_R039_EN.md). Il codice gestisce più job, banda e soglie CPU/RAM cooperative, assegnazioni fra repliche solo su storage condiviso e affinità ai filesystem locali. La PR #11 è una anteprima, NON una release candidate certificata.


## Documentazione corrente Rust r038

Leggere il [manuale operativo italiano](docs/OPERATIONS_R038_IT.md), [l'indice documentale](docs/DOCUMENTATION_INDEX_R038.md) e [l'inventario delle funzionalità](docs/FEATURE_INVENTORY_R038.md). I PDF storici r018 con screenshot Python/Tk non attestano la nuova GUI Rust. Le funzionalità della PR #10, incluso il repack automatico delle parità 8+2, non sono ancora una RC certificata.


**Multi Backup `0.1.0-r018-rc1`** è l'ultima versione Python/Tk distribuita; **Rust nativo `0.1.0-r038`** è una anteprima di sviluppo nella PR #10, non ancora una RC certificata. Licenza **EUPL-1.2**. Le guide operative r038 descrivono le funzionalità del codice in sviluppo, non quelle già distribuite.

![Dashboard Multi Backup](docs/screenshots/dashboard.png)

## Caratteristiche

- **GitHub** (attivo per default): mirror dei repository Git e Git LFS, configurazioni, release, issue/PR, workflow, log e artifact CI; pulizia degli artifact **facoltativa**, con simulazione e protezione conservativa degli artifact usati per la distribuzione.
- **Dropbox** (disattivato per default): esportazione incrementale e cifrata delle versioni dei file tramite API di sola lettura, deduplicazione delle versioni già archiviate e manifest cifrati. **Non cancella e non modifica i file Dropbox, e non effettua purge dei backup locali.**
- **Ripresa** dopo interruzione tramite checkpoint e verifiche SHA-256; `--restart` avvia un ciclo nuovo, mentre `--resume` riprende quello incompleto mantenendone il nome e i checkpoint.
- **Interfaccia grafica** fotografica in stile glass blu/viola con Dashboard, Backup, Archivi, Ripristino, Plugin, Cifratura, Aggiornamenti e Impostazioni; finestra nativa About accessibile da menu Help, barra superiore e sidebar. Tasti Start/Resume separati e cancellazione locale protetta da conferma; CLI per consultare, estrarre e montare gli archivi.
- **Avanzamento:** gradiente antialiasing 4x rosso→giallo→verde e indicatore di attività animato solo durante un backup con eventi recenti; tasto hamburger per la sidebar. La libreria Rust è utilizzata nei pacchetti nativi macOS/Windows per il progresso e la politica di recupero CI; backup, ripristino, vault e GUI Rust completa richiedono ancora la migrazione con test E2E.
- **Early release:** commit su `main` contenente `[full-ci]`; **stable release:** push di un tag stabile come `v0.1.0`.
- Aggiornamenti: controllo all'avvio e ogni sei ore (predefinito), scelta del canale `stable` o `early-release`; download verificato con SHA-256. La RC non installa automaticamente eseguibili privi di firma e approvazione.

## Avvio rapido

Sono richiesti **Python 3.11+**, `age` e, per GitHub, `gh`, `git`, `git-lfs`. Prima di usare GitHub: `gh auth login`. I valori dei Secrets GitHub non sono esportabili tramite API.

```bash
python3 -m pip install -r requirements.txt
python3 multi_backup.py --version
python3 multi_backup.py plugins
python3 multi_backup.py keys generate --name primary
python3 multi_backup.py backup --backend github --recipient age1CHIAVEPUBBLICA --progress
python3 multi_backup.py backup --backend github --cleanup-ci-artifacts --cleanup-ci-dry-run --keep-ci-runs 5
python3 multi_backup.py gui
```

Abilitare il motore Dropbox esplicitamente e fornire il token **tramite variabile d'ambiente o portachiavi del sistema operativo**, mai come parametro o file di configurazione non protetto:

```bash
python3 multi_backup.py plugin enable dropbox
export MULTI_BACKUP_DROPBOX_TOKEN='TOKEN_PERSONALE'
python3 multi_backup.py backup --backend dropbox --recipient age1CHIAVEPUBBLICA
```

Per scegliere il canale degli aggiornamenti:

```bash
python3 multi_backup.py settings channel early-release
python3 multi_backup.py updates check
```

Gli archivi sono cifrati con `age`. Per visualizzarli e recuperarli usare `list`, `browse`, `view`, `show`, `extract`, `checkout`, `mount` o la GUI. Il montaggio necessita di spazio temporaneo su disco cifrato; `mount` richiede `bindfs` sotto Linux/macOS o `subst` sotto Windows.

## Distribuzioni native

La CI include job macOS 13+ (Apple Silicon e Intel), Windows 11 x64 e RPM per Rocky/AlmaLinux 8/9/10. La pubblicazione su GitHub Releases è associata ai due canali. Gli indici RPM vengono predisposti su `gh-pages`, ma le definizioni `.repo` restano volutamente disabilitate e richiedono firme GPG prima di un utilizzo production.

**Stato:** prima release candidate, test locali disponibili; le build native devono essere verificate sulle piattaforme di destinazione, e prima di abilitare aggiornamenti non assistiti devono essere configurate firma, notarizzazione e fiducia nei pacchetti.

## Documentazione

- [Indice della documentazione Rust r038 e stato della certificazione](docs/DOCUMENTATION_INDEX_R038.md)
- **[Guida operativa Rust r038 — Italiano](docs/R038_OPERATIONS_IT.md)**: totale/incrementale, dimensioni, catene, verifica e schedule, restore verso utenti diversi, purge e GC confermata.
- **[Rust r038 Operations — English](docs/R038_OPERATIONS_EN.md)**: procedure equivalenti e prerequisiti dei plugin.
- I PDF vengono generati dalla CI con appendice r038 sperimentale; gli screenshot originali si riferiscono alla GUI r018. La migrazione dei vecchi archivi Python è facoltativa, ma il restore dei nuovi backup Rust rimane un gate obbligatorio.

- [Manuale utente IT (PDF)](docs/manuals/Multi-Backup-Manuale-Utente-IT.pdf)
- [User manual EN (PDF)](docs/manuals/Multi-Backup-User-Manual-EN.pdf)
- [Brochure IT (PDF)](docs/brochures/Multi-Backup-Brochure-IT.pdf)
- [Brochure EN (PDF)](docs/brochures/Multi-Backup-Brochure-EN.pdf)
- [Canali di rilascio](docs/RELEASE_CHANNELS.md), [sicurezza](docs/SECURITY.md), [handoff](docs/HANDOFF.md).

**Licenza:** [EUPL-1.2](LICENSE).

### Production Gate unico e installazione macOS

Ogni push avvia un unico **Production Gate**, con test e audit di sicurezza; solo dopo il suo successo si avviano compilazione, verifica e pubblicazione. `[full-ci]` su `main` richiede una early-release; un normale tag di versione richiede una stable (ancora bloccata fino al completamento delle firme). La full CI si può avviare manualmente da **Actions → Multi Backup - Production CI → Run workflow**.

Il DMG corretto contiene un solo elemento visibile, **`Multi Backup.app`**, da trascinare nella cartella **Applicazioni**. Al suo interno si trovano la CLI e gli strumenti di cifratura; macOS deve disporre anche di `git`. Il DMG viene pubblicato **solo dopo** firma Developer ID Application, notarizzazione Apple sia dell'app sia del DMG, verifica Gatekeeper e test dell'app copiata fuori dal volume DMG. I precedenti DMG RC non firmati potrebbero essere rifiutati da Gatekeeper: **non disabilitare Gatekeeper**. Le credenziali necessarie vanno impostate come **GitHub Actions secrets** secondo [la documentazione dei canali](docs/RELEASE_CHANNELS.md).

## Funzionalità introdotte nella r013

Al primo backup si chiede esplicitamente se generare una chiave age e si richiede una password robusta (minimo 12 caratteri). La chiave privata è cifrata con AES-256-GCM e scrypt nel file `default.agevault`, conservato fuori dall'applicazione; gli aggiornamenti di Multi Backup non lo sostituiscono. All'avvio successivo, prima di aprire le pagine, serve sbloccare la chiave con la password. Lo sblocco resta valido fino alla chiusura dell'app. Le vecchie chiavi `default.txt` vengono migrate mantenendo la stessa chiave originale per ripristinare i backup precedenti.

Dalla pagina **Encryption** si può creare una **copia di recupero**, confermando la password della chiave corrente, scegliendo una nuova destinazione e impostando una seconda password per il file cifrato. La copia non sovrascrive file esistenti. Si può importare una copia soltanto quando non esiste già un vault locale. Su Mac con Touch ID disponibile, dopo uno sblocco con password si può attivare l'autenticazione biometrica nel Portachiavi locale; macOS gestisce il riconoscimento e la password resta sempre alternativa.

La nuova pagina **Restore** consente il recupero tramite lo stesso plugin con cui è stato creato l'archivio. La destinazione locale deve essere nuova. Il ripristino remoto GitHub richiede l'autenticazione e crea un nuovo repository; quello Dropbox chiede un token temporaneo con permessi di scrittura e carica un nuovo file, anche tramite sessioni per file grandi. I dati GitHub di configurazione, le CI e i metadata possono essere estratti localmente; la loro ricostruzione remota completa non è certificata.

Tutte le pagine sono scorrevoli; la sidebar include Restore e rimane collassabile. È possibile navigare con Ctrl+1…Ctrl+8. L'eventuale cleanup delle CI viene confermato esplicitamente a ogni backup. Gli aggiornamenti in early-release verificano i certificati TLS usando il bundle di CA incluso nell'app senza disabilitare la verifica SSL.

### Anteprime Rust native — test installazione

La CI della PR r020 genera pacchetti **senza Python** separati dalla r018: DMG a singola app macOS ARM64/Intel con firma ad hoc, installer Inno Setup e ZIP Windows x64, archivio eseguibile Ubuntu 24.04 x64. Sono anteprime non notarizzate Apple e non firmate Windows, senza ancora gli RPM Rust Rocky/Alma. La r018 rimane disponibile e invariata.

La GUI Rust r020 conserva la chiave age decifrata soltanto in RAM dopo lo sblocco del vault e permette ripristini locali/remoti senza un file di chiave privata in chiaro. Per macOS è predisposto il supporto Touch ID nativo (da validare su hardware reale); la CI include anche sei RPM Rust per Rocky/AlmaLinux 8/9/10. I pacchetti rimangono anteprime non stabili e non firmate per la produzione. Consultare [l'inventario della migrazione](docs/RUST_PORT_INVENTORY.md).

La GUI Rust verifica il vault nella directory delle impostazioni già usata dalla r018 e, se presente, ne impone lo sblocco all'avvio prima di mostrare i comandi. È disponibile il controllo degli aggiornamenti stable/early con download e SHA-256 verificato, mai installato automaticamente. La r020 resta un'anteprima finché non saranno certificate tutte le prove funzionali e le firme di distribuzione.


## Debian: pacchetti nativi Rust

La CI ora prevede quattro pacchetti .deb: Debian 12 Bookworm (LTS) e
Debian 13 Trixie, entrambi per amd64 e arm64. Ogni pacchetto viene compilato
sull'architettura/distribuzione di destinazione, verificato e installato
in una fase della CI. Rimane una preview finché non saranno validati
firma, aggiornamenti, sicurezza e restore reale.

## Documento architetturale riservato

Consulta docs/PRIVATE_ARCHITECTURE_WORKFLOW.md. Il diagramma operativo,
i dati di rete e le credenziali non vanno mai pubblicati nel repository.
Il PDF viene generato/cifrato localmente con AES-256 e password scelta
dall'operatore; GitHub conserva al massimo il PDF cifrato, dopo un controllo
di crittografia. L'originale e il generatore privato restano offline.

**AFS SSH agentless restore (anteprima):** la destinazione AFS può essere un host SSH remoto autenticato con chiave pinned e token AFS già autorizzato, mantenendo il controllo del nuovo volume e -overwrite abort. Vedi [manuale plugin](docs/PLUGIN_EXECUTORS_R039_IT.md). Richiesti test reali prima della RC.
