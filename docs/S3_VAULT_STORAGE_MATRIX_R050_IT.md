# Multi Backup r050 — Matrice di compatibilità vault WORM S3

Data: 16 novembre 2026. **Stato della certificazione sempre determinato
dall'ultimo collaudo realmente eseguito, non dalla sola presenza delle API.**

## Matrice

| Backend | Config JSON `provider` | Object Lock COMPLIANCE secondo vendor | Versioni S3 | Backup/verify/restore Rust | CI con server reale | Qualifica Multi Backup |
|---|---|---|---|---|---|---|
| MinIO S3 | `minio` | Sì | Sì | Sì | **PASS** (run 43813760616 e full CI 43813760546) | Profilo MinIO mono-server collaudato |
| Ceph RGW S3 | ceph-rgw | Dipendente da release/config e prova reale | Sì | Motore S3 Rust predisposto | **FAIL** [43846329965](https://github.com/desalvo/multi-backup/actions/runs/43846329965): mismatch pod OSD/worker/dispositivo; prova successiva [43848726571](https://github.com/desalvo/multi-backup/actions/runs/43848726571) avviata | NON certificato fino a PASS reale con 3 host CRUSH, Object Lock, verify/restore |
| RustFS S3 | rustfs | Sì nel profilo RustFS 1.0.3 verificato | Sì | Backup age, verify e restore nativi | **PASS REAL** [43819587169](https://github.com/desalvo/multi-backup/actions/runs/43819587169), [43837819453](https://github.com/desalvo/multi-backup/actions/runs/43837819453), [43844964347](https://github.com/desalvo/multi-backup/actions/runs/43844964347) | Server singolo S3 Object Lock COMPLIANCE collaudato; HA/root threat NON certificati |
| Vault Linux locale | n/d | WORM non basato su S3 | n/d | r046 append-only locale | Test separati | Non equiparabile a Object Lock remoto |

Il motore Rust applica **identici requisiti fail-closed** a MinIO, Ceph e
RustFS: endpoint HTTPS (HTTP solo su loopback e flag di test esplicita),
versioning enabled, Object Lock enabled con default COMPLIANCE, versionId
dopo ogni PUT, GetObjectRetention per la versione creata,
retain-until minimo richiesto, archivio age verificato e restore in
directory nuova. Non implementare un fallback 'soft WORM' se il server
non rispetta queste condizioni.

## Test reale RustFS (r050)

File: `scripts/ci/s3-worm-rustfs-live.sh` e
`.github/workflows/s3-rustfs-live-r050.yml`.

Immagine ufficiale `docker.io/rustfs/rustfs:1.0.3`, con SHA
dell'immagine riportato nel summary GitHub Actions. Il runner
GitHub-hosted avvia RustFS su loopback 127.0.0.1:9000 usando un volume
tmpfs volatile e chiavi S3 casuali generate per il job. La CLI Rust di
Multi Backup non sa né deve accedere a credenziali amministrative
esterne o ad altri bucket reali.

Sono **obbligatori**:
1. Avvio server RustFS effettivo e autenticazione S3 riuscita.
2. Creazione di un bucket Object Lock con versioning Enabled e retention
   COMPLIANCE di default; bucket non WORM rifiutato dalla CLI Rust.
3. Dimostrazione che la medesima identità può cancellare un oggetto
   ordinario (controllo endpoint e permessi).
4. Backup age cifrato, creazione di oggetto+manifest versionati con
   retention COMPLIANCE verificata per entrambe le versioni.
5. Verify con age corretta, verify con age errata respinto; restore
   byte-identico e rifiuto di sovrascrittura.
6. Tentativo reale di DELETE della versione protetta: rifiutato con
   motivo connesso all'Object Lock, non con errore generico.
7. Tentativo di ridurre la scadenza COMPLIANCE: rifiutato; controllo
   della scadenza invariata e verifica finale del backup.

**FAIL** anche con server non avviato, endpoint irraggiungibile,
capability non disponibile, test saltato o errore di tipo non valido.
Il fatto che RustFS dichiari Object Lock non equivale a certificazione
di Multi Backup prima del PASS.

## Scope e limiti

Il test di RustFS è su server reale **single-node**. Non certifica
disponibilità distribuita, recovery da guasti di dischi indipendenti,
resistenza a un root/cluster-admin che distrugga fisicamente i dati,
cifratura sul server, replication, HA di Multi Backup, multipart/resume,
catalogo autenticato indipendente o recupero da delete-marker.

Il test Ceph in
`.github/workflows/ceph-rgw-s3-real-r049.yml` usa un cluster RADOS
multi-worker kind con tre OSD/MON **virtuali**, non tre host fisici.
I suoi risultati non possono essere inferiti dal PASS RustFS o MinIO.

Riferimenti RustFS:
- https://rustfs.org/features/worm/
- https://docs.rustfs.com/en/installation/container
- https://github.com/rustfs/rustfs/releases
- https://docs.rustfs.com/en/reference/s3-compatibility
