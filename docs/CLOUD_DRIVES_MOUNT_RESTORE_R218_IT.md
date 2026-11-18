# Multi Backup r218 — Cloud Drive mount, browsing e restore selettivo

## Architettura comune

I provider Dropbox, Google Drive, iCloud Drive, OneDrive, ownCloud, Nextcloud e Box usano un trasporto cloud comune basato su rclone configurato esternamente, con credenziali conservate unicamente nel file privato amministrato dal server (0600). Le app desktop usano una CLI Rust per il mount FUSE/WinFsp, mentre browser e app iOS/iPadOS/Android condividono le API autenticate del server e la stessa UI responsive. Le funzionalità sono Preview, NON provider READY; il catalogo aggiunge iCloud B-In/B-Out, 16 percorsi READY già qualificati su 34 plugin.

| Provider | rclone type | Mount POSIX / WinFsp | Browser server | Backup B-In/B-Out |
|---|---|---|---|---|
| Dropbox | dropbox | Preview | Preview | B-In e B-Out dedicati già disponibili |
| Google Drive | drive | Preview | Preview | Planned / Planned |
| iCloud Drive | iclouddrive + service=drive | Preview | Preview | Nuovo B-In singolo file e B-Out ciphertext Preview |
| OneDrive | onedrive | Preview | Preview | Planned / Planned |
| ownCloud | webdav + vendor=owncloud | Preview | Preview | Planned / Planned |
| Nextcloud | webdav + vendor=nextcloud | Preview | Preview | Planned / Planned |
| Box | box | Preview | Preview | Mount/browser Preview, backup engine dedicato da integrare |

Il mount è una vista cloud mutabile: NON è una snapshot consistente né riproduce automaticamente ACL, xattr, UID/GID, hardlink, versioni e semantiche di locking del provider. La VFS cache di rclone può creare dati in chiaro nella cache locale; usare disco e cache cifrati. Su macOS serve macFUSE; su Linux FUSE; su Windows WinFsp.

## CLI desktop

~~~sh
multi-backup-native plugins cloud-list --config /private/rclone.conf --provider nextcloud --directory Documenti --json
multi-backup-native plugins cloud-download --config /private/rclone.conf --provider nextcloud --path Documenti/file --destination /private/NEWFILE
multi-backup-native plugins cloud-mount --config /private/rclone.conf --provider nextcloud --mountpoint /mnt/nextcloud --access ro
# SOLO dopo esplicita richiesta di mount RW:
multi-backup-native plugins cloud-mount --config /private/rclone.conf --provider nextcloud --mountpoint /mnt/nextcloud --access rw --confirm-rw nextcloud
~~~

Sono inoltre predisposti cloud-upload (oggetto nuovo con nome casuale, verifica SHA256 di readback) e cloud-delete con SHA256 atteso e conferma testuale esatta; nessun comando di sync distruttivo o delete ricorsivo.

## Server web e mobile: modalità sicura

API: GET /api/cloud/remotes, /api/cloud/list, /api/cloud/download e POST /api/cloud/upload, /api/cloud/delete. Il server riceve solo ID provider registrati e percorsi sanificati. Le credenziali rclone non vengono mai trasmesse al client.

- Default READ ONLY. MULTI_BACKUP_CLOUD_WRITE_ENABLED=1 abilita il percorso upload solo se il gateway è in modalità hardened HTTPS e l'utente è admin, CSRF valido, approvazione offline recupero obbligatoria.
- MULTI_BACKUP_CLOUD_DELETE_ENABLED=1 abilita la cancellazione singolo file (mai cartelle o ricorsioni), sempre con autorizzazione indipendente, hash originale, stringa DELETE provider:path e audit server.
- Per la preview: download massimo 64 MiB, upload massimo 8 MiB, senza upload multipart; richiedere streaming e prova di robustezza per READY. Verifica pre-delete SHA256 e operazione deletefile NON sono atomiche rispetto a writer concorrenti; la possibilità di recupero dipende da versioning/trash del provider.
- Nome locale montato e token non vengono passati dagli input web. La configurazione del server è un file preinstallato amministrativamente, non modificabile dal browser.

## Browsing e restore dai backup cloud

La console server include già browsing di manifest verificati e selezione di file/cartelle per restore nel chunk store age tramite /api/backups e /api/export, con autorizzazione offline. Le nuove API cloud consentono di navigare il drive e scaricare un file di backup age, e il mount desktop consente lettura POSIX. **Non è ancora completata** l'integrazione automatica Cloud Drive → import chunk store → browsing dei file INTERNI all'archivio remoto → restore selettivo remoto senza staging in chiaro; tale percorso resta un gate obbligatorio prima di READY. Non equiparare il browsing dei file remoti al browsing dei membri dell'archivio cifrato.

## Collaudi ancora necessari

Gli unit test di validazione, la compilazione Rust e l'E2E con backend fittizio si distinguono esplicitamente dai test reali su account Dropbox/Google/iCloud/OneDrive/ownCloud/Nextcloud. Per READY: OAuth/2FA, rinnovo token, listing versionato, download grande, upload resume, delete con version guard dove disponibile, mount FUSE su Linux e macOS, WinFsp su Windows, funzionamento mobile, restore selettivo e integrità age, permessi e isolamento multiutente.

Riferimenti tecnici:
- https://rclone.org/commands/rclone_mount/
- https://rclone.org/commands/rclone_lsjson/
- https://rclone.org/iclouddrive/
- https://rclone.org/commands/rclone_deletefile/
- https://rclone.org/webdav/


## r218 — ZIP64 cifrato: ispezione e restore selettivo cloud

Nuovi comandi CLI: plugins cloud-backup-inspect e plugins cloud-backup-restore. Accettano provider approvato, configurazione rclone privata, cartella remota con bundle.index.age e part-NNNNNN.zip, identity age e directory di lavoro locale privata. Il primo comando autentica l'indice cifrato e ogni volume ZIP e restituisce le voci logiche del manifest. Il secondo richiede --confirm RESTORE:PROVIDER:DIR, un percorso di destinazione inesistente ed eventuale --selection PATH. Prima della pubblicazione del restore verifica age, ZIP SHA256 e chunk/parity; byte budget predefinito 8GiB, massimo 32GiB.

Il server espone POST /api/cloud/backup/inspect e POST /api/cloud/backup/restore, disabilitati di default: servono MULTI_BACKUP_CLOUD_RECOVERY_ENABLED=1, HTTPS hardened, amministratore, CSRF e autorizzazione offline. I test usano cifratura vera ma trasporto cloud rclone simulato: non esiste ancora certificazione con account cloud veri, fencing atomico di versioni remote o restore direttamente verso un nuovo cloud provider.
