# Collaudo Lustre e Oracle — r041 (15 novembre 2026)

**Stato:** procedure e verifiche di sicurezza predisposte; **nessuna certificazione reale Lustre o Oracle ancora conseguita**. Questa modifica è impilata sulla PR #12 (`feature/r040-real-kubernetes-ci`). Nessun rilascio stable è autorizzato.

## 1. Test automatici eseguibili senza sistemi reali

Il workflow `.github/workflows/lustre-oracle-guards.yml` compila il programma Rust e lancia `scripts/test-lustre-oracle-guards.sh` su una VM GitHub standard. Verifica che directory locali non possano fingere di essere mount Lustre, né durante il backup né durante il ripristino; verifica che Oracle non venga eseguito tramite il plugin SQL previsto solo per PostgreSQL/MariaDB, e che i workflow manuali rifiutino un laboratorio non autorizzato.

**Questi test non simulano un vero cluster Lustre né un database Oracle e non devono essere presentati come test funzionali reali.**

## 2. Collaudo Lustre su sistema reale

Il test `scripts/test-real-lustre-roundtrip.sh` richiede:

- un cluster Lustre di laboratorio funzionante (MGS/MDS/OSS e almeno gli OST necessari per la configurazione scelta), client Lustre autorizzato e comandi `lfs`, `getfacl`, `getfattr`, `age-keygen`, `jq`;
- **due snapshot coerenti MDT+OST**, già preparati e montati su **due mountpoint Lustre esatti in sola lettura**, uno per lo stato completo e uno per quello incrementale. Non basta un bind mount o una copia di directory;
- un **terzo mountpoint Lustre esatto in lettura/scrittura**, esclusivamente di collaudo, per creare nuove directory di restore senza sovrascritture;
- una fixture deliberatamente artificiale, in entrambi gli snapshot, con file `.multi-backup-ci-fixture` di contenuto `DEDICATED_LUSTRE_TEST_DATA_V1`. Lo snapshot iniziale deve contenere `README.txt`, `payload/removed.bin`, `payload/stripe.bin`; nel secondo il README cambia, `removed.bin` scompare e compare `payload/new.bin`;
- su `payload/stripe.bin` predisporre una modalità POSIX non banale, UID/GID noti, mtime controllato, ACL, attributo esteso `user.multi_backup_ci` e configurazione di striping Lustre significativa. L'implementazione corrente può fallire proprio sul ripristino di questi metadati: **non attenuare i controlli per ottenere una CI verde**.

Il test usa il CLI Rust reale per due backup cifrati con age e compressi con zstd, verifica il backup e la catena, esegue due restore su nuove directory del mount RW, confronta i contenuti e verifica rimozioni, metadati e stripe count. Le nuove directory ripristinate sono lasciate sul mount di laboratorio per la verifica; non vengono eliminate automaticamente.

Per usare il workflow manuale `.github/workflows/lustre-oracle-field.yml`, registrare un runner dedicato `lustre-ci` e predisporre:

- `/etc/multi-backup-ci/allow-disposable-lustre` con contenuto `DEDICATED_ISOLATED_LUSTRE_FIELD_LAB`;
- environment GitHub `lustre-field-lab` protetto con approvazione e variabili `LUSTRE_FULL_MOUNT`, `LUSTRE_INCREMENTAL_MOUNT`, `LUSTRE_RESTORE_MOUNT`, `LUSTRE_SNAPSHOT_ATTESTED=COORDINATED_MDT_OST_LAB_SNAPSHOTS`;
- un commit SHA esatto di 40 caratteri previamente esaminato.

Non eseguire un runner self-hosted privilegiato su PR non approvate.

## 3. Collaudo Oracle su sistema reale

Oracle rimane un **plugin pianificato**, non un adapter di backup/ripristino completo. La procedura `scripts/test-oracle-rman-lab.sh` esegue soltanto questi comandi su database di laboratorio isolato:

```text
BACKUP VALIDATE CHECK LOGICAL DATABASE;
RESTORE DATABASE VALIDATE;
```

