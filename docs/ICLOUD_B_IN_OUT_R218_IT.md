# Multi Backup r218 — plugin iCloud Drive B-In / B-Out

iCloud Drive è un provider *separato* dalle app mobili iOS/iPadOS. Il nuovo plugin Rust **icloud** nel catalogo raggiunge **34 provider**; 16 hanno almeno un singolo scope READY preesistente, iCloud **Preview** in entrambe le direzioni.

## Prerequisiti

Installare rclone e age sul server/host desktop. Configurare tramite rclone config una sezione [icloud] con type=iclouddrive, service=drive, utilizzando autenticazione Apple ID e 2FA ufficialmente supportata da rclone. Il file rclone.conf deve essere regolare e permessi 0600. Multi Backup non riceve password Apple né token; rclone gestisce la sessione, il rinnovo dei trust token e i limiti del servizio. iCloud Photos e iCloud Backup di dispositivi non fanno parte di questa integrazione.

### B-In: singolo file remoto con SHA256 indipendente

~~~sh
multi-backup-native plugins icloud-import \
 --config /private/rclone.conf --remote icloud:Progetti/file.data \
 --recipient age1CHIAVE_PUBBLICA --expected-sha256 SHA256_ATTESO \
 --destination /private/archivi/NUOVO_FILE.age --json
~~~

Il file esatto viene letto con rclone cat, verificato con digest indipendente e cifrato in streaming age; il file locale di uscita è creato senza overwrite tramite hard link create-only. Nessuno staging locale del file remoto in chiaro. Non è un backup ricorsivo con revisioni/ACL.

### B-Out: archivio cifrato, nuovo nome casuale, verifica remota

~~~sh
multi-backup-native plugins icloud-upload \
 --config /private/rclone.conf --remote-parent icloud:MultiBackup \
 --archive /private/archivi/backup.age --identity /private/keys/identity.txt \
 --expected-sha256 SHA256_ARCHIVIO_CIFRATO --json
~~~

L'archivio viene prima verificato tramite SHA256 e decifrabilità age con l'identità esplicitamente fornita. La copia remota usa un nome random mb-<128bit>.age, senza cancellazione, sincronizzazione o sovrascrittura intenzionale, seguita da lettura completa e digest dei byte remoti. Senza API cloud create-if-none-match non esiste garanzia assoluta anti-race, né versioning Apple immutabile. Scegliere S3 Object Lock COMPLIANCE per backup realmente WORM.

## Test e limiti

Workflow iCloud indipendente: compila Rust, esegue unit test di sicurezza e roundtrip age reale con trasporto rclone sintetico. NON certifica account Apple reale, rinnovo 2FA, grandi file, concorrenza, restore completo e retention. La CI iCloud non può bloccare il gate della RC r217. Per browsing, mount desktop e server, vedere CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md.

Fonti: https://rclone.org/iclouddrive/ e https://rclone.org/commands/rclone_mount/.
