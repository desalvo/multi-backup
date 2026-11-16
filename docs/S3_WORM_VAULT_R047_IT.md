# Multi Backup r047 — vault S3 Ceph RGW e MinIO

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

Cambiare provider in "minio" per MinIO. HTTP ammesso solo con flag esplicita
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
Al momento backup, verify e restore sono via CLI; GUI/server/HA e agenti
necessitano ancora di integrazione dedicata. Non definire pronta una RC
finché tutti questi percorsi non sono certificati.

## CI reale

Il job s3-vault-minio-real parte dopo Production Gate e prima dei job
di pubblicazione. Esegue un container MinIO effettivo e verifica:
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
