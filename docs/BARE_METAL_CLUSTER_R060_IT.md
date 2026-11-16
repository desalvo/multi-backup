# Multi Backup 0.1.0 r060–r064 — Installazione bare metal, PostgreSQL HA e vault (manuale IT)

**Stato: preview da verificare in CI.** Il percorso server bare metal è in corso di implementazione e NON è una release stable o una certificazione contro un root compromesso. La stessa applicazione Rust gira su Kubernetes e su server fisici/VM; i suoi server condividono un unico **cluster logico** PostgreSQL, non database indipendenti che divergono.

## 1. Sistemi, pacchetti, prerequisiti

- Rocky Linux 9/10 e AlmaLinux 9/10, architetture `x86_64` e `aarch64`: pacchetto `multi-backup-server-*.rpm`.
- Debian 12/13, architetture `amd64` e `arm64`: pacchetto `multi-backup-server_*.deb`.
- Compilare **nativamente nel sistema/architettura destinazione**: `cargo build --release --manifest-path rust-server/Cargo.toml --bin multi-backup-server`, poi `SERVER_PACKAGE_FORMAT=rpm|deb scripts/package-rust-server-unix.sh`.
- Le release di desktop e agenti sono indipendenti: questo pacchetto contiene il server web/API Rust con systemd, esempio di configurazione, SNMP e istruzioni. Non dipende da Python.
- Dipendenze operative: PostgreSQL client (`pg_dump`, `pg_restore`, `psql`), `age`, una authority X.509, Patroni, PostgreSQL, etcd e HAProxy. Patroni, etcd e HAProxy si installano con i pacchetti della distribuzione/repository affidabili e devono essere gestiti come servizi separati.

La prima installazione copia `/etc/multi-backup/server.env` (mode 0600), installa `multi-backup-server.service`, crea account `multi-backup` e le directory private. Non avvia né pubblica il server prima che configurazione e segreti siano pronti. La rimozione del pacchetto non cancella database o backup.

## 2. Topologia bare metal consigliata

```text
       browser HTTPS :443 / monitoring REST :8443 / Prometheus :9502 / SNMP
                          |
                      reverse proxy / ingress fidato
                 +--------+----------+
                 |        |          |
             mb-node1  mb-node2   mb-node3   (Rust server + HAProxy locale)
                 |        |          |
          local TLS:6432 -> HAProxy primary check via Patroni :8008
                 |        |          |
             PostgreSQL1 PostgreSQL2 PostgreSQL3   (VLAN privata DB)
                   Patroni + etcd3 quorum 2/3
                          |
           Vault S3 Object Lock COMPLIANCE separato
```

**Non esporre MAI PostgreSQL 5432, Patroni 8008 o etcd 2379/2380 alle interfacce pubbliche.** Il database deve essere raggiungibile solo fra i nodi autorizzati nella **rete privata del cluster**: 5432 TCP è necessario tra primaria e repliche, non dai browser o da Internet. HAProxy applicativo ascolta soltanto su `127.0.0.1:6432`. Limitare le porte con firewalld/nftables e verificare da una rete esterna che 5432/2379/2380/8008 non rispondano. Usare TLS reciproco per il DCS etcd e certificati PostgreSQL con SAN coerenti.

Ogni nodo ha un ID stabile `MULTI_BACKUP_REPLICA_ID`, una directory locale distinta, un servizio web e un account PostgreSQL con privilegi minimi sul database `multi_backup`. La pagina Cluster mostra repliche, heartbeat, coda dei job, storage e backup inventariati tramite PostgreSQL: da ogni nodo la stessa vista. Il backup fisico locale non diventa accessibile su un altro server solo perché presente nel catalogo; il routing remoto dei restore richiede ancora un canale separato autenticato e non è certificato.

## 3. PostgreSQL HA: Patroni + etcd, non un master fisso