Il primo legge i file di origine e controlla la possibilità di backup senza creare nuovi backup; il secondo legge backup RMAN **già esistenti** per verificarne la possibilità di ripristino, ma non ripristina alcun file. È necessario quindi un database Oracle autorizzato/licenziato, montato o aperto, con backup RMAN esistenti e integri. Occorrono `ORACLE_HOME`, `ORACLE_SID`, `MB_ORACLE_EXPECTED_DB_NAME`, autenticazione OS locale per `sqlplus` e `rman`, accesso tramite account non-root, e marker root-owned `/etc/multi-backup-ci/allow-disposable-oracle` con contenuto `DEDICATED_LICENSED_ORACLE_LAB_ONLY`.

Nel workflow manuale usare un runner isolato `oracle-ci` e un environment protetto `oracle-field-lab`, con le suddette variabili. Lo script rifiuta database con nome diverso da quello previsto, ruoli non PRIMARY o stato diverso da READ WRITE. Non vengono distribuiti binari proprietari né inserite password nel repository.

**La validazione RMAN non certifica il backup Oracle di Multi Backup.** Mancano ancora l'adapter Rust RMAN, acquisizione e cifratura sicura dei backup piece, control file e SPFILE, archived redo log, recupero in una nuova istanza, incrementali e test di errore/PITR.

## 4. Avvio manuale

Il workflow `lustre-oracle-field.yml` è solo `workflow_dispatch`, mai `pull_request` o `push`. Serve il proprietario `desalvo`, l'SHA esatto dei sorgenti verificati, la conferma `APPROVED-DISPOSABLE-FIELD-LAB`, e l'approvazione del relativo environment. Il workflow manuale dev'essere presente sul branch di default per essere disponibile dall'interfaccia Actions: finché resta solo nella PR, l'esecuzione automatica dei test su hardware reale rimane **in attesa**.

## 5. Situazione per la prima RC

- **Lustre:** backup/restore dati nel codice Rust, ma collaudo reale e fedeltà dei metadati non certificati.
- **Oracle:** soltanto catalogo/piano e procedure RMAN di preflight; adapter backup/restore ancora non implementato.
- **Kubernetes:** la CI r040 deve dimostrare due pod funzionanti, PostgreSQL TLS verificato, riavvio/failover e security gate.
- Restano necessari collaudi AFS, agenti, scritture distribuite e fencing, firma/upgrade/rollback e audit di sicurezza. Nessun tag stable.

Per dettagli e corrispondente documento inglese: [LUSTRE_ORACLE_FIELD_ACCEPTANCE_R041.md](LUSTRE_ORACLE_FIELD_ACCEPTANCE_R041.md).

## Esiti effettivi dei laboratori reali (15 novembre 2026)

- **Commit verificato**: `eb3dbf2f964686452fbacc865fd1e9e3be1fcd92` per entrambi i laboratori.
- **Lustre PASS**: [CI 43761650658](https://github.com/desalvo/multi-backup/actions/runs/43761650658) (e precedente [43760867316](https://github.com/desalvo/multi-backup/actions/runs/43760867316)). Rocky Linux 9/QEMU con kernel Lustre autentico, un MDT, tre OST, client reale, striping su due OST, verifica checksum e secondo mount RO. È un collaudo **infrastrutturale**, NON un restore cifrato Rust di snapshot MDT/OST consistenti con metadati completi.
- **Oracle Database Free PASS**: [CI 43761650595](https://github.com/desalvo/multi-backup/actions/runs/43761650595). Backup RMAN autentico di `FREEPDB1` in ARCHIVELOG su database disposable, acquisizione dei backup piece via plugin generico POSIX Rust, cifratura age/zstd, verifica e restore byte-per-byte, poi `RESTORE PLUGGABLE DATABASE FREEPDB1 VALIDATE` sui file recuperati. Non è un restore fisico del DB e non implementa un adapter Oracle nativo.
- **Guard di sicurezza PASS**: [CI 43761650585](https://github.com/desalvo/multi-backup/actions/runs/43761650585). La [full CI 43761650615](https://github.com/desalvo/multi-backup/actions/runs/43761650615) va controllata indipendentemente a conclusione.
- **Restano bloccanti per la RC**: snapshot Lustre coerenti MDT+OST, restore Rust full/incremental con conservazione di ACL/xattr/UID/GID/mtime/stripe; adapter RMAN Oracle e restore reale su un nuovo database con archivelog/PITR; ulteriori collaudi di sicurezza/HA e installazione. PR Draft; nessun tag stable.
