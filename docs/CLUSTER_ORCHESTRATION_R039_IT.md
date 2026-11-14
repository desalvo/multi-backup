# Multi Backup r039 — Parallelismo e server a repliche (manuale tecnico IT)

**Stato:** 15 novembre 2026 — PR #11, nuova architettura Rust in sviluppo. **NON è una RC certificata**. La r038 resta la base della PR #10. Questa guida documenta il codice effettivamente presente, i prerequisiti e i gate ancora necessari; i manuali storici Python/Tk non attestano il comportamento Rust.

## 1. Modello comune a tutti i plugin

Ogni backup ha: input plugin, specifica dei dati (per AFS: cella e volume esatto), trasporto che esegue il backup e destinazione. Il trasporto può essere:

| Modalità | Dove si esegue | Requisiti e stato |
| --- | --- | --- |
| server_local | Replica del server abilitata e con accesso alla sorgente | Esecutore Rust disponibile soltanto per POSIX/NFS/SMB/SSHFS/GPFS CES in anteprima |
| ssh_agentless | Host remoto su connessione SSH | AFS: piani validati, ssh con chiave host fissata, stdout streaming; altri plugin non eseguibili finché non esiste un adapter |
| managed_agent_pull | Host agente che esegue un profilo locale approvato, richiesto dal server | Coda agente PostgreSQL, poll firmato, profili locali, upload cifrato; integrazione end-to-end da qualificare |
| managed_agent_push | Host agente che decide autonomamente quando eseguire un profilo | Client push Rust, policy server obbligatoria, upload firmato; AFS primo esecutore di anteprima |

Non basta registrare un plugin nel catalogo per renderlo eseguibile. Per AFS un singolo dump è **uno stream indivisibile**: si parallelizzano volumi diversi, senza frammentare arbitrariamente i byte di un volume in più repliche. Per altri input i segmenti potranno essere distribuiti solo se l'adapter dimostra consistenza, snapshot e idempotenza.

## 2. Thread, parallelismo, banda, CPU e RAM

Ogni job e agente utilizza il contratto Rust execution_policy::ExecutionLimits:

~~~json
{
  "max_concurrent_jobs": 4,
  "threads_per_job": 4,
  "max_transfer_bytes_per_sec": 20971520,
  "max_host_cpu_percent": 75.0,
  "max_host_memory_percent": 80.0,
  "min_free_memory_bytes": 1073741824
}
~~~