Predisporre **tre nodi** Patroni/PostgreSQL e tre membri etcd (quorum due). Patroni governa elezione del primario, failover e reintegrazione delle repliche (streaming replication, `pg_rewind` quando possibile, oppure nuova base backup). Non eseguire due primari manualmente. Configurare `synchronous_mode: on`, `synchronous_mode_strict: true`, `synchronous_node_count: 1` per garantire un primario e una replica sincrona con scritture bloccate in mancanza di una replica valida. Si può imporre `synchronous_node_count: 2` su tre nodi per conferma su tutte le repliche, **accettando il blocco delle scritture quando una singola replica è indisponibile**.

Patroni non può garantire zero perdita in tutti i casi limite (ad esempio una transazione visibile ma ancora in attesa di conferma di replica) e quorum/disponibilità assoluti sotto partizioni di rete. Testare sistematicamente split-brain, interruzione di rete, promozione, reintegrazione e ricostruzione.

Configurare ogni nodo usando il modello `packaging/ha/patroni-node.yaml.example`, sostituendo IP privati, identificativi, credenziali e certificati. Esempio rete `10.70.0.0/24`, con IP `10.70.0.11`, `.12`, `.13` per PostgreSQL/Patroni/etcd; usare certificati e credenziali diverse da quelle web. Verificare:

```bash
patronictl -c /etc/patroni/patroni.yml list
curl --cacert /etc/multi-backup/secrets/patroni-ca.pem https://10.70.0.11:8008/health
ss -lntup | grep -E ':(5432|6432|2379|2380|8008)'
# da una RETE ESTERNA: 5432, 2379, 2380, 8008 devono essere irraggiungibili
```

Per l'endpoint applicativo locale configurare `packaging/ha/haproxy-postgres.cfg.example`. Se il primario cambia, HAProxy indirizza nuove connessioni soltanto al Patroni primary riconosciuto (health check **HTTPS /primary** con `check-ssl`, SNI e certificato validato dalla CA privata; mai `/health` o `/leader`). La configurazione originale con controllo HTTP in chiaro su :8008 era incompatibile con `restapi.certfile` e avrebbe marcato DOWN tutti i nodi sani. Le connessioni già aperte vengono interrotte e il server Rust deve riprovare operazioni idempotenti. Ogni PostgreSQL presenta un certificato verificabile da `db-ha.internal` (nome che risolve a loopback per il client attraverso il proxy), con CA configurata in `MULTI_BACKUP_CLUSTER_TLS_CA_FILE`.

## 4. Schema, backup DB interno e disaster recovery

Alla partenza Multi Backup crea/aggiorna automaticamente lo schema tramite **migrazioni versionate e checksum SHA-256**, serializzate da `pg_advisory_xact_lock`: una sola replica applica la migrazione, le altre verificano lo schema. **Non cambiare il contenuto delle migrazioni già applicate**; nei futuri upgrade aggiungere nuove versioni e fare prima un backup.

Nella pagina *Database interno* abilitare backup automatico (1, 6, 12, 24 ore o 7 giorni) e destinazione **S3 WORM COMPLIANCE**. Configurare per ogni replica `MULTI_BACKUP_DB_BACKUP_S3_CONFIG` con bucket immutabile indipendente e retention di almeno **30 giorni**, identità age di recupero, `pg_dump` compatibile e directory di staging con permessi 0700. La politica è condivisa in PostgreSQL: un solo server ottiene il lease per ogni backup. Il flusso di pg_dump è cifrato con age prima del primo file e successivamente salvato sullo storage S3 con retention WORM. L'app registra catalogo, job, verifica e scadenze, e fornisce comando di backup immediato.

**Limitazione attuale:** il dump logico non sostituisce un archivio WAL continuo e un test di recovery consistente fra database e backup dati. Nel vault locale append-only non c'è certificazione WORM contro root; la policy ammette solo S3 COMPLIANCE per gli snapshot automatici del DB. In caso di perdita del database, ripristinare su **nuovo cluster isolato vuoto**, con applicazione e agenti fermati, selezionando con criterio un backup DB e gli archivi dati corrispondenti. Non avviare automaticamente il restore sul primario corrente. Vedere `scripts/db-disaster-restore.sh` per la procedura conservativa, che fallisce se il target non è vuoto.

## 5. Prometheus, SNMP e API REST

