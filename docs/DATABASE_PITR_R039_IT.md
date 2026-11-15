# Multi Backup r039 — collaudo reale WAL e binlog PITR

## Scopo e classificazione

I test in `.github/workflows/database-pitr-integration.yml` usano veri
server PostgreSQL 16 e MariaDB 11.4 in container Docker separati per sorgente
e destinazione. Il backup e la verifica age transitano realmente nei binari
Rust Multi Backup; i passaggi di produzione/replay dei log rimangono eseguiti
dai client nativi del database, orchestrati dagli script di laboratorio.
**Questo è un collaudo E2E dell'integrazione della pipeline, non una
certificazione della gestione PITR completamente automatica nel plugin.**

## PostgreSQL: base backup fisico + WAL

Lo script `scripts/test-postgresql-wal-pitr.sh` avvia PostgreSQL 16 con
`wal_level=replica`, `archive_mode=on`, `archive_command` e archiviazione
WAL in directory privata. Configura in laboratorio un account di replicazione
SCRAM per `pg_basebackup -Fp -X stream`, acquisisce un backup fisico,
inserisce 20 righe successive, crea un restore point nominato e inserisce una
transazione successiva che **non** deve comparire nel database ripristinato.

Dopo `pg_switch_wal()`, attende il file WAL archiviato e arresta la sorgente.
La CLI Rust crea due archivi age autenticati distinti: base fisica e WAL.
Entrambi sono verificati e ripristinati in directory nuove. PostgreSQL riparte
in un altro container da `recovery.signal` con `restore_command` e
`recovery_target_name`; il test richiede promozione fuori dal recovery,
esattamente 30 righe, somma degli ID 465 e assenza della transazione ID 999.
Verifica chiave age errata e restore su destinazione esistente rifiutati.

**Limite:** `pg_basebackup` crea una directory temporanea **in chiaro** nel
laboratorio prima della cifratura. Non è ancora un esecutore fisico Rust
senza staging. Nessuna promessa di zero plaintext sul disco di produzione;
non abilitare questo flusso come backup unattended finché il plugin fisico
non streamma direttamente su archivio cifrato con verifica del checkpoint.

## MariaDB: backup logico full + ROW binary log

`scripts/test-mariadb-binlog-pitr.sh` usa MariaDB 11.4 con log binario
attivo, `binlog-format=ROW`, `sync-binlog=1` e flush InnoDB a commit.
Esegue `plugins database-backup` e `plugins database-restore-new`
tramite il motore Rust effettivo. In assenza di scritture concorrenti,
`FLUSH BINARY LOGS` identifica il confine successivo al backup consistente.
Il test acquisisce file e posizione iniziale con `SHOW MASTER STATUS`,
inserisce 20 righe, salva posizione finale, aggiunge una transazione ID 999,
ruota il log e arresta il server.

Multi Backup archivia i binlog con age, li verifica e li estrae su spazio
privato. Il replay avviene mediante `mariadb-binlog --start-position
--stop-position` sul server di recovery indipendente, dopo che il plugin
Rust ha ripristinato il FULL. Verifica 30 righe, somma ID 465, assenza
dell'ID 999, e blocco della sovrascrittura sul database ripristinato.

Questo collauda un **solo file binlog in formato ROW e soglia posizionale**.
Restano da integrare nel plugin i job di cattura continua e retention dei log,
il replay senza staging, la continuità fra file binlog multipli, il failover
GTID, la gestione delle transazioni DDL/motori non InnoDB e le versioni
compatibili Oracle MySQL.

## Stato / policy CI

- Primo collaudo MariaDB binlog verificato verde: run 43672237760, job
  MariaDB (sul commit 218852c3), output PASS e restore al record 30.
- PostgreSQL: prima esecuzione ha rifiutato un client di replicazione non
  autorizzato in `pg_hba.conf`; la correzione usa un record SCRAM limitato
  all'IP bridge del runner. Una seconda correzione conserva i permessi privati
  della directory WAL con copia temporanea amministrata.
- **Non marcare PostgreSQL PITR verde** senza specifico log di avvio riuscito
  del nuovo server e verifica del numero esatto delle righe.
- Test reale AFS+Kerberos è separato; non dipende da questi test.

### Esecuzione

```bash
bash scripts/test-postgresql-wal-pitr.sh
bash scripts/test-mariadb-binlog-pitr.sh
```

I due script richiedono Linux con Docker, Rust CLI compilato in
`rust/target/debug/multi-backup-native`, age, jq, client PostgreSQL/MariaDB,
porte TCP 15432/15433 e 13306/13307 libere. Operano soltanto su container
e directory temporanee creati per il test; mai su database di produzione.
Il workflow GitHub contiene due job indipendenti con fail-closed e cleanup.
