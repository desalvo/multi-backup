# Multi Backup — provider readiness r209 (17 novembre 2026)

## Contratto di certificazione
**READY** richiede adattatore Rust eseguibile, vero provider, B-In e B-Out dichiarati realmente compatibili, restore completo su nuova destinazione, riscontro indipendente dei byte e casi negativi di ACL/integrità/credenziali/fault. Una CI verde di *compilazione* o di *preflight* non è prova provider. I nuovi cambiamenti sono limitati alla branch `feature/r209-worm-vault-db-patroni-ovirt` e restano separati da main.

| Plugin / requisito | Funzione reale | CI verificata | Ancora non coperto |
|---|---|---|---|
| Anti-ransomware Vault B-Out | `vault-worm`: S3 Object Lock COMPLIANCE verificato, versioned ciphertext+manifest, manifest-version escrow, test completo di chiave age, create-only restore | RustFS [#44070336929](https://github.com/desalvo/multi-backup/actions/runs/44070336929) PASS | Separazione host amministrativi/credenziali, root compromise, backup unattended con policy isolata, guasti fisici su host |
| Vault locale | `vault-store`: append-only applicativo senza cancellazione API | Test Rust precedenti | **NON** rootproof né WORM indipendente; non certificare come anti-ransomware completo |
| PostgreSQL 16 FULL | `plugins database-backup/restore-new`: pg_dump custom cifrato streaming e create-only pg_restore | [#44072156248](https://github.com/desalvo/multi-backup/actions/runs/44072156248) PASS | Database roles, extensions, interlocking coordinato con snapshot cluster |
| PostgreSQL physical WAL PITR | pg_basebackup + WAL archivio age, recovery at named target | [#44072156266](https://github.com/desalvo/multi-backup/actions/runs/44072156266) PASS | Orchestrazione fisica unificata di Patroni/WAL e point-in-time in server/agents |
| PostgreSQL / Patroni | `Profile.patroni_api` + `patroni_expected_members`: HTTPS /primary leader lock, expected healthy members, stable timeline, guarded backup and new-db restore | [#44072273975](https://github.com/desalvo/multi-backup/actions/runs/44072273975) FAIL; [#44072873240](https://github.com/desalvo/multi-backup/actions/runs/44072873240) in esecuzione | Non contare come PASS finché l'ultimo run completo non è verde. 3 processi CI non sono 3 host fisici |
| MariaDB 11.4 / Galera 4 | Real InnoDB database, logical age B-In, create-only B-Out restore, 3 members Primary/quorum and data recovery | [#44072156212](https://github.com/desalvo/multi-backup/actions/runs/44072156212) PASS | DDL concurrent snapshots, complete Galera restore/fencing |
| MariaDB ROW binlog PITR | Full logical base + encrypted binlog from real MariaDB, position-bounded restore | [#44072156266](https://github.com/desalvo/multi-backup/actions/runs/44072156266) PASS | GTID cross-node replay, automated continuous archiving |
| Oracle MySQL 8.4 | Genuine server, 150 rows/byte-exact binary BLOB restore and no-overwrite | [#44072156604](https://github.com/desalvo/multi-backup/actions/runs/44072156604) PASS | MySQL-native GTID PITR, 8.0/every release certification |
| oVirt VM B-In | New `ovirt-imageio-backup`: an existing external `READY` VM backup and `TRANSFERRING` imageio DOWNLOAD for exact IDs, trusted HTTPS, SHA256/size before age manifest | [#44071743748](https://github.com/desalvo/multi-backup/actions/runs/44071743748) PASS hosted safety + Rust focused [#44071509315](https://github.com/desalvo/multi-backup/actions/runs/44071509315) PASS | **Real oVirt provider job SKIPPED**; no direct remote VM restore, no full VM config/OVF/multi-disk/boot, no native CT; no claim Ready |

## Collaudo oVirt autentico necessario
Il workflow manuale `.github/workflows/ovirt-real-field-r209.yml` usa runner self-hosted `multi-backup-ovirt-field` su ambiente approvato `multi-backup-ovirt-readonly-lab`, solo VM di test isolate. Il runner unprivileged deve avere:
- `/etc/multi-backup-ci/allow-disposable-ovirt` con testo `DEDICATED_OVIRT_VM_BACKUP_FIXTURE_ONLY`.
- Configurazione `/etc/multi-backup-ci/ovirt-field/backup-image.json` proprietario runner 0600 con URL Engine HTTPS, bearer token a scope limitato, `vm_id`, `backup_id`, `disk_id`, `transfer_id`, `transfer_url`, `expected_sha256`, `expected_bytes`. Non mettere token in secrets CI nei comandi o nella documentazione.
- Una VM reale oVirt non produttiva con backup `READY` e transfer IMAGEIO di DOWNLOAD già avviato. La fase/URL e i dischi devono essere corrispondenti e ancora disponibili; non riutilizzare volumi o VMIDs di produzione.
- `age-keygen`, `qemu-img`, `jq`, Rust; il job ricrea il disco in una directory NEW e verifica SHA/size e `qemu-img check`, rifiuta chiavi sbagliate e collisioni. **NON** avvia né modifica VM sull'Engine. Per certificare un VM backup completo serviranno OVF, tutti i dischi, restore su VM nuova, boot e verifica guest.

## Modello di minaccia Vault
Su Linux, un processo locale con root sul server che contiene i file può manomettere dischi/fs locali anche se la GUI rifiuta `rm`. Il Vault anti-ransomware affidabile necessita **dominio di autorità esterno**, root separato e credenziali limitate: S3 Object Lock COMPLIANCE, credenziali di sola scrittura dal server di backup e credenziali indipendenti per leggere/ripristinare. L'identificativo immutabile della versione manifest va custodito anche all'esterno del provider e del server di backup. Non dichiarare tolleranza a compromissione totale di account root/console del server Vault senza test specifici; per cattiva gestione credenziali, anche la disponibilità può essere attaccata pur conservandosi le versioni protette.

## Readiness generale
Il catalogo completo resta di **33 provider, 32 B-In e 26 B-Out**. Altri provider ancora incomplete: Google Drive, OneDrive, HDFS, Alfresco, Kubernetes, VMware VM, Fibre Channel, NVMe-oF, oVirt restore, PVE/PBS field completo e GPFS nativo. È **tecnicamente scorretto** dichiarare la RC completa o gli adapter generici tutti `Ready` senza test reali di backup e ripristino in ogni direzione.

One-line English commit summary: Add externally enforced WORM vault and genuine database/PITR field gates, enforce Patroni leader-lock safety, and introduce read-only attested oVirt IMAGEIO backup preview.
