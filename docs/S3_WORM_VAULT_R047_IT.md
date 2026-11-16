# Multi Backup r047–r050 — vault S3 Ceph RGW, MinIO e RustFS

**Stato: anteprima Rust, NON certificata RC/WORM.** Il trasporto usa
AWS Signature V4 direttamente in Rust. E' distinto dal vault per le chiavi
age e dal deposito append-only locale di r046.

## Requisiti della destinazione

Configurare un bucket DEDICATO con versioning Enabled, Object Lock Enabled,
retention predefinita COMPLIANCE e TLS valido. La creazione del bucket e delle
policy avviene fuori da Multi Backup. Credenziali ingest distinte da admin
e restore; nessun DeleteObject, DeleteObjectVersion o BypassGovernanceRetention.
S3 Object Lock protegge le versioni, NON necessariamente il catalogo:
un delete marker può nascondere una versione non cancellata.

Configurazione JSON senza credenziali:

    {
      "provider": "ceph-rgw",
      "endpoint": "https://vault-rgw.example.org",
      "region": "us-east-1",
      "bucket": "multi-backup-worm",
      "prefix": "roma1/backups",
      "recipient": "age1CHIAVEPUBBLICAVALIDA",
      "retention_days": 30,
      "allow_loopback_http_for_testing": false
    }

Cambiare provider in "minio" per MinIO o "rustfs" per RustFS. HTTP ammesso solo con flag esplicita
e solo verso loopback/localhost per test. No token o password nel file;
le credenziali vengono da AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY e
AWS_SESSION_TOKEN opzionale di un ambiente isolato. Niente credenziali
nei parametri CLI o nei log.

## CLI

    multi-backup-native s3-vault preflight --config /private/s3.json
    multi-backup-native s3-vault backup --config /private/s3.json --source /source
    multi-backup-native s3-vault verify --config /private/s3.json \
      --id HEX_ID --identity /private/key.age
    multi-backup-native s3-vault restore --config /private/s3.json \
      --id HEX_ID --identity /private/key.age --destination /new-restore

Lo staging locale contiene esclusivamente ciphertext age. Ogni oggetto PUT
ha If-None-Match *, Object Lock COMPLIANCE e retain-until. Si verifica il
versionId e si interroga GetObjectRetention sulla versione effettiva.
Il manifest viene scritto LAST, dopo l'archivio cifrato. Per il restore,
la GET usa versionId, controlla digest/dimensione e verifica la decifratura
e l'integrità age. Destinazioni esistenti rifiutate. Nessuna API DELETE,
purge, cancellazione retention o modifica del bucket viene esposta.

Non ci sono ancora multipart/resume remoti né catalogo firmato indipendente.
CLI, GUI Rust e API server ora invocano il trasporto S3; ogni percorso deve
ancora passare test reali indipendenti. HA e agenti S3 necessitano ancora
di un collaudo end-to-end e fencing dedicato. Non definire pronta una RC
finché tutti questi percorsi non sono certificati.

## CI reale

Il job s3-vault-minio-real parte dopo Production Gate e prima dei job
di pubblicazione. Compila ed esegue un server MinIO effettivo da sorgenti upstream e verifica:
- rifiuto di un bucket senza Object Lock COMPLIANCE;
- backup age, verifica, restore byte-identico;
- rifiuto di identity errata e restore sovrascrivente;
- tentativo di DELETE versionato in COMPLIANCE rifiutato DAL SERVER,
  seguito da una nuova verifica di integrità.

Il PASS MinIO non equivale a PASS Ceph. Per Ceph RGW serve un laboratorio
autorizzato separato con versione RGW, bucket usa-e-getta, versionId,
preflight, restore, riduzione retention e delete negativi.
In assenza di prove il risultato è NOT RUN, non PASS.
Se la versione Ceph non supporta il conditional PUT o Object Lock come
richiesto, deve fallire chiuso, senza attenuare i controlli.

## Rischi e chiusure RC in parallelo

- Un root/storage-admin ostile può distruggere i dischi o alterare la
  configurazione dello storage: WORM software richiede nodo/cluster e
  credenziali amministrative indipendenti, audit esterno e drill reali.
- Mancano manifest autenticati indipendentemente, discovery delle versioni
  in caso di delete marker, multipart/resume e garbage collection.
- Rust: test installazione desktop, agent push/pull, lease fencing e HA
  Kubernetes reale, restore sul cluster condiviso e audit di sicurezza.
- GitHub/Dropbox: prove E2E con credenziali di account test isolate nei
  secret del repo privato, non nella CI pubblica.
- Altri plugin: prove distinte POSIX/NFS/SMB/SSHFS/WebDAV/CephFS/
  GlusterFS/Lustre, PostgreSQL/MariaDB/Galera e Oracle se autorizzato.
- GPFS: rimane sperimentale NON bloccante per la prima RC.
- Stable: firma installatori, notarizzazione, upgrade e rollback certificati.

**Mai indicare come PASS un test saltato, un mock o la sola compilazione.**

## Desktop Rust e API server

La pagina Vault dell'app Rust ora include config JSON S3, preflight,
backup, verify e restore. Se la chiave age è stata già sbloccata, il
restore usa l'identità IN RAM; in alternativa richiede il path di una
identity privata. S3 username/password non vengono inseriti nei campi GUI.

Sul server impostare MULTI_BACKUP_S3_VAULT_CONFIG sul path del JSON,
AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY tramite il secret store di
deployment e MULTI_BACKUP_AGE_IDENTITY_FILE per verify/restore. API:
- GET /api/s3-vault/status — sessione admin e vero preflight remoto;
- POST /api/s3-vault/backup — admin, same-origin, CSRF, copia cifrata del
  SOLO store live configurato sul server, nessun path dal browser;
- POST /api/s3-vault/verify — admin, same-origin e CSRF; JSON {"id":"HEX"};
- POST /api/s3-vault/restore — admin, CSRF, same-origin, approvazione
  indipendente di recovery e destinazione NUOVA sotto export root.

Non esiste endpoint delete/purge. In produzione il server deve usare
credenziali ingest e restore con privilegi minimi e separati secondo
l'architettura del deployment. L'uso di un'unica credenziale S3
read+write in un processo non è ancora certificato least-privilege.

## r050 — RustFS S3 WORM: stato preliminare

`provider: "rustfs"` è ora una terza opzione del medesimo trasporto Rust
SigV4. **Non** allenta nessun requisito Object Lock, COMPLIANCE, versioning,
TLS, verifica della retention su ciascuna versione o ripristino age.

La documentazione ufficiale RustFS riporta versioning e Object Lock WORM
COMPLIANCE. Questa è un'indicazione del produttore: la certificazione di
compatibilità Multi Backup richiede il vero collaudo separato e verde con
server `docker.io/rustfs/rustfs:1.0.3` in CI.

Test: `.github/workflows/s3-rustfs-live-r050.yml` e
`scripts/ci/s3-worm-rustfs-live.sh`; non usare un mock come sostituto.
Il test usa storage temporaneo, S3 autenticato, archivio age, restore,
versionId, COMPLIANCE e attacchi negativi alla retention.
Vedere `docs/S3_VAULT_STORAGE_MATRIX_R050_IT.md`.

**Nota:** la certificazione single-node RustFS non attesta capacità
di protezione da root dell'infrastruttura o guasto/disastro multi-nodo.
Ceph RGW e RustFS non ereditano il PASS reale già ottenuto da MinIO.
