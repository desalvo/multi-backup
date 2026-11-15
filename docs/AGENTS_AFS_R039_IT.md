# Multi Backup r039 — Guida agenti Rust e AFS (anteprima)

**Stato:** nuova implementazione di sviluppo, non certificata e non ancora distribuita. La versione r038 è la base Rust del server; i nuovi pacchetti r039 sono separati dall'applicazione principale. Non attivare agenti con permessi privilegiati o su backup unici prima dei test end-to-end.

## Modello operativo

- **AFS locale:** il processo autorizzato usa il client esterno OpenAFS per produrre un dump di un volume scelto esplicitamente.
- **AFS agentless SSH:** il comando vos viene costruito da argomenti validati; connessione SSH con chiave autorizzata e chiave host fissata nel known_hosts; il dump ritorna via stdout SSH.
- **AFS agente pull:** l'agente effettua una connessione TLS in uscita, richiede solo job approvati e li esegue se il proprio profilo locale corrisponde esattamente all'hash autorizzato. Se il processo è fermo non esegue job.
- **AFS agente push:** l'agente avvia un proprio profilo autorizzato e propone un dump cifrato al server, indipendentemente dalla schedule server. Ogni backup identifica agente, account, cella, volume, profilo, timestamp, ID e recipient age.
- **Altri plugin:** l'agente è un processo Rust generico estensibile; in r039 il primo esecutore integrato è AFS. Script e plugin esterni richiedono profili esplicitamente approvati; non si eseguono shell arbitrarie provenienti dalla rete.

La semantica AFS è **per volume**, non per directory. Il dump completo è vos dump -id NOME_VOLUME -cell CELLA. Per un dump incrementale usare vos dump -id NOME_VOLUME -time "mm/dd/yyyy HH:MM"; la data deve corrispondere al dump precedente senza lacune. Il restore AFS richiede vos restore verso un volume nuovo e, per incrementali, prima il full poi gli incrementi nell'ordine corretto; la ricostruzione AFS completa richiede ancora certificazione su un server reale.

Le modalità di autenticazione AFS configurabili comprendono token già disponibili sul client, keytab Kerberos con kinit/aklog e -localauth **soltanto sul fileserver AFS autorizzato**. -localauth non va combinato con -cell, mentre la SSH identity per l'accesso all'host remoto è una credenziale diversa da quella AFS. L'esecuzione automatica keytab/localauth è ancora soggetta a gate di sicurezza e non va confusa con il solo piano di comando. Sono possibili profili di comandi personalizzati mostrati al setup; la modifica richiede approvazione e audit, mai esecuzione remota arbitraria.

## Identità, approvazione e anti-spoofing

Ogni agente ha ID, utente, token ad alta entropia e **chiave Ed25519 dispositivo** esclusiva. Le richieste sono firmate su metodo, percorso, SHA256 dei dati, SHA256 dei metadati, sequenza monotona e timestamp. Il server deve associare il token e la chiave pubblica all'agente e a un utente registrato e richiedere approvazione amministrativa. Rifiutare agenti pending/revocati, credenziali sbagliate, replay, timestamp scaduti, firme alterate e modifiche non approvate ai profili. Solo connessioni HTTPS con certificato valido, senza eccezioni insecure. Chiave privata e token sono file privati locali separati; non vanno nei manifest o nella CLI.

## Setup iniziale (Linux/macOS)

~~~bash
mkdir -p "$HOME/.config/multi-backup-agent"
chmod 700 "$HOME/.config/multi-backup-agent"
multi-backup-agent init --id afs-host-01 --user backup-operator \
  --server https://backup.example.org/ \
  --config "$HOME/.config/multi-backup-agent/agent.json"
multi-backup-agent status --config "$HOME/.config/multi-backup-agent/agent.json"
~~~

L'output iniziale espone la **chiave pubblica** da verificare e approvare sul server. Un amministratore prepara il token registrato sul server e lo trasferisce al file agent-server-token.txt con permessi 0600. Nessun servizio viene avviato automaticamente durante l'installazione.

Il profilo locale deve essere JSON con campi id, recipient, enable_push e job AFS (id, cell, volume, auth, execution, mode). Esempio:

~~~json
{
 "id":"volume-test",
 "recipient":"age1SOSTITUIRE_CON_CHIAVE_PUBBLICA",
 "enable_push":true,
 "job":{
  "id":"volume-test","cell":"example.org","volume":"user.test",
  "auth":{"type":"existing_token"},
  "execution":{"type":"managed_agent","agent_id":"afs-host-01"},
  "mode":{"kind":"full"},"server":null,"partition":null,
  "command_profile":null
 }
}
~~~

Memorizzarlo in profiles/volume-test.json, configurarlo e approvarlo sul server prima di permettere comandi remoti. Preview e attività:

~~~bash
multi-backup-agent preview --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent run --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent push --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent poll --config ~/.config/multi-backup-agent/agent.json --once true
~~~

Il dump AFS passa sullo stdout al processo age; **il plaintext non viene scritto nello spool**. I file di spool rimangono cifrati; se la rete fallisce vengono trattenuti per riesame, non cancellati. Gli agenti non ascoltano su una porta di rete.

## Pacchetti previsti sulla matrice esistente

| Sistema | Architetture | Pacchetto |
| --- | --- | --- |
| macOS 13+ | arm64, x86_64 | PKG, opzionale esempio LaunchAgent |
| Windows | x64 | EXE Inno Setup, avvio manuale o Task Scheduler approvato |
| Debian 12/13 | amd64, arm64 | DEB con unit systemd disabilitata |
| Rocky Linux 8/9/10 | x86_64 | RPM con unit systemd disabilitata |
| AlmaLinux 8/9/10 | x86_64 | RPM con unit systemd disabilitata |