Tre servizi opzionali, separati dall'API web browser. Nessuno espone connessioni al DB:
- **Prometheus**: `MULTI_BACKUP_PROMETHEUS_ENABLED=true`, bind predefinito `127.0.0.1:9502`, endpoint `GET /metrics` OpenMetrics text 0.0.4. Per bind su IP privato non-loopback è obbligatorio `MULTI_BACKUP_PROMETHEUS_TOKEN_FILE` con bearer di almeno 32 byte e permessi 0600. Dati: nodi online, job queued/running/failed, ruolo PostgreSQL, repliche sync, esiti backup DB.
- **REST**: `MULTI_BACKUP_REST_API_ENABLED=true`, bind `127.0.0.1:8443` oppure specifico IP privato, certificato `MULTI_BACKUP_REST_TLS_CERT_FILE` e chiave `MULTI_BACKUP_REST_TLS_KEY_FILE`. `GET /v1/status`, `/v1/cluster`, `/v1/database` sono **read-only**, solo HTTPS con `Authorization: Bearer mbmon_...`. I token sono creati/revocati nella UI (approvazione offline), sono rilasciati una sola volta e nel database rimane solo l'hash; scope monitor.read, scadenza 1–365 giorni.
- **SNMP**: servizio opzionale `multi-backup-snmp.service` basato su Net-SNMP con extend Rust. Predefinito SNMPv3 autenticato e cifrato (**authPriv**). Configurazione `MULTI_BACKUP_SNMP_ENABLED`, `MULTI_BACKUP_SNMP_VERSIONS` (`3`, `1,2c`, `1,2c,3`), `MULTI_BACKUP_SNMP_BIND`, `MULTI_BACKUP_SNMP_PORT` (161) e `MULTI_BACKUP_SNMP_COMMUNITY` (`public`). SNMPv1/v2c con community pubblica è **solo loopback**: non configurarlo su VLAN pubblica; usare SNMPv3 per il monitoraggio remoto. Net-SNMP espone le misure come NET-SNMP-EXTEND-MIB, senza promettere OID enterprise non assegnati.

Per SNMPv3 creare il relativo utente con `net-snmp-create-v3-user` o le procedure della distribuzione, poi eseguire `multi-backup-configure-snmp`, controllare `/etc/multi-backup/snmpd.conf` e abilitare il servizio. Il default resta disabilitato.

## 6. Configurazione e avvio

```bash
# RPM oppure DEB, pacchetto compilato per quella CPU e distribuzione:
sudo dnf install ./multi-backup-server-*.rpm
# oppure: sudo apt install ./multi-backup-server_*_amd64.deb
sudo install -d -m0700 -o multi-backup -g multi-backup /var/lib/multi-backup/data /var/lib/multi-backup/exports /var/lib/multi-backup/db-staging
sudo install -d -m0750 -o root -g multi-backup /etc/multi-backup/secrets
# Segreti: root:multi-backup mode 0640; nessun accesso ad altri utenti.
sudoedit /etc/multi-backup/server.env
sudo systemctl daemon-reload
sudo systemctl enable --now multi-backup-server
sudo systemctl status multi-backup-server
```

Il reverse proxy HTTPS deve rimuovere dagli input esterni e iniettare personalmente gli header trusted/private-gate; la porta HTTP interna 8080 non va pubblicata. Un file `server.env` copiato con valori di esempio non deve permettere l'avvio funzionante senza segreti/identità reali. Testare login da tutti i nodi, status cluster, backup schedulato, restore isolato, scadenza delle sessioni, revoca token, SNMPv3, Prometheus, failover Patroni, rapporto WORM remoto e impossibilità di accedere al DB dalla rete esterna.

## 7. Requisiti non ancora certificati

Conformità systemd/RPM/DEB su tutte le dodici combinazioni OS/CPU, restore DB/backup dati coordinato, Patroni failover reale, certificazione SNMPv3 e TLS API, disponibilità dei pacchetti firmati, fencing exactly-once, protezione WORM da root sull'host di storage: **test e code review necessari**. La prima RC non si pubblica se i gate richiesti restano rossi.
