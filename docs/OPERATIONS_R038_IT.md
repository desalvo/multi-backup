# Multi Backup 0.1.0-r038 — Manuale operativo Rust (anteprima)

**Aggiornato:** 14 novembre 2026. **Ambito:** CLI e interfaccia Rust, PR #10 in sviluppo. **NON è una RC certificata né una release stabile.**

Questo manuale descrive le funzionalità effettivamente implementate, quelle di anteprima non certificate e quelle ancora da realizzare. È la **fonte operativa corrente** per la r038; le vecchie schermate e istruzioni Python/Tk della r018 rimangono documentazione storica. Non è richiesto convertire i precedenti archivi Python: nessuna procedura li cancella automaticamente. Conservare sempre una copia esterna della chiave privata age.

## 1. Prerequisiti e sicurezza

- Utilizzare il **binario compilato** `multi-backup-native`, presente nei pacchetti Rust di anteprima. Nessun comando presente qui richiede Python.
- Installare/avere nel PATH `age` e `age-keygen`; utilizzare solo dispositivi e cartelle private con spazio sufficiente.
- Non conservare mai la chiave privata dentro il backup o la cartella sorgente. Non inserire token/password nelle opzioni CLI, negli script o nella documentazione.
- Alcune operazioni di mantenimento richiedono conferma esplicita; una procedura sicura parte sempre da **verifica e anteprima**.
- Eseguire prima una prova di restore completa verso una cartella nuova: un esito di CI verde non equivale a un backup remoto qualificato.
- Lo storage v1 è sperimentale. La ridondanza 8+2 non sostituisce copie indipendenti, disconnessione dalla rete o WORM reale.

## 2. Preparare storage e identità

```bash
age-keygen -o /private/keys/multi-backup-age.txt
# Annotare il recipient pubblico age1... mostrato da age-keygen.
multi-backup-native storage init --root /private/backup-store \
  --recipient age1SOSTITUIRE_CON_LA_CHIAVE_PUBBLICA --chunk-mib 4
```

Lo storage crea `.multi-backup-v1/config.json.age`, `manifests/`, `chunks/`, `stripes/` e `parity/` quando necessari. I chunk sono identificati tramite HMAC-SHA256, compressi con zstd e cifrati con age. **Non cancellare manualmente oggetti all'interno dello storage.**

## 3. Backup totale e incrementale

Creare un file `sources.json` con le sorgenti effettive (esempio):

```json
[
  {"namespace":"documents","directory":"/data/documents"},
  {"namespace":"projects","directory":"/data/projects"}
]
```

```bash
# Backup totale: ottenere FULL_ID dal JSON restituito.
multi-backup-native storage backup --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt \
  --sources-json sources.json --label "Totale novembre" \
  --mode full

# Backup incrementale: indicare sempre il precedente FULL o INCREMENTALE.
multi-backup-native storage backup --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt \
  --sources-json sources.json --label "Incrementale novembre" \
  --mode incremental --parent FULL_ID
```

Un incrementale conserva l'**inventario completo dei file** in un manifest age, ma condivide i chunk immutabili già esistenti. La catena `parent_id` identifica il precedente e risale al totale originario. Il ripristino di un singolo punto usa il suo inventario, senza copiare fisicamente tutta la catena.

## 4. Informazioni e dimensioni

```bash
multi-backup-native storage list --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --json
multi-backup-native storage info --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --id BACKUP_ID --json
```

La scheda `info` restituisce per il punto selezionato e per ogni predecessore:
numero file, byte non compressi, somma delle dimensioni **zstd prima della deduplica**, dimensione dei chunk univoci **zstd dopo deduplica**, byte compressi dei chunk assenti dal predecessore, file aggiunti/modificati e rimossi. Le dimensioni zstd **escludono l'overhead di age, manifest e parità**; non rappresentano quindi lo spazio fisico liberabile da GC. Il rapporto fornisce gli ID di tutta la catena e il totale di riferimento.

## 5. Verifica e ripristino

```bash
# Predefinito: verifica TUTTA la catena fino al totale.
multi-backup-native storage verify-chain --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --id INCREMENT_ID

# Opzione: verifica solo il punto selezionato, mantenendo il controllo del link.
multi-backup-native storage verify-chain --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --id INCREMENT_ID --no-chain

# Restore completo: directory di destinazione ancora INESISTENTE.
multi-backup-native storage restore --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --id INCREMENT_ID \
  --destination /restores/novembre

# Restore selettivo di un percorso logico.
multi-backup-native storage restore --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --id INCREMENT_ID \
  --selection documents/fatture --destination /restores/solo-fatture
```

