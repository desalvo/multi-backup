# Multi Backup — runner HA fisico self-hosted, senza test duplicati (r168)

## Architettura raccomandata e isolamento

**Un solo runner effimero su un nodo di controllo**, preferibilmente quarto host o VM isolata dall'hypervisor che esegue il failover, con accesso SSH strettamente limitato a **tre host fisici** del laboratorio. I tre host hanno servizi PostgreSQL 16, Patroni ed etcd con TLS, quorum 2/3 e storage/failure domain dichiarati. Il runner rimane operativo anche quando si spegne uno dei tre nodi.

**Repository Multi Backup pubblico:** registrare un runner persistente e fidato su questo repository espone il lab a codice introdotto da PR/fork. GitHub scoraggia i self-hosted su repository pubblici. Preferire un **repository privato dedicato al laboratorio** (esempio NON creato automaticamente: `desalvo/multi-backup-ha-lab`) con policy di approvazione e runner effimero; il job fa checkout di un **commit SHA immutabile** del repository pubblico. Non eseguire repository di terzi, PR o workflow non revisionati sul runner con chiavi SSH del lab.

## Installazione e registrazione

1. Creare/approvare manualmente il repository **privato** per l'orchestrazione. Vai su **Settings → Actions → Runners → New self-hosted runner** e scegli Linux x64. Segui gli URL di download/checksum e il token di registrazione temporaneo mostrati **in quel momento** da GitHub, mai copiarli nei file del progetto. Il token scade rapidamente.
2. Installare il runner su una macchina usa-e-getta sotto un account Linux *non root*, senza sudo illimitato, senza Docker socket, SSH agent forwarding o credenziali di produzione. Registrarlo con etichetta `multi-backup-ha-field` e l'opzione `--ephemeral`; GitHub assegna un solo job e poi deregistra quel runner. Distruggere/reimpostare la macchina di controllo dopo il job.
3. Creare l'environment GitHub `multi-backup-ha-lab` nel repo privato, limitare ai branch approvati, abilitarne **required reviewers**, e ridurre al minimo `GITHUB_TOKEN` usando `permissions: contents: read`.
4. Sul runner, predisporre `$HOME/.ssh/known_hosts` da fingerprint SSH convalidate **fuori banda** e una chiave SSH solo read-only per ciascuno dei tre host. `StrictHostKeyChecking=yes`; host autorizzati tramite `authorized_keys` vincolati dove possibile. Non usare root o password in CI.
5. Copiare nel **repository privato** il semplice wrapper `docs/examples/ha-private-runner.workflow.yml` come `.github/workflows/ha-field.yml`, aggiornando il `ref` al commit pubblico revisionato e attivare da **Actions → HA 3 host → Run workflow** con i tre nomi DNS del lab. Non salvare i dati del laboratorio nel repository pubblico.

### Non duplicazione

Il **codice del test rimane unicamente in**
`scripts/ci/ha-three-host-field-r166.py`. Il wrapper privato non duplica alcuna assert; fa checkout del codice revisionato ed esegue una sola volta il *preflight reale*. I fixture negativi restano nella CI hosted `.github/workflows/r166-validation-fast.yml`, non nel runner fisico. Il test di failover e partition nella CI hosted `patroni-netns-real-ha-r149.py` è già stato superato ma riguarda un unico host con tre stack isolati: non lo rilanciamo per chiamarlo «test fisico».

Il precedente workflow in repo pubblico `.github/workflows/ha-three-host-field-r166.yml` è stato mantenuto come opzione di compatibilità, ma limitato al solo preflight, serializzato e con environment dedicato; *non registrare qui un runner con accesso al laboratorio senza una revisione esplicita dei rischi*. GitHub mostra `workflow_dispatch` solo se il workflow è presente sul default branch; il wrapper del repo privato risolve questa limitazione senza un merge prematuro delle modifiche applicative su `main`.

### Scope di certificazione

Il preflight legge via SSH `/etc/machine-id`, kernel boot IDs, `patronictl list` da ogni host, verifica il servizio etcd/Patroni e una sola primaria coerente. **Non interrompe processi, alimentazione o rete** e non dimostra da solo domini di guasto fisicamente separati.

Dopo l'attestazione manuale dei tre domini fisici, le prove distruttive saranno eseguite nel laboratorio **solo con approvazione esplicita dell'operatore**: strict sync WAL ACK, arresto primario, elezione e recupero, split-brain/old-primary write fencing, perdita quorum DCS, ricongiunzione e restore da nodo distinto. Non sono ancora certificati su tre host fisici; non simulare un PASS. Il runner privato può riutilizzare gli **assert e le evidenze** del test esistente dove il sistema lo consente, evitando duplicazioni di codice e di risorse.

## Readiness CI software (separato dall'HA fisico)

Il gate `.github/workflows/software-readiness-r168.yml` riusa `rust-focused.yml`, `agent-cluster-live-pg.yml`, `r166-desktop-xvfb.yml` e gli script già presenti `scripts/test-server-gateway-container.sh`, `tests/kubernetes/test-kind-cluster.sh` sullo **stesso commit**. La sua esecuzione su branch `ci/r168-software-field-gates` non attiva simultaneamente i vecchi workflow push, evitando di ripetere gli stessi test. Un solo OCI image viene riusato dal gateway e dal cluster Kind.

**Risultato del gate da verificare prima di promuovere.** Non affermare che questa suite certifica i tre host fisici, installatori firmati per tutti i sistemi, plugin mancanti o tutta la RC.
