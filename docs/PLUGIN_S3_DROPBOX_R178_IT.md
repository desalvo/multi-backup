# Multi Backup r178 — plugin S3 B-In e Dropbox B-Out (16 novembre 2026)

## Scope effettivo (Preview, NON Ready)

**S3/Ceph RGW/RustFS B-In:** il comando Rust `plugins s3-object-backup` legge
esattamente **una versione S3 identificata dal chiamante**, verifica la
`x-amz-version-id`, la lunghezza e l'hash **SHA-256 fornito da un inventario
indipendente**, e inserisce l'oggetto in `Store::backup_stream`, con chunk
cifrati age e manifest finale solo dopo un download integrale verificato.
Non salva mai l'intero file sorgente in chiaro nel disco locale. Un successivo
`storage restore` ripristina i file cifrati in una directory nuova.

Config JSON S3 riusa `s3_vault::Config` per credenziali AWS via variabili
`AWS_ACCESS_KEY_ID` e `AWS_SECRET_ACCESS_KEY` (eventuale `AWS_SESSION_TOKEN`),
TLS autenticato (HTTP consentito solo su loopback nei test) e nome bucket,
regione, prefisso. Per B-In **non** richiede che il bucket sia WORM; richiede
invece **version ID immutabile** e SHA del payload, e non utilizza un GET
non versionato. Nessuna azione di cancellazione o overwrite S3.

Esempio dopo `storage init --root /safe/store --recipient age1...`:

```sh
multi-backup-native plugins s3-object-backup \
  --config /private/s3.json --object-key datasets/2026-11-16.bin \
  --version-id 'VERSION-ID-DA-HEAD-OBJECT' --sha256 'SHA256-DEL-PAYLOAD' \
  --root /safe/store --identity /private/age-identity \
  --label 'S3 snapshot verificato' --json
```

**Dropbox B-Out:** `plugins dropbox-backup` cifra una directory localmente
con age e invia unicamente `.tar.gz.age`; `plugins dropbox-upload` invia
un archivio age già presente con **sidecar SHA-256 obbligatorio**. Usa API
Dropbox ufficiali `upload_session/start`, `append_v2` e `finish`,
blocchi 4 MiB, modalità `add`, `autorename:false`,
`strict_conflict:true`: un path esistente causa errore, non sovrascrittura.
Il plugin rifiuta percorsi remoti traversabili, dati senza cifratura verificata
e tentativi di upload ambigui senza riconciliazione operatore.
Una nuova chiamata autenticata `files/get_metadata` per `id` confronta
`rev`, path, `size` e Dropbox `content_hash` con gli hash locali.
Le credenziali restano in `MULTI_BACKUP_DROPBOX_TOKEN`, mai in argomenti/log.
Servono scope `files.content.write` e `files.metadata.read`.

```sh
multi-backup-native plugins dropbox-backup \
  --source /data/snapshot --recipient age1... \
  --remote-path '/backups/snapshot-20261116.tar.gz.age' --json

multi-backup-native plugins dropbox-upload \
  --archive /archives/verified.tar.gz.age \
  --remote-path '/backups/verified-20261116.tar.gz.age' --json
```

## Evidenze e limitazioni

- Test S3 locali: validazione ID e SHA, byte troncati/eccessivi; suite
  RustFS CI `s3-worm-rustfs-live.sh` estesa con oggetto reale avente due
  versioni, importazione della versione precedente, restore identico,
  rifiuto SHA errato. La CI su SHA r178 deve ancora finire prima del PASS.
- Test Dropbox locali: modalità create-only obbligatoria, path non valido,
  ricevute tamperate, archivio senza sidecar rifiutato **prima della rete**.
- Test Dropbox reale **separato e manuale** sul solo account di test approvato:
  `.github/workflows/live-github-dropbox-r057.yml` con
  `provider=dropbox-output`, SHA esatto e conferma
  `ISOLATED-GITHUB-DROPBOX-TEST-ACCOUNTS`. Legge il pinned test account ID,
  crea un solo archivio cifrato nuovo, scarica, verifica SHA256 / Dropbox hash
  e autentica il restore con age. Non cancella dati sul provider. Con token
  assente il workflow deve rifiutare il collaudo; **non** dichiarare PASS.
- Non sono ancora coperti: listing/batch multi-oggetto S3, policy IAM/STS,
  snapshot S3 non versionato, Ceph RGW reale per questa nuova strada B-In,
  resume multipart, importazione metadati cloud/ACL, restore diretto
  verso S3 o Dropbox, OAuth refresh/rotazione completa, fault-tolerance
  dopo timeout commit, fatturazione/quota per grandi backup e field
  Dropbox B-Out non ancora eseguito.
- Registry: `s3` B-In da `planned` a `preview` **senza falsificare
  R-Out** (resta `planned`); `dropbox` B-Out da `planned` a
  `preview`, R-In deriva da B-Out ma non certifica il restore remoto.
  I due comandi sono dedicati: non promuovere il generic dispatcher
  e non attribuire `Stage::Ready`.

**One-line English commit summary:** Add version-pinned S3 object input into encrypted Rust chunks and create-only Dropbox ciphertext upload with independent metadata verification and live-test gates.
