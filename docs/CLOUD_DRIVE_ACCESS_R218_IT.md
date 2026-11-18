# Multi Backup r218 — cloud storage montati, accesso POSIX e file browser server

## Requisito architetturale (permanente)
Tutti i plugin di cloud storage devono presentare distintamente le capacità:
- B-In / B-Out nativi con versioning dove il provider lo permette.
- Montaggio desktop: Linux/macOS FUSE/rclone; Windows WinFsp; iCloud Drive locale su Mac tramite File Provider, non credenziali Apple ID esposte a server o client terzi.
- Browsing e gestione da server: visualizzazione gerarchia, download, upload, cancellazione autorizzata, ricerca e selezione, con scoping obbligatorio al provider/account/prefix.
- Browsing/restore dei backup: manifest age verificati, selezione file/cartelle, download/ripristino su nuovo path POSIX (oppure in un mount verificato), protezione retention/hold e audit.
- Capability e permessi individuali per utente, cloud, sorgente, destinazione, e operazione. Disabilitare anziché simulare operazioni che il provider non supporta.

## Implementato nel branch r218 (Preview, non READY globale)

Il server Rust include /api/cloud/profiles, /api/cloud/list, /api/cloud/download, /api/cloud/upload e /api/cloud/delete. La dashboard mostra una sezione Cloud Drive con elenco account autorizzati, navigazione per cartelle, upload e cancellazione protetta. La sessione admin è obbligatoria; le operazioni di scrittura richiedono Same-Origin, CSRF e, in hardened mode, approvazione offline. Non sono mai esposti API rclone RC/config, token provider o filesystem host arbitrari.

Configurazione: MULTI_BACKUP_CLOUD_PROFILES_FILE punta a un file JSON privato (chmod 0600 su Unix), contenente rclone_config assoluto e profiles. La configurazione rclone è separata e chmod 0600. Per ogni profilo: id, provider, remote (NOME già configurato, non URL arbitrario), prefix limitato, allow_upload e allow_delete (false per default). iCloud lato server rclone obbligatoriamente read-only. Il server utilizza MULTI_BACKUP_RCLONE_BIN con eseguibile assoluto: rclone è un processo figlio, senza shell o amministrazione RC.

Limitazioni attuali: upload e download via browser <=8 MiB per richiesta, create-only non ancora atomico sotto concorrenza su cloud remoto, upload/delete non abilitati in assenza di policy, cancellazione di archivi .age e .age.sha256 vietata tramite file manager (retention separata). Audit dettagliato di ogni operazione cloud, resumable upload, grandi file e ACL per utente rimangono da completare prima di READY.

Il mount desktop è supervisionato: scripts/cloud/cloud_mount.py riceve un registro privato e un profilo allowlisted, avvia rclone mount in foreground e usa VFS privato, senza --allow-other né API rclone RC. Il mount è RO se le scritture non sono autorizzate; per iCloud remoto è sempre RO. La GUI può supervisionare il processo e riutilizzare il percorso montato come un filesystem. Il mount non garantisce di per sé gli equivalenti POSIX di locking, ACL, inode, rename atomico o snapshot consistency.

Il backup browser esistente nella dashboard Rust conserva la propria verifica age e selezione logica. I backup su storage cloud possono essere acquisiti prima in cache cifrata locale, poi verificati tramite lo storage/manifest originale e infine ripristinati su path nuovo. Non si deve usare un FUSE mount live come se garantisse coerenza transazionale dei chunk senza snapshot, blocco scritture e verifica.

## Provider: matrice capacità

| Provider | Mount locale | Gestione server | Limite particolare |
| --- | --- | --- | --- |
| Dropbox | rclone FUSE/WinFsp | List/download; upload/delete con scope e policy | OAuth, rate limits, revisioni |
| Google Drive | rclone FUSE/WinFsp | Idem | Google Docs richiedono export, non sono byte-file ordinari |
| OneDrive / SharePoint | rclone FUSE/WinFsp | Idem | Tenant, eTag, token scopes |
| ownCloud / Nextcloud | rclone WebDAV + FUSE/WinFsp | Idem | WebDAV ACL/ETag, server-side lock |
| iCloud Drive | remoto rclone RO; locale Mac File Provider | remota RO; scrittura solo helper Mac autorizzato | rclone iCloud non implementa upload/delete |
| S3 / Ceph RGW / RustFS | rclone FUSE/WinFsp | Idem | Object-store != POSIX; WORM e retention separati |

Riferimenti pubblici: https://rclone.org/commands/rclone_mount/ ; https://rclone.org/iclouddrive/ ; https://rclone.org/rc/

## Esempio registro

