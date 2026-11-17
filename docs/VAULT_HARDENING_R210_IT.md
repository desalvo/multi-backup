# Multi Backup r210 — hardening strutturale Vault WORM
Data: 17 novembre 2026. Branch: `feature/r210-vault-credential-isolation-ovirt`. Non è una release RC certificata.

## Authority e credenziali separate
**Host A, backup writer:** utente non-root, accesso esclusivo alla chiave pubblica age e alle variabili `MB_WORM_WRITER_ACCESS_KEY_ID` / `MB_WORM_WRITER_SECRET_ACCESS_KEY` (facoltativa `MB_WORM_WRITER_SESSION_TOKEN`). `vault-worm backup --config ... --source ...` NON accetta più la chiave privata age. Il ruolo può scrivere solo snapshot e manifest cifrati nella propria prefix, consultare retention e leggere il manifest con VersionId esplicita. Non deve possedere credenziali root/admin del Vault, privilegi delete o leggere i dati cifrati.

**Host B, Vault S3:** istanza di RustFS, Ceph RGW o S3 con Object Lock COMPLIANCE e versioning, TLS attendibile, root/amministrazione separata dall'host A. Confinare la rete di A all'endpoint dati S3; vietare accesso a console, SSH, admin API e BMC di B. La protezione contro root su A richiede policy S3 realmente applicate su B; il solo filtro Rust non basta. Non installare root/owner credentials di B su A o C.

**Host C, recovery:** macchina/identità amministrata separatamente con `MB_WORM_RECOVERY_ACCESS_KEY_ID` / `MB_WORM_RECOVERY_SECRET_ACCESS_KEY` (facoltativa `MB_WORM_RECOVERY_SESSION_TOKEN`), chiave PRIVATA age custodita fuori da A e ricevuta escrow indipendente. Può leggere soltanto versioni S3 esplicite e retention, mai scrivere/eliminare. `vault-worm verify --config ... --id ID --manifest-version VERSION --identity private.age` e `restore` su NEW directory. Il writer deve replicare il receipt e il VersionId in archivio indipendente.

**Runtime:** il client Rust rifiuta credenziali ambientali `AWS_*` e l'altro ruolo mentre esegue WORM. Un unico processo non può essere writer e recovery; server e agenti devono lanciare processi/worker isolati con privilegi distinti. Creare due utenti IAM **differenti**, NON usare root/owner o un account admin condiviso. Template documentati in `docs/policies/worm-{writer,recovery}-iam.example.json`: sostituire `__BUCKET__` / `__PREFIX__`, verificare policy reali e permessi effettivi. Non assumere compatibilità perfetta tra provider S3.

## Collaudi obbligatori prima di dichiarare il Vault anti-ransomware READY
1. Con writer attivo su A verificare upload cifrato COMPLIANCE, versioning, retention e receipt, **senza** chiave privata age. Provare con le credenziali writer REALI a scaricare una versione cifrata, creare un delete marker (DELETE senza VersionId), cancellare una versione bloccata, listare le versioni e cambiare bucket policy: **tutte le richieste devono essere AccessDenied dal provider**, non solo dal client.
2. Trasferire il receipt in escrow esterno; su C con credenziali recovery e chiave age privata eseguire verifica/restore, controllare SHA-256 e confronto byte-for-byte dei dati; provare wrong-key, digest errato, directory esistente, delete-marker avversario e VersionId sconosciuta.
3. Con credenziali recovery provare PutObject e DeleteObject: **AccessDenied provider-side**. Controllare che recovery non veda credenziali writer e viceversa. Registrare policy IAM effettive, request IDs, version IDs, TLS/CA, UID, machine-id di A/B/C diversi e SHA testato senza esportare segreti.
4. Simulare perdita/rebuild di A; restore su nuova C, con B intatto; successivamente provare fault di B solo su un secondo backend/replica indipendente. Nessun claim di protezione contro root sul medesimo host B se root può fermare/cancellare tutti i nodi e dati.
5. Non considerare un receipt writer come recuperabilità verificata: `recovery_verified=false`; solo verifica indipendente su C lo porta a true. `remote_iam_separation_certified=false` finché non esistono evidenze IAM reali.

## Evidenze e limiti (r210)
- CI RustFS reale #44079837799: PASS backup/retention/restore e controlli negativi applicativi; usa un singolo **account admin del laboratorio** per entrambe le invocazioni, quindi NON certifica la separazione IAM esterna.
- CI Rust #44079974782: PASS; CI oVirt hosted #44080039584: PASS per contratti/guardie, **Engine genuino non eseguito**.
- Il client oVirt richiede ora membership del disco nel backup VM, image ID, identità backup/transfer e formato attestato, e rifiuta incrementali senza base. Non ha ancora il restore diretto VM in Engine, tutti i dischi/OVF/boot e provider E2E autentico.
- Resto dei 33 provider: Google Drive, OneDrive, HDFS, Alfresco, FC/NVMe-oF, Kubernetes/CSI, VMware e restore completo PVE/PBS/oVirt ancora parziali o non field-certified. GPFS CES via NFS/SMB non equivale a GPFS nativo. HA su tre host fisici distinta dalla CI hosted. Nessun merge su main, nessuna RC pronta.

One-line English commit summary: Enforce isolated WORM writer/recovery authority and keyless backup, harden oVirt evidence, and add fail-closed IAM policies and CI qualification.
