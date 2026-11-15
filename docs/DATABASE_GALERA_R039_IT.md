# Multi Backup r039 — collaudo MariaDB 11.4 Galera 4

## Esito verificato

**PASS, GitHub Actions run 43673606744, commit 37e5de3943b76f453fe5448d251f64db7d80afa5.**

La CI `.github/workflows/database-galera-integration.yml` esegue
`scripts/test-mariadb-galera-pitr.sh` su tre istanze realmente
Galera 4, realizzate con MariaDB 11.4 e provider `libgalera_smm.so`,
in una rete Docker isolata. Il provider e `rsync` SST vengono installati
dall'immagine di test `ci/galera/Dockerfile`.

## Scenario end-to-end

1. Avvio bootstrap del nodo 1, join degli altri due nodi e verifica effettiva
   `wsrep_cluster_size=3`, `wsrep_cluster_status=Primary`,
   `wsrep_ready=ON`, `wsrep_local_state_comment=Synced`.
2. Inserimento di 30 record tramite connessioni ai tre membri; confronto
   del numero di righe su ogni nodo.
3. Arresto improvviso del nodo 3, quorum Primario dei nodi 1+2 (size=2),
   ulteriori dieci scritture e verifica delle 40 righe sui membri superstiti.
4. Riavvio del nodo 3, verifica join e riallineamento a tre membri
   con 40 righe identiche.
5. Backup logico FULL sul nodo 2 mediante
   `multi-backup-native plugins database-backup`, con manifest e chunk
   cifrati age/zstd, parità e verifica; profilo con
   `"galera_expected_nodes":3` (controllo prima **e dopo** il dump).
6. Seconda perdita controllata di nodo 3: il backup con un requisito di tre
   membri è rifiutato mentre ne rimangono due Primari; nessun nuovo
   manifest cifrato pubblicato; successivo rientro.
7. Dopo il FULL, rotazione binlog sul nodo 2, scrittura di 20 transazioni
   sui soli nodi 1 e 3, e acquisizione della posizione finale sul nodo 2.
   L'uso di `log_slave_updates=ON`, `wsrep_gtid_mode=ON`,
   `binlog_format=ROW` e GTID domain differenziati rende possibile la
   verifica della cattura dei write-set remoti.
8. Una transazione successiva con ID 999 viene esclusa dalla finestra PITR.
   Il binlog del nodo 2 viene copiato dopo lo stop del cluster, cifrato e
   verificato tramite la CLI Rust, e ripristinato su percorso nuovo.
9. Il FULL viene ripristinato su un server MariaDB 11.4 **standalone
   indipendente**, quindi viene applicato il log tramite
   `mariadb-binlog --start-position --stop-position`.
   Esito verificato: **60 righe, somma ID 1830, ID 999 assente**.
10. Un secondo restore sullo stesso database è rifiutato senza cancellare o
    sovrascrivere i dati.

## Protezioni Rust

`rust/src/database.rs` interpreta `galera_expected_nodes` soltanto per
un input MariaDB e ammette 2–64 nodi. Impone membership Primaria,
stato Synced e wsrep_ready ON, prima e dopo il dump. Se il dump o la
verifica wsrep falliscono il manifest non deve diventare utilizzabile.
Un profilo Galera è rifiutato come destinazione di `database-restore-new`:
il recupero deve essere esplicito verso un nuovo database isolato.

## Limiti della certificazione

È un **collaudo di integrazione reale in CI su tre container**, non ancora
una certificazione Galera distribuita su tre host fisici. Non sono inclusi:
partizioni di rete/split-brain forzati, guasti simultanei o perdita totale
del quorum, workload di DDL concorrenti, SST sotto carico, SST autenticati
e cifrati, test con versioni MariaDB diverse, GTID multi-file/multi-source,
failover con cambio del nodo che archivia i binlog, PITR automatico del
scheduler/agent, continuità del log attestata senza quiescing, hard fencing
del writer, retention e backup binlog continuo senza staging. I binlog del
laboratorio vengono momentaneamente copiati in chiaro in spazio temporaneo
prima della loro archiviazione age: **non applicare queste procedure senza
modifiche a un ambiente di produzione**.

Il test usa le porte loopback 13316–13319, Docker, age, jq e
`mariadb`/`mariadb-binlog`; richiede il binario Rust compilato
`rust/target/debug/multi-backup-native`.
