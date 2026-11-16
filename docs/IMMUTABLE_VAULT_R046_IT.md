# Multi Backup r046 — vault per archivi anti-ransomware (anteprima funzionale)

Questo è un **deposito di backup** distinto dal precedente \`vault .agevault\` per la **chiave privata age**. È implementato in Rust (\`rust/src/immutable_vault.rs\`) ed è accessibile dal CLI, dalla GUI desktop e dal server autenticato. NON è corretto chiamarlo WORM certificato senza un'infrastruttura di conservazione indipendente.

## Cosa funziona e cosa NON è ancora certificato

- Crea un archivio tar.gz.age cifrato direttamente dal sorgente, con un nuovo ID casuale per ogni backup e una firma di integrità SHA-256 a lato.
- Nessuna operazione di overwrite, delete, purge o GC è esposta dal modulo, dal CLI o dal server.
- Prima di un restore valida identificativo, checksum del ciphertext, autenticità age, membri e path dell'archivio; estrazione solo in una directory nuova e separata dal vault.
- GUI native Rust: nuova sezione "Deposito backup anti-ransomware" nella pagina Vault; usa chiave age sbloccata in RAM per il restore oppure chiave esplicita.
- Server Rust: \`GET /api/vault-store/status\`, \`GET /api/vault-store/list\`, \`POST /api/vault-store/backup\`, \`POST /api/vault-store/restore\`. Sessione amministratore; CSRF e origin su ogni POST; **ripristino necessita anche codice indipendente di autorizzazione recupero** in ambiente hardened; audit di backup e restore; nessuna API delete.
- Sul server il backup crea una nuova copia cifrata dello **store dati Multi Backup corrente**, non legge percorsi filesystem arbitrari inviati dal browser. Restore in un nuovo percorso sotto MULTI_BACKUP_EXPORT_ROOT; **non modifica lo store originale**.
- Test CLI reali con \`age\`: backup cifrato, list, verify, restituzione byte-identica, identità errata, path traversal, doppio ripristino, ciphertext corrotto e nessuna scrittura nel vault stesso.
- **Non implementato/certificato:** enforcement fisico WORM, S3 Object Lock Compliance/retention, credenziali write-only indipendenti su nodo separato, audit antimanomissione esterno, crash/power-loss con commit atomico unico di archivio+sidecar, restore remoto diretto cross-server, snapshot metadati/ACL/xattr, carichi elevati. Il filesystem del server, se accessibile a un attaccante con privilegi equivalenti a quelli del sistema, può ancora essere cancellato.

## Preparazione locale: CLI

\`\`\`bash
# Directory vault già montata e creata da operatore, idealmente su nodo separato
mkdir -m 0700 -p "$HOME/vault-lab"
age-keygen -o "$HOME/vault-test.key"
RECIPIENT=$(age-keygen -y "$HOME/vault-test.key")

multi-backup-native vault-store init \
  --root "$HOME/vault-lab" --recipient "$RECIPIENT"
multi-backup-native vault-store backup \
  --root "$HOME/vault-lab" --source /path/to/disposable-test-data
multi-backup-native vault-store list --root "$HOME/vault-lab"
multi-backup-native vault-store verify \
  --root "$HOME/vault-lab" --id SNAPSHOT_ID --identity "$HOME/vault-test.key"
multi-backup-native vault-store restore \
  --root "$HOME/vault-lab" --id SNAPSHOT_ID \
  --identity "$HOME/vault-test.key" --destination "$HOME/new-disposable-restore"
\`\`\`

Il vault deve essere **preesistente e disgiunto** dal sorgente. Utilizzare dischi/dati di prova. L'applicazione non crea automaticamente uno storage WORM remoto e non può impedire al proprietario della VM di cancellare un normale filesystem.

## Server

Impostare \`MULTI_BACKUP_IMMUTABLE_VAULT_ROOT\` sul server Rust verso una directory **separata** dalla live \`MULTI_BACKUP_DATA_ROOT\`, già inizializzata con \`vault-store init\`. Impostare \`MULTI_BACKUP_AGE_IDENTITY_FILE\` su un'identità consentita solo al processo di recupero. Il server rifiuta all'avvio il percorso vault sovrapposto al normale store e la configurazione senza manifest vault.

Nel dashboard viene mostrata una sezione Vault con elenco e comandi di creazione/recupero. La creazione produce un archivio non sovrascritto, il restore usa la medesima approvazione fuori banda del recupero normale. Tenere il server esposto solo tramite proxy TLS hardened e accessi separati.

## Requisiti per certificare la dicitura anti-ransomware

1. Storage fisicamente indipendente da backup server / host sorgente, root/admin separati.
2. Politica **S3 Object Lock Compliance** / WORM, o altro enforcement append-only lato storage con retention non abbreviabile dai normali operatori.
3. Credenziali **ingest** che consentano soltanto creare nuovi oggetti (non cancellare, sovrascrivere, alterare retention); credenziali **restore** separate, con approvazione fuori banda.
4. CI reale in laboratorio isolato con tentativi di DELETE, overwrite e modifica retention; devono essere respinti **dal deposito**, non soltanto dalla nostra API.
5. Recupero da chiave offline e confronto bytewise e ACL/xattr; test guasto/crash e verifica retention dopo compromissione credenziali ingest.
6. Solo dopo queste prove si potrà trasformare il catalogo da Preview a Ready e pubblicare una RC con dicitura "anti-ransomware qualificato".

## GitHub e Dropbox in CI: prove reali con risorse isolate

Il motore Rust GitHub legge le credenziali da \`gh auth\`, quello Dropbox da \`MULTI_BACKUP_DROPBOX_TOKEN\`. I test reali devono usare **account di prova isolati**, scoped read-token per backup e separato write-token per restore, archivi pubblici age del test, directory nuova e cleanup esplicito dei soli test. NON usare token personali nel repo pubblico; configurare le credenziali nel repo CI privato \`desalvo/multi-backup-labs\`. I test senza credenziali restano NOT RUN, non PASS.

**Requisito RC:** CI completa dell'integrazione Rust su main, test GUI/installers, failover server/agent, almeno un backup e restore reale end-to-end del core. GPFS è escluso come sperimentale; vault per archivi resta in Preview fino a enforcement remoto WORM verificato.