Le unità sono **job contemporanei**, **thread per job**, **byte/secondo per stream** (20 MiB/s nell'esempio), percentuale utilizzo totale CPU, percentuale memoria host utilizzata e **byte RAM libera minima**. I default sono 4 job e 4 thread; limiti di banda/CPU/memoria disabilitati finché non configurati. Limiti ammessi: 1–256 job, 1–64 thread/job, massimo 2048 thread aggregati; banda configurata almeno 1024 B/s, CPU/RAM (0,100].

**Pull:** il server restituisce la policy registrata per l'agente; l'agente applica il valore più restrittivo fra server e policy locale. **Push:** l'agente decide la partenza, ma richiede preventivamente al server la policy firmata/autenticata, poi esegue con la policy più restrittiva. Il server applica anche il rate limit sul flusso age in ingresso. Se le soglie locali non sono verificabili, il controllo fallisce senza fingere che siano rispettate.

Il motore AFS applica banda sullo stream di vos dump e controlli CPU/RAM in fase di avvio e periodicamente durante la trasmissione. I limiti RAM/CPU sono **cooperativi**, non limiti kernel hard: cgroups v2, systemd MemoryMax/CPUQuota, Job Objects Windows e controlli hard sul processo remoto via SSH richiedono ancora integrazione e test. Un trasporto agentless non può inventare il load di un host remoto: senza un probe remoto approvato si può limitare la banda ricevuta dal server, ma le soglie CPU/RAM dell'host remoto restano non garantite. Il numero threads_per_job è parte del contratto, ma **non accelera il singolo vos dump** né garantisce multithreading dei plugin che non lo implementano.

## 3. Cluster PostgreSQL e Kubernetes

Ogni replica del server espone la stessa API HTTPS, catalogo condiviso di agenti, job, stati, storage e sessioni utente. La fonte autorevole è **PostgreSQL** con connessione SSL che richiede verifica del certificato e hostname (sslmode=verify-full). Le repliche inviano heartbeat periodici (20 s); un nodo senza heartbeat diventa non eleggibile dopo 90 s. I job sono prelevati con SKIP LOCKED e lease con token per evitare doppie assegnazioni contemporanee.

**Regola vincolante per lo storage:**

- **shared**: backend condiviso realmente montato o raggiungibile da ciascuna replica autorizzata (per esempio CephFS/RWX o un backend object cloud supportato da un adapter). Deve essere registrato esplicitamente e verificato: la stessa path string non dimostra condivisione reale. I job indipendenti possono essere distribuiti su più repliche.
- **node_local**: filesystem che esiste su una replica specifica. Ogni replica vede i metadati nel database, ma SOLO la replica owner può reclamare/eseguire quel job. Gli utenti vedono lo stato cluster complessivo anche se non hanno montato fisicamente quel filesystem. La restituzione dei file di restore da una replica non-owner richiede ancora proxy/redirect interno autorizzato.
- **mai** trattare implicitamente uno storage locale come condiviso o assumere che un volume AFS sia divisibile tra repliche.

Configurazione server di sviluppo:

~~~text
MULTI_BACKUP_CLUSTER_DATABASE_URL=postgresql://utente:...@postgres.example.org:5432/multi_backup?sslmode=verify-full
MULTI_BACKUP_REPLICA_ID=<nome univoco del Pod>
MULTI_BACKUP_REPLICA_MAX_JOBS=4
MULTI_BACKUP_CLUSTER_STORAGE_IDS=shared-cephfs,local-host-a
MULTI_BACKUP_CLUSTER_EXECUTOR=enabled
MULTI_BACKUP_AGENT_UPLOAD_ROOT=/encrypted/shared-agent-upload
MULTI_BACKUP_AGENT_PROXY_SECRET_FILE=/secrets/trusted-proxy-secret
MULTI_BACKUP_PUBLIC_ORIGIN=https://backup.example.org
~~~

**Mai** incollare password reali nella configurazione Git, YAML o manuali; montare un Secret K8s di sola lettura. La directory agent upload deve essere su storage RWX per bilanciamento fra repliche; imposta 0700. Il secret di attestazione proxy è iniettato esclusivamente dal gateway fidato, con header x-mb-trusted-https-proxy e x-forwarded-proto=https. I client non devono poter impostare/influenzare l'header proxy: l'ingress deve sempre rimuoverlo dall'input e sostituirlo da un segreto non esposto. Se mancano TLS, attestation proxy o database condiviso, le API degli agenti devono essere disabilitate.

### API amministrative disponibili

Tutte richiedono login admin, gateway privato e CSRF per modifiche:

| Metodo e API | Operazione |
| --- | --- |
| GET /api/cluster/status | Stato unico di repliche, job e storage |
| POST /api/cluster/storage | Registra storage con visibility shared o node_local, owner_replica per node_local e confirmed_shared per shared |
| POST /api/cluster/jobs | Registra job plugin/trasporto/limiti/specifica nella coda PG |
| GET /api/agents/admin/list | Stato e connettività degli agenti |
| POST /api/agents/admin/enroll | Registra ID, user e public key Ed25519; restituisce token **una sola volta** |
| POST /api/agents/admin/approve | Approvazione esplicita agent ID |
| POST /api/agents/admin/revoke | Revoca agent ID |
| POST /api/agents/admin/limits | Aggiorna la policy CPU/RAM/parallelismo/banda dell'agente |
| POST /api/agents/admin/enqueue | Accoda job pull con profile_id e hash del profilo |

Le API agent GET /api/agents/policy, GET /api/agents/poll e POST /api/agents/upload richiedono invece token, utente, chiave Ed25519 approvata, firma su body/metadati/sequenza e TLS attestato. Il server persiste nonce unici per accettare richieste concorrenti fuori ordine senza consentire replay. La ricezione dei file cifra **prima** sul nodo agente e riversa su spool server, non considera la ricezione equivalente a verifica del dump AFS decifrato.

### Esempi di definizione dei job

Storage condiviso dichiarato e confermato dall'amministratore:

~~~json
{
  "storage": {
    "storage_id":"shared-cephfs",
    "backend":"posix",
    "visibility":"shared",
    "owner_replica":null
  },
  "confirmed_shared":true
}
~~~

Job diretto su filesystem condiviso con source plugin POSIX:

~~~json
{
  "job_id":"backup-docs-001",
  "plugin_id":"posix",
  "storage_id":"shared-cephfs",
  "source_transport":"server_local",
  "execution_limits":{"max_concurrent_jobs":4,"threads_per_job":4,
    "max_transfer_bytes_per_sec":20971520},
  "job_spec":{"source":"/data/source","store":"/data/encrypted-store",
    "identity":"/run/secrets/age-identity","label":"backup-docs",
    "namespace":"files","top_namespace":"","mode":"full",
    "snapshot_attested":false}
}
~~~

L'esecutore su replica è **opt-in**. Per ora esegue solo i plugin filesystem POSIX/NFS/SMB/SSHFS/GPFS CES configurati in modalità server_local. I trasporti SSH e agent hanno un canale di gestione proprio ma **non** vengono automaticamente trasformati in jobs di shell sulle repliche. Il claim del lease è implementato, con rinnovo periodico e commit dell'esito vincolato al token; mancano ancora fencing delle scritture di contenuti dopo perdita del lease, crash-restart del writer, job cancellati durante l'esecuzione, QoS globale di tutti i pod e verifiche reali multi-replica. **Non qualificare come exactly-once**.

### Sessioni UI e limiti di condivisione

Le sessioni di login/logout sono condivise in PG quando il cluster è abilitato; così un browser può raggiungere repliche diverse senza perdere il cookie. Il browser espone informazioni comuni tramite endpoint cluster/status. Il flusso OIDC di login conserva ancora alcuni stati pre-login in memoria della replica: richiede affinity/sticky routing oppure migrazione futura al registro condiviso. I filesystem locali rimangono fisicamente disponibili solo sul nodo che li possiede; restore/upload routing interno e dashboard completo per ogni nodo sono ancora in lavorazione.

## 4. Pacchetti, procedure di monitoraggio e RC

L'agente viene costruito separatamente su macOS arm64/x86_64 (PKG), Windows x64 (EXE), Debian 12/13 amd64/arm64 (DEB), Rocky/Alma Linux 8/9/10 x86_64 (RPM). Il servizio non parte né si approva automaticamente. Il server multi-replica è una build Rust comune, con l'immagine OCI prevista dal progetto: il Deployment Kubernetes multi-replica rimane da testare.

Per verificare funzionamento e guasti:
1. Eseguire una prova con due repliche, PG condiviso e uno storage node_local registrato solo sulla prima: la seconda non deve acquisirne job.
2. Con storage shared **reale** e autorizzato, accodare volumi/file indipendenti e osservare i lease distribuiti su entrambe.
3. Spegnere la replica durante un lavoro, verificare scadenza, failover e fencing: nessuna perdita o falso successo di backup.
4. Con l'agente su host remoto verificare token errato, chiave errata, certificato non valido, replay, revoca, arresto agente, profilo alterato, upload interrompibile.
5. Misurare banda, CPU, memoria e parallelismo reale su host agent, server e SSH; misurare inoltre consumo complessivo su cluster.
6. Fare restore AFS full e incremental su un **volume nuovo** e verificare contenuto, cella, ACL e VLDB; confrontare hash dell'output recuperato.

**Gate ancora aperti:** strumenti OpenAFS e keytab/localauth su host autorizzati, hard CPU/RAM per SSH remoto e host, full multiprovider orchestration, storage object writers, push/pull reali, fencing transazionale, ricongiungimento OIDC, routing restore locale da altre repliche, test CI di tutti gli OS e test Kubernetes, firma/notarizzazione degli installer, security audit e rollback.

Riferimenti: [agenti IT](AGENTS_AFS_R039_IT.md), [agenti EN](AGENTS_AFS_R039_EN.md), [catalogo plugin](PLUGIN_MATRIX.md), [certificazione RC](RC_CERTIFICATION_MATRIX.md), [stato sviluppo](DEVELOPMENT_STATUS.md).

## Aggiornamento UI r039 — stato condiviso fra le repliche (15 novembre 2026)

Nell'interfaccia web Rust è stato aggiunto il pannello **Cluster, agenti e backup paralleli**. Visualizza in sola lettura, con accesso amministrativo, l'insieme delle repliche (heartbeat e capacità), storage condivisi o locali con owner, ultimi job con stato/replica/trasporto e agenti approvati, offline o revocati. Il pannello interroga GET /api/cluster/status e GET /api/agents/admin/list sul server raggiunto e riceve lo stesso registro PostgreSQL indipendentemente dalla replica. Nessun input del database viene interpretato come HTML; l'interfaccia usa contenuti testuali sicuri. La gestione di registrazione, policy e cancellazione degli agenti resta effettuata tramite API amministrative protette; non è un'interfaccia di gestione completa degli agenti. Da collaudare su deployment Kubernetes con bilanciatore e sessioni condivise.

