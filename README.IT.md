# Multi Backup

**Multi Backup `0.1.0-r002-rc1`** è un'applicazione multipiattaforma per backup cifrati, incrementali e riprendibili, basata su motori indipendenti (*plugin*) e distribuita con licenza **EUPL-1.2**.

![Dashboard Multi Backup](docs/screenshots/dashboard.png)

## Caratteristiche

- **GitHub** (attivo per default): mirror dei repository Git e Git LFS, configurazioni, release, issue/PR, workflow, log e artifact CI; pulizia degli artifact **facoltativa**, con simulazione e protezione conservativa degli artifact usati per la distribuzione.
- **Dropbox** (disattivato per default): esportazione incrementale e cifrata delle versioni dei file tramite API di sola lettura, deduplicazione delle versioni già archiviate e manifest cifrati. **Non cancella e non modifica i file Dropbox, e non effettua purge dei backup locali.**
- **Ripresa** dopo interruzione tramite checkpoint e verifiche SHA-256; `--restart` crea un nuovo ciclo senza sovrascrivere i backup validi.
- **Interfaccia grafica** con Dashboard, Backup, Archivi, Plugin, Cifratura, Aggiornamenti e Impostazioni; supporto CLI anche per consultare, estrarre e montare gli archivi.
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

- [Manuale utente IT (PDF)](docs/manuals/Multi-Backup-Manuale-Utente-IT.pdf)
- [User manual EN (PDF)](docs/manuals/Multi-Backup-User-Manual-EN.pdf)
- [Brochure IT (PDF)](docs/brochures/Multi-Backup-Brochure-IT.pdf)
- [Brochure EN (PDF)](docs/brochures/Multi-Backup-Brochure-EN.pdf)
- [Canali di rilascio](docs/RELEASE_CHANNELS.md), [sicurezza](docs/SECURITY.md), [handoff](docs/HANDOFF.md).

**Licenza:** [EUPL-1.2](LICENSE).