La verifica controlla age, SHA-256/HMAC, riferimenti dei chunk e parità. Il restore non sovrascrive destinazioni esistenti, evita link simbolici e nomi di percorso non sicuri. Eseguire un confronto byte-per-byte dei file recuperati prima di eliminare il backup originale.

## 6. Eliminazione sicura di full e incrementali

**Purge** = modifica i manifest, non rimuove chunk o parità. L'azione richiede ID completo e conferma.

```bash
multi-backup-native storage delete-backup --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt \
  --id BACKUP_DA_ELIMINARE --confirm-id BACKUP_DA_ELIMINARE
```

- Eliminando `FULL F0 -> INC I1 -> INC I2` il punto I1, I2 viene ricollegato a F0.
- Eliminando F0, il successore diretto I1 viene **promosso a FULL**; gli altri punti mantengono lo stato completo senza duplicare i chunk.
- Un backup **protetto** non può essere eliminato. Se un successore protetto dovrebbe avere il manifest riscritto, l'operazione è bloccata finché il proprietario non toglie esplicitamente la protezione.
- In presenza di link danneggiati, verifica fallita o identità errata, **non forzare** la cancellazione: eseguire verifica e recupero. La modifica dei manifest richiede ancora certificazione contro crash/guasti simultanei.

## 7. Garbage collection e repack automatico 8+2

La **GC è separata dal purge**. Il repack viene eseguito automaticamente durante `gc-collect`: se una stripe Reed-Solomon 8+2 contiene chunk attivi e orfani, crea una stripe sostitutiva con **solo gli attivi**. Verifica i due nuovi shard age cifrati e pubblica **per ultimo** l'indice cifrato di sostituzione. Non cancella la vecchia stripe nello stesso istante.

```bash
# Vedere i chunk orfani, le stripe miste e i byte eventualmente recuperabili.
multi-backup-native storage gc-preview --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt --grace-hours 48

# Dopo aver letto l'anteprima, avviare GC + repack (operazione distruttiva sui soli orfani).
multi-backup-native storage gc-collect --root /private/backup-store \
  --identity /private/keys/multi-backup-age.txt \
  --confirm ERASE-ORPHANS --grace-hours 48
```

**Prima passata:** produce/attiva la nuova parità e mantiene quella precedente come protezione da crash. **Passate successive, oltre il periodo di sicurezza:** solo dopo la maturazione dell'indice sostitutivo rimuovono la vecchia parità e i chunk non più usati. Il grace period predefinito è 48 ore, minimo 24. Una pulizia dopo un'interruzione può recuperare cartelle di parità preparate ma mai confermate, solo se abbastanza vecchie e prive di indice attivo. La funzione è codice r038 in verifica CI: **non considerarla un GC di produzione finché non supera i test con interruzione, alimentazione simulata, carichi reali e multi-OS**.

**Importante:** l'anteprima non è un'azione distruttiva, non effettua repack e il numero di byte immediatamente recuperabili può differire dal recupero totale ottenibile dopo le successive passate. Non cancellare manualmente le directory `parity/` e `stripes/`.

## 8. Schedule di backup e verifica

La CLI Rust contiene `schedule run` (esecuzione manuale o da cron/launchd/Utilità di pianificazione) e `schedule daemon` (polling nel processo). Un job sceglie `action: "backup"` con `backup_mode: "full"|"incremental"`, oppure `action: "verify"` con `verify_full_chain: true` per default.

```json
{
  "version": 1,
  "jobs": [
    {
      "id": "nightly-full", "enabled": true, "interval_seconds": 86400,
      "action": "backup", "backup_mode": "full",
      "root": "/private/backup-store",
      "identity": "/private/keys/multi-backup-age.txt",
      "source": "/data/documents",
      "input_plugin": "posix", "output_plugin": "posix",
      "namespace": "documents", "label": "Totale programmato"
    },
    {
      "id": "audit", "enabled": true, "interval_seconds": 604800,
      "action": "verify", "root": "/private/backup-store",
      "identity": "/private/keys/multi-backup-age.txt",
      "backup_id": "SOSTITUIRE_CON_ID_REALE", "verify_full_chain": true
    }
  ]
}
```

