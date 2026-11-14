# Multi Backup — guida operativa Rust r038 (IT)

**Stato:** anteprima di sviluppo nel branch della PR #10, **non** release distribuita né prodotto certificato. Ultima versione pubblicata: Python/Tk r018. La r038 usa programmi Rust nativi; la migrazione del contenuto dei vecchi archivi Python è facoltativa e non blocca la RC. **Le procedure che scrivono o cancellano dati vanno usate solo su un ambiente di prova** finché i gate nella [matrice RC](RC_CERTIFICATION_MATRIX.md) non sono chiusi.

Documenti collegati: [storage e formato](CHUNK_STORAGE_V1.md), [catalogo plugin](PLUGIN_MATRIX.md), [protezione](BACKUP_PROTECTION.md), [manutenzione](MAINTENANCE_POLICY.md), [sicurezza](SECURITY.md), [stato sviluppo](DEVELOPMENT_STATUS.md) e [handoff](HANDOFF.md).

## 1. Funzionalità e limiti realmente disponibili

| Componente | r038 sul branch | Limitazioni per la RC |
|---|---|---|
| CLI/GUI Rust senza Python | Codice e pacchetti anteprima, GUI con opzioni full/incremental, informazioni catena, purge e GC | Installazione reale, copertura UI, sicurezza e CI dell'ultimo commit da certificare |
| Storage POSIX cifrato | Backup full e incremental, manifest completi, age+zstd, HMAC, dedup, parità 8+2, restore in nuova directory | Dati molto grandi, fault injection, coerenza ACL/xattr e crash recovery non certificati |
| Plugin filesystem | POSIX→POSIX eseguibile; su Linux input NFS, SMB, SSHFS, GPFS CES in mount **esatto e read-only** → store POSIX | La dichiarazione di snapshot NON prova la consistenza; niente restore in remoto né writer universale |
| GitHub / Dropbox | Engine Rust dedicati preliminari, comandi create-new verso credenziali di destinazione distinte | Restore cross-account completo, selettivo, metadata/permessi e casi reali non certificati |
| Incrementali | Collegati a un parent; ogni manifest contiene l'intero inventario dei riferimenti ai chunk | Nessuna dipendenza da delta plaintext, ma verifica end-to-end obbligatoria |
| Verifica catene | Manuale e nel runner Rust, predefinita sull'intera catena | Avvisi, allarmi esterni e cronologia delle verifiche da completare |
| Purge | Rebase del successore o promozione a FULL, soltanto manifest | Figli protetti bloccano la modifica; non cancella chunk |
| GC | Anteprima ed esecuzione separata, lock sullo store, soglia 48 ore, conserva parità condivisa | Repack 8+2 misto, GC remoto, transazioni crash-safe e policy WORM non certificate |
| Schedule | CLI \`schedule run\` o \`schedule daemon\`, stato persistente e intervallo per job | Configurazione GUI/sistema operativo, restart e logging di produzione da certificare |

Non confondere **plugin registrato**, **preview eseguibile** e **provider certificato**. Il catalogo comprende 32 plugin, 53 ruoli dichiarati e cinque coppie di backup filesystem in preview; nessuna delle 682 combinazioni generiche è ancora certificata per produzione.

## 2. Preparazione di un laboratorio cifrato

Installare l'eseguibile Rust \`multi-backup-native\` e gli helper \`age\` / \`age-keygen\` in una macchina di prova. \`git\`, \`gh\` e \`git-lfs\` servono ai workflow GitHub. Conservare la chiave privata fuori dallo storage, in una cartella riservata e su disco cifrato. In questa fase le operazioni del chunk store ricevono un percorso locale a un'identità age: il vault nella GUI non è ancora integrato con tutte le operazioni.

Esempio Linux/macOS (sostituire i percorsi e il destinatario age **reale**):

\`\`\`bash
mkdir -p "$HOME/.config/multi-backup/keys" "$HOME/mb-r038-test"
chmod 700 "$HOME/.config/multi-backup/keys"
age-keygen -o "$HOME/.config/multi-backup/keys/r038.txt"
# Leggere la public key age1... prodotta dal comando, senza divulgarne la privata
multi-backup-native --version
multi-backup-native storage init \
  --root "$HOME/mb-r038-test/store" --recipient 'age1SOSTITUIRE_CON_CHIAVE_PUBBLICA'
\`\`\`

La coppia \`store\` + identità age deve essere custodita separatamente. **Una chiave privata persa non è ricostruibile dalla public key.**

## 3. Backup manuale totale e incrementale

Creare \`sources.json\` con un array delle directory realmente esistenti, ad esempio:

\`\`\`json
[
  {"namespace": "documents", "directory": "/home/utente/Documenti"},
  {"namespace": "metadata", "directory": "/home/utente/Metadata"}
]
\`\`\`

### Totale

\`\`\`bash
multi-backup-native storage backup --root STORE --identity AGE_IDENTITY \
  --sources-json sources.json --label "FULL 001" --mode full \
  --top-namespace "Test/Utente"
\`\`\`

Annotare l'ID di 64 caratteri restituito: \`F0\`. Ogni full contiene l'inventario completo dei file e nuovi chunk soltanto per contenuti non già presenti nello store.

### Incrementale

\`\`\`bash
multi-backup-native storage backup --root STORE --identity AGE_IDENTITY \
  --sources-json sources.json --label "INC 001" --mode incremental \
  --parent F0 --top-namespace "Test/Utente"
\`\`\`

Annotare l'ID \`I1\`. Al successivo incremento indicare \`--parent I1\`. Il namespace superiore deve corrispondere a quello della catena. Il manifest incrementale contiene **tutti** i riferimenti dei file per il proprio punto di ripristino, non solo una delta list; i chunk immutati sono condivisi e non duplicati. File cancellati nella sorgente non compaiono nell'inventario del nuovo incremento, ma restano recuperabili dai precedenti punti finché conservati.

La UI **Storage chunk** offre la scelta Totale/Incrementale e usa il backup selezionato come padre. I backup storici Python non sono un prerequisito.

### Backup tramite plugin

\`\`\`bash
multi-backup-native plugins backup --input posix --output posix \
  --source /cartella/sorgente --root STORE --identity AGE_IDENTITY \
  --label "FULL plugin" --namespace files --mode full
\`\`\`

Per un mount Linux *già attivo* e read-only si può usare, solo in prova:

\`\`\`bash
multi-backup-native plugins backup --input nfs --output posix \
  --source /mnt/snapshot-nfs --source-snapshot-attested \
  --root STORE --identity AGE_IDENTITY --label "NFS snapshot" \
  --namespace files --mode full
\`\`\`

Il percorso deve essere l'esatto mountpoint che il kernel riconosce con filesystem atteso; per GPFS CES va aggiunto \`--input-gpfs-access ces-nfs\` oppure \`ces-smb\`. Non viene montato nulla automaticamente, non sono garantiti ACL/xattr, consistenza dei database live o snapshot applicativo.

## 4. Informazioni dei backup e catena

\`\`\`bash
multi-backup-native storage list --root STORE --identity AGE_IDENTITY --json
multi-backup-native storage info --root STORE --identity AGE_IDENTITY --id I1 --json
\`\`\`

\`storage info\` mostra il backup selezionato e la catena ordinata dal full di riferimento a ogni incremento: ID, tipo, numero dei file, dimensione non compressa, dimensione **zstd compressa prima di age**, dimensione compressa **deduplicata per chunk univoci**, nuovi chunk compressi rispetto al predecessore, file aggiunti/modificati e file cancellati. La misura fisica sul disco può essere superiore per overhead di age, manifest, protezione e parità 8+2. **Non sommare le dimensioni deduplicate dei singoli backup per ottenere lo spazio fisico complessivo**: i chunk sono condivisi fra snapshot.

## 5. Verifica manuale o programmata

\`\`\`bash
# Comportamento predefinito: F0 -> I1 -> ... -> ID richiesto
multi-backup-native storage verify-chain --root STORE \
  --identity AGE_IDENTITY --id I1

# Verifica esclusivamente lo snapshot selezionato (non gli antenati)
multi-backup-native storage verify-chain --root STORE \
  --identity AGE_IDENTITY --id I1 --no-chain
\`\`\`

La verifica controlla autenticazione age, HMAC/SHA dei chunk e dati di parità. Una catena corrotta, un parent assente o una verifica non superata NON deve essere dichiarata sana. Effettuare anche prove periodiche di restore su un host e uno storage separati.

## 6. Restore integrale e parziale

\`\`\`bash
# La directory NEW_DIR non deve esistere
multi-backup-native storage restore --root STORE --identity AGE_IDENTITY \
  --id I1 --destination NEW_DIR

# Un singolo file o prefisso di directory presente nel manifest
multi-backup-native storage restore --root STORE --identity AGE_IDENTITY \
  --id I1 --selection "Test/Utente/documents/cartella" \
  --destination ANOTHER_NEW_DIR
\`\`\`

Il ripristino non sovrascrive destinazioni esistenti. Verificare prima il punto di ripristino e scegliere la destinazione con sufficiente spazio. Per un ripristino di più file tramite GUI usare l'elenco di percorsi della schermata Storage.

### Ripristino verso un utente **diverso** (provider con account)

L'identità della **sorgente** resta come provenienza nel backup; l'autorizzazione della **destinazione** è indipendente. Non riutilizzare automaticamente credenziali o secret originali. In r038 le operazioni remote dedicate sono **limitate** a un nuovo repository GitHub o a un nuovo file Dropbox, non a un intero account con metadati e permessi.

\`\`\`bash
# Destinazione: repository nuovo e owner associato alla login gh.
# Il mirror cifrato deve includere il singolo repository richiesto.
multi-backup-native restore github --archive REPO.tar.gz.age \
  --identity AGE_IDENTITY --target altro-owner/nuovo-repo \
  --source-account owner-originale --target-account altro-owner \
  --visibility private --confirm CREATE-NEW

# Destinazione: account associato a un token WRITE diverso da quello READ.
export MULTI_BACKUP_DROPBOX_WRITE_TOKEN='TOKEN_DI_DESTINAZIONE'
multi-backup-native restore dropbox --archive FILE.tar.gz.age \
  --identity AGE_IDENTITY --path /cartella/file-ripristinato.ext \
  --source-account account-origine --target-account ACCOUNT_ID_DESTINAZIONE \
  --confirm CREATE-NEW
unset MULTI_BACKUP_DROPBOX_WRITE_TOKEN
\`\`\`

GitHub richiede \`gh auth status\` per il nuovo owner, autorizzazione a creare repository, \`git\` e \`git-lfs\`; rifiuta un repository già esistente. Dropbox richiede che il token di scrittura corrisponda all'account esplicitamente indicato, verificato tramite API; rifiuta la sovrascrittura e verifica l'hash del file. Le altre destinazioni account (Drive, OneDrive, Nextcloud ecc.) hanno solo contratti di plugin, **non** workflow remoti completi. La selezione arbitraria di più repository/file verso utenti diversi resta una funzione da ultimare.

## 7. Cancellazione: rebase del successore e purge logico

Esempio: \`FULL F0 → INC I1 → INC I2\`.

- **Eliminare I1:** I2 mantiene il proprio inventario completo e viene collegato a F0.
- **Eliminare F0:** I1 diventa FULL autonomo; I2 resta il successivo incremento.
- **Eliminare l'ultimo incremento:** non sono necessarie modifiche ai precedenti.

\`\`\`bash
# L'ID completo deve comparire due volte intenzionalmente.
multi-backup-native storage delete-backup --root STORE --identity AGE_IDENTITY \
  --id ID_DA_ELIMINARE --confirm-id ID_DA_ELIMINARE
\`\`\`

Per sicurezza l'operazione rifiuta backup **protetti**, figli protetti o dipendenze non verificabili. Per poter riorganizzare un figlio protetto, la rimozione della sua protezione deve essere una scelta **separata ed esplicita**:

\`\`\`bash
multi-backup-native storage unprotect-backup --root STORE \
  --identity AGE_IDENTITY --id ID_FIGLIO --confirm-id ID_FIGLIO
\`\`\`

Se una sequenza di più manifest viene interrotta non si devono cancellare i dati fisici: controllare tutte le catene e ripetere/riconciliare dopo ispezione. La riorganizzazione dei manifest è un percorso di sviluppo e non ha ancora una transazione multi-manifest certificata contro power loss. Un purge rimuove il **manifest** del backup selezionato, non cancella chunk/parità e non recupera immediatamente spazio.

## 8. Garbage collection: recuperare spazio senza perdere dati

Prima di eseguire la GC, completare un backup e un restore di prova, verificare la catena e conservare una copia indipendente di recovery.

\`\`\`bash
multi-backup-native storage gc-preview --root STORE \
  --identity AGE_IDENTITY --grace-hours 48 --json

# Esecuzione fisica DOPO aver esaminato il report, con conferma distinta
multi-backup-native storage gc-collect --root STORE \
  --identity AGE_IDENTITY --grace-hours 48 \
  --confirm ERASE-ORPHANS --json
\`\`\`

Il periodo di sicurezza non può essere inferiore a **24 ore** (default **48 ore**). La GC verifica e scandisce tutti i backup ancora live, controlla manifest/catene/parità e rimuove soltanto i ciphertext orfani idonei e le stripe totalmente obsolete. Se una stripe 8+2 contiene **anche un solo chunk ancora utile**, trattiene la stripe intera e i chunk obsoleti associati. **Il ripack crash-safe delle stripe miste non è ancora disponibile**; quindi la GC può liberare meno spazio del teorico. Il report distingue \`reclaimable_bytes\` dalle dimensioni non compressa/compressa. Nessun GC remoto e nessuna cancellazione automatica basata unicamente sulla retention sono qualificati.

## 9. Esecuzione manuale e scheduler Rust

Esempio \`jobs.json\`: un job incrementale richiede un parent FULL valido alla prima esecuzione. Il runner mantiene in stato \`last_backup_id\` per le successive esecuzioni **del medesimo job**. Un job di verifica distinto deve indicare \`backup_id\` esplicitamente: non eredita automaticamente l'ID prodotto da un altro job.

\`\`\`json
{
  "version": 1,
  "jobs": [
    {
      "id": "documents-incremental",
      "enabled": true,
      "interval_seconds": 86400,
      "action": "backup",
      "backup_mode": "incremental",
      "root": "/backup/store",
      "identity": "/private/age-identity.txt",
      "source": "/data/documents",
      "input_plugin": "posix",
      "output_plugin": "posix",
      "namespace": "documents",
      "top_namespace": "Work",
      "label": "Daily documents",
      "parent_id": "ID_DI_UN_FULL_GIA_VERIFICATO"
    },
    {
      "id": "verify-reference",
      "enabled": true,
      "interval_seconds": 604800,
      "action": "verify",
      "root": "/backup/store",
      "identity": "/private/age-identity.txt",
      "backup_id": "ID_DA_VERIFICARE",
      "verify_full_chain": true
    }
  ]
}
\`\`\`

\`\`\`bash
# Un solo controllo, in modo regolare; --force per eseguire subito
multi-backup-native schedule run --config jobs.json \
  --state /private/scheduler-state.json
multi-backup-native schedule run --config jobs.json \
  --state /private/scheduler-state.json --force

# Processo persistente di polling, da supervisionare esternamente
multi-backup-native schedule daemon --config jobs.json \
  --state /private/scheduler-state.json --poll-seconds 60
\`\`\`

Conservare \`jobs.json\` e il file di stato con permessi privati; non inserire token/chiavi all'interno della configurazione. Il runner rifiuta ID duplicati e intervalli inferiori a 60 secondi, impedisce due cicli concorrenti con un lock e non avanza un job che fallisce. Le scadenze sono intervalli in secondi, **non** un calendario cron completo. Il daemon non è ancora distribuito come servizio OS certificato, la UI di configurazione non è completa e notifiche/logging multiutente restano da implementare.

## 10. Controlli prima della RC

1. CI dell'**ultimo commit** della PR #10 verde su Production Gate, Rust GUI, test crittografici e build native.
2. Ripristino byte-identico di FULL, INC e selezioni, anche dopo purge/rebase e GC in ambiente isolato; test di interruzione dell'alimentazione e journal di commit.
3. Test reale GitHub e Dropbox fra account diversi, con credenziali least-privilege, LFS/metadata/permessi, verifica dei risultati e restore parziale.
4. Conservazione e recupero affidabile delle chiavi age; verifiche di WORM e legal hold, GC remoto e ricompattazione delle stripe di parità.
5. Installer macOS/Windows/Linux realmente avviati, firmati/notarizzati dove richiesto, upgrade/rollback sicuri e controlli di sicurezza indipendenti.

**Questa guida descrive le funzionalità del branch r038 e i loro limiti; non costituisce evidenza di certificazione della RC.**