File privato /private/cloud_profiles.json:

    {
      "rclone_config": "/private/rclone.conf",
      "profiles": [
        {"id":"dropbox","provider":"dropbox","remote":"db","prefix":"Backups","allow_upload":false,"allow_delete":false},
        {"id":"nextcloud","provider":"nextcloud","remote":"nc","prefix":"Team","allow_upload":true,"allow_delete":false},
        {"id":"icloud-ro","provider":"icloud","remote":"apple","prefix":"","allow_upload":false,"allow_delete":false}
      ]
    }

Configurare i remoti rclone con OAuth su host affidabili, fuori da script e Git. Avvio mount:

    python3 scripts/cloud/cloud_mount.py --registry /private/cloud_profiles.json --profile nextcloud --mount-point /mnt/nextcloud --rclone /usr/bin/rclone

Richiesti FUSE/macFUSE o WinFsp, mountpoint preesistente vuoto, e sessione/permessi locali. La password di iCloud non viene inserita in Multi Backup.

## Demo reale del server, senza account cloud esterni

Dal checkout del branch feature/r218-icloud-bidir-server-demo, con Docker e Docker Compose:

    docker compose -f deploy/cloud/compose.demo.yml up -d --build

Aprire solo http://127.0.0.1:8080 e autenticarsi con le credenziali DI SOLA DEMO:
admin / Demo-Backup-2026!

Il server è realmente compilato in Rust e gestisce sessioni, browsing, download, upload e delete su rclone. Il cloud demo è un alias temporaneo locale isolato sotto /data/demo-cloud, non Dropbox o iCloud reale. All'avvio viene creato un vero backup cifrato age per il browsing dei backup. Server esposto SOLO su loopback, nessun token personale, nessuna autenticazione esterna. NON pubblicare l'immagine su Internet o riutilizzare la password demo in produzione.

Smontare e cancellare i dati simulati:

    docker compose -f deploy/cloud/compose.demo.yml down -v

Il workflow cloud-bridge-r218.yml collauda i profili/mount con test negativi, la compilazione Swift su macOS, il login HTTP reale e il CRUD via server su un rclone alias, non la compatibilità con account cloud commerciali.

## Blocchi per READY completo
Provisioning/supervisione mount dalla GUI multi-OS; streaming upload/download >8MiB; autentico collaudo OAuth/provider e File Provider iCloud su Apple ID isolato; recupero browsing da chunk store remoto con caching coerente; policy fine-grained per utenti/ruoli; lock per oggetti; upload create-only atomico, idempotenza e retry; audit persistente dei CRUD; sincronizzazione/vettore versione cloud; retention/hold remoti; negative test offline/token scaduto e rollback; collaudo su Windows WinFsp e macOS Finder oltre al Linux FUSE.

## Selective restore from a mounted cloud age chunk store (new r218 tool)

The Rust backup browsing/selection requirement is supported by scripts/cloud/mounted_backup_restore.py. It checks that the source is a real mounted cloud filesystem, rejects symlinks/special files/unsafe paths, requires an explicit source quiescence assertion, and copies encrypted store objects to a private local cache with double-read SHA checks. Rust authenticates the age manifest and chunk-chain before listing or writing selected content to a NEW destination. The cache is temporary and deleted even when the operation fails. This is a Preview; not proof of multi-object atomic snapshot support.

Read-only list of logical archives (on a trusted actual FUSE mount, with the original age identity stored locally):

    python3 scripts/cloud/mounted_backup_restore.py \
      --mount-point /mnt/nextcloud --cloud-store-subdir MultiBackup \
      --source-quiesced YES_SOURCE_QUIESCED \
      --cache-parent /private/cache --age-identity /private/keys/identity.txt \
      --native /usr/local/bin/multi-backup-native --mode list

Restore one selected file to a completely new local directory:

    python3 scripts/cloud/mounted_backup_restore.py \
      --mount-point /mnt/nextcloud --cloud-store-subdir MultiBackup \
      --source-quiesced YES_SOURCE_QUIESCED \
      --cache-parent /private/cache --age-identity /private/keys/identity.txt \
      --native /usr/local/bin/multi-backup-native --mode restore \
      --id FULL_64_HEX_BACKUP_ID --selection example/readme.txt \
      --destination /private/restore-new

On a signed-in Mac, --file-provider-macos permits the system-managed local iCloud Drive directory instead of a distinct FUSE mount, but only within that exact File Provider root.

Current RC distinction: the old server UI already browses/restores local age-backed logical archives; this new tool stages remote cloud encrypted stores and calls the same Rust CLI. Automatic background hydration, GUI drag-and-drop and streaming >8MiB cloud HTTP uploads still require further integration.