```bash
multi-backup-native schedule run --config /private/jobs.json \
  --state /private/schedule-state.json --force
multi-backup-native schedule daemon --config /private/jobs.json \
  --state /private/schedule-state.json --poll-seconds 60
```

Lo stato è persistito in file con restrizioni su Unix; job con errore non avanzano la data di successo. **Limite attuale:** un job di verifica deve avere un ID noto, e le schedule configurate da GUI non sono ancora persistite/certificate. L'azione GC è ancora manuale, non schedulata. Non avviare più daemon simultanei senza verificare i lock.

## 9. Plugin e restore verso un altro utente

Sono registrati 32 plugin; **cinque coppie di backup** hanno una via operativa Rust in anteprima: POSIX→POSIX, e su Linux NFS/SMB/SSHFS/GPFS CES montati *read-only*→POSIX. Tutte le altre coppie non certificate falliscono senza tentare un backup generico.

Per sorgenti NFS, SMB, SSHFS o GPFS già montate il comando richiede `--source-snapshot-attested` e un filesystem read-only riconosciuto; **il flag non certifica uno snapshot backend**. GPFS CES richiede `--input-gpfs-access ces-nfs|ces-smb`.

Il **restore cross-account** distingue provenienza e destinazione: l'account originale non autorizza la scrittura. Per GitHub selezionare un repository nuovo `owner/nuovo-repo` e autenticare `gh` con un account autorizzato. Per Dropbox fornire un **token di destinazione** tramite `MULTI_BACKUP_DROPBOX_WRITE_TOKEN`; con `--target-account` viene verificato l'account del token prima dell'upload.

```bash
multi-backup-native restore github --archive /backup/repo.tar.gz.age \
  --identity /private/keys/multi-backup-age.txt \
  --source-account oldowner --target-account newowner \
  --target newowner/nuovo-repo --visibility private \
  --confirm CREATE-NEW

MULTI_BACKUP_DROPBOX_WRITE_TOKEN=TOKEN_DESTINAZIONE \
multi-backup-native restore dropbox --archive /backup/file.tar.gz.age \
  --identity /private/keys/multi-backup-age.txt \
  --source-account old@example.com --target-account new@example.com \
  --path /cartella/nuovo-file.ext --confirm CREATE-NEW
```

L'esempio di variabile d'ambiente è illustrativo: evitare di inserire token nella history della shell, preferire secret manager/OS keychain e processi che esportano il segreto temporaneamente. **La copia fra account e il restore remoto parziale per qualunque plugin NON sono ancora certificati**. Il restore GitHub attuale crea un repo completo da un mirror, Dropbox carica un file per volta; metadati di account, permessi, integrazioni e segreti non vengono trasferiti implicitamente.

## 10. Stato, errori e criteri per la RC

- **Implementato in Rust preview:** CLI/GUI compilate, chunk age+zstd/HMAC, stripe 8+2, full/incrementale, restore locale selettivo, statistiche, chain verify, purge con rebase, GC separata con repack automatico, scheduler locale, pianificazione cross-account e alcuni percorsi remoti dedicati.
- **Da certificare:** test end-to-end con GitHub/Dropbox veri, completezza di metadata/ACL, restore tra utenti, transazioni di rebase e GC dopo crash, repack di più generazioni, GUI di pianificazione, correttezza degli installer, update/rollback, firma/notarizzazione, sistemi WORM e audit di sicurezza indipendente.
- **Non dichiarato pronto:** 682 possibili coppie di plugin, backup cloud-to-cloud universale, provider database/VM/SAN, GC remota, WORM reale.
- **CI di questa PR:** controllare l'ultimo SHA della PR #10 e le prove di ripristino prima di un merge. Non usare il verde della vecchia PR #9 come prova delle modifiche r038.

Per i dettagli ingegneristici e i gate: [CHUNK_STORAGE_V1.md](CHUNK_STORAGE_V1.md), [MAINTENANCE_POLICY.md](MAINTENANCE_POLICY.md), [PLUGIN_MATRIX.md](PLUGIN_MATRIX.md), [RC_CERTIFICATION_MATRIX.md](RC_CERTIFICATION_MATRIX.md) e [DEVELOPMENT_STATUS.md](DEVELOPMENT_STATUS.md).