AFS richiede vos esterno, e nelle varianti che usano keytab anche kinit/aklog: gli strumenti AFS/krb5 non vengono incorporati perché versioni e credenziali dipendono dall'host. Il pacchetto agente non introduce Python, Tk o PyInstaller.

## Gate obbligatori

Non considerare certificata la r039 finché non sono superati: CI per tutti gli installer, test AFS full/incremental/restore su celle reali, SSH key rotation/host spoofing, gestione keytab e localauth con privilegi, server enrolment→approval→revoke, anti-replay anche dopo restart, TLS e certificato server, push/pull reali, interruzione agent e recupero, autenticazione dei profili e scripts, ripristino byte-identico di dati ricevuti, installazione/uninstall/upgrade/rollback, firma/notarizzazione e analisi di sicurezza. La documentazione va aggiornata a ogni modifica.

## r039 — parallelismo e limiti degli agenti

Il server imposta una policy per agente tramite POST /api/agents/admin/limits; il file JSON dell'agente contiene limits propri. Per pull la risposta del poll contiene i limiti del server; per push l'agente ottiene prima la policy server tramite GET /api/agents/policy, pur scegliendo autonomamente quando iniziare. La policy effettiva è sempre la più restrittiva.

Campi: max_concurrent_jobs (1–256), threads_per_job (1–64), max_transfer_bytes_per_sec (byte/s), max_host_cpu_percent, max_host_memory_percent, min_free_memory_bytes. Default 4 job, 4 thread e nessuna soglia opzionale; es. 10485760 B/s = 10 MiB/s. Si possono eseguire in parallelo dump di VOLUMI AFS differenti, ma NON dividere il dump di un singolo volume. Server e agente limitano il traffico ciphertext; soglie CPU/RAM sono cooperative, non cgroups hard. Per SSH agentless non sono garantite soglie load/memoria sul remoto senza un probe approvato. [Manuale di cluster IT](CLUSTER_ORCHESTRATION_R039_IT.md) e [EN](CLUSTER_ORCHESTRATION_R039_EN.md).

## r039 — approvazione obbligatoria dei profili e concorrenza multiprocesso

La registrazione e approvazione dell'agente **non** autorizza automaticamente ogni profilo/comando. Preparare un file profiles/ID.json nell'area privata dell'agente e usare `multi-backup-agent preview --config PATH --profile ID`. Il comando mostra il piano vos e il relativo `profile_sha256`; il gestore deve confrontare l'hash con il contenuto sottoposto a revisione. Solo dopo approvazione indipendente, chiamare **POST /api/agents/admin/profile**, autenticato come amministratore con CSRF e recovery authorization separata:

~~~json
{"agent_id":"afs-host-01","profile_id":"volume-test",
 "profile_sha256":"64-caratteri-hex-del-file-json-revisionato",
 "provider":"afs"}
~~~

Sostituire il valore illustrativo con l'hash SHA-256 reale emesso da `preview`. Il server registra e approva esattamente quella coppia profilo/hash: `POST /api/agents/admin/enqueue` e qualsiasi upload cifrato (push o pull) vengono rifiutati se hash/profilo non corrispondono, se l'agente è revocato oppure se il pull non possiede una lease valida. L'upload viene rifiutato **prima** di ricevere il flusso cifrato non autorizzato, e la validazione viene ripetuta durante il commit. Ogni modifica del file richiede una nuova approvazione.

Un agente limita `max_concurrent_jobs` anche fra diversi processi CLI e tra job push/pull tramite slot-lock in `encrypted-spool`: quando tutti gli slot sono occupati, il nuovo job viene rifiutato senza superare il tetto; il lock viene rilasciato alla fine o dopo terminazione del processo. Non dichiarare ancora garantita la consistenza di schedulazioni attraverso host con device identity clonata: chiavi private e token non vanno copiati fra macchine.

## r039 — Limite aggregato di upload tra processi dell'agente

Il campo **facoltativo** `max_host_transfer_bytes_per_sec` in
`agent.json` limita i byte al secondo complessivamente caricati verso il
server da tutti i processi dello **stesso agente sullo stesso host**.
Esempio: `"max_host_transfer_bytes_per_sec": 10485760` per 10 MiB/s.
I limiti `limits.max_transfer_bytes_per_sec` provenienti dal server rimangono
separati e si applicano a ciascun flusso; l'agente non può allentarli.
Il contatore in `encrypted-spool/.host-upload-budget-v1.json` è condiviso tramite
lock del filesystem, conservato con permessi privati (0600, directory 0700)
e aggiornato prima di leggere i byte. Conta anche prenotazioni non consumate:
può ridurre temporaneamente il throughput, mai aumentarlo.
Non cancellare/sostituire il file mentre ci sono processi attivi.
Uno stato corrotto, un rollback dell'orologio o permessi insicuri bloccano
l'upload e richiedono intervento dell'amministratore.

Questa versione limita l'**upload HTTPS dell'agente**; non limita l'intera
banda dell'host, il dump AFS locale, il server, più host distinti o più pod
Kubernetes. Per questi serve una quota condivisa cross-node e, per una garanzia
rigida su CPU/RAM/rete, cgroups/Job Objects o controlli di rete a livello OS.
Non considerare questa funzionalità certificata senza test su host reali.
