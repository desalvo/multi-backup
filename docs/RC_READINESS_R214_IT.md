# Multi Backup r214 — READY circoscritto e gate preliminare RC

Data: 17 novembre 2026. Ramo feature/r214-ready-posix-rc-gate sopra r213/r212/r211. Nessun merge, tag o release.

## READY: 2 plugin su 33, solo per funzioni esplicitamente circoscritte

| Plugin | Ruoli READY | Funzioni qualificate | Non certificato |
|---|---|---|---|
| POSIX | B-In, B-Out e ripristino corrispondente | Contenuto di file regolari su albero fermo, annidamento, file vuoti, age, verifica chunk, correzione parità, restore in directory NUOVA; collisioni, chiave errata e symlink rifiutati | ACL, xattr, UID/GID, permessi, timestamp, hardlink/symlink, directory vuote, consistenza applicativa live e HA fisica |
| PostgreSQL 16 | B-In e restore dedicato in database NUOVO | Un singolo database, dump logico FULL, crittografia age, verifica, pg_restore, confronto delle righe, rifiuto credenziali e target errati | Ruoli/globali, ACL/owner, PITR integrato con Patroni, DDL concorrenti, HA fisica e upgrade cross-versione |

Ogni descriptor READY ha un campo machine-readable ready_scope: nessun'altra capability è contrabbandata come generale. Il planner non rende executable alcuna coppia di provider senza un dispatcher compilato realmente implementato. Solo POSIX→POSIX è genericamente executable nel perimetro indicato. PostgreSQL usa i suoi comandi CLI dedicati.

Non sono dichiarati READY, tra gli altri, MySQL/MariaDB misti (GTID/PITR completi mancanti), Oracle intero (RAC/ASM/TDE), RBD (guest quiesce/fencing), iSCSI SAN (CHAP/multipath), CephFS (fault fisici), VMware VADP/CBT e oVirt full VM.

## Full RC preflight sul medesimo SHA

- .github/workflows/posix-ready-r214.yml: backup e restore CLI reali Rust age + prove negative; catalogo verifica i due scope READY.
- .github/workflows/database-live-integration.yml: PostgreSQL 16 e MariaDB 11.4 vere in container, backup age e restore su database nuovi con confronto riga per riga, utenze errate e sovrascritture negate; richiamabile con workflow_call.
- .github/workflows/rc-preflight-r214.yml: avvia in parallelo Rust core/server/desktop, PostgreSQL agent/HA, GUI Linux Xvfb, OCI server e Kubernetes kind a tre nodi, SQL reali, POSIX Ready, e test negativi di policy.
- scripts/ci/rc-preflight-r214.py e tests/test_rc_preflight_r214.py: blocco fail-closed su source SHA, ruoli/contratti READY, 33 plugin, evidenza POSIX backup e restore.
- Soltanto se tutti i job sono SUCCESS, compila CLI e agent Rust nativi Linux dallo SHA esatto, genera SHA256SUMS e un manifest dei limiti, carica un artifact CI Linux pre-RC. Il workflow NON pubblica release, tag o canale stable.
- Non riutilizzare il vecchio publisher r174/r175: era vincolato allo SHA 1f1a726a... e ai vecchi artifact; non sono binari della r214.

## Cosa manca alla RC completa

1. Build multipiattaforma sullo stesso SHA e gate production completo, incluse immagini native macOS ARM64/Intel, installer Windows, pacchetti Debian e RPM con installazione/avvio effettivi, sicurezza, checksum/provenienza e gestione delle chiavi agli aggiornamenti.
2. Chiusura dei job rossi del gate cumulativo e realizzazione di un publisher RC idempotente: soltanto prerelease, senza produrre stable, mai sovrascrivere asset esistenti.
3. Le certificazioni hardware dei provider restano specifiche: r213 ha predisposto il laboratorio oVirt/ESXi ma non ha eseguito il provisioning reale. Nessuna certificazione VMware VADP/CBT o full oVirt senza hypervisor autentici. HA fisico su 3 host ancora non effettuato.

**Collegamenti:** PR r213 https://github.com/desalvo/multi-backup/pull/50 ; oVirt HTTPS protocol PASS https://github.com/desalvo/multi-backup/actions/runs/44134574398 ; VMware simulator PASS https://github.com/desalvo/multi-backup/actions/runs/44095177198 ; RBD genuino https://github.com/desalvo/multi-backup/actions/runs/44000026264 ; Oracle Free fisico PDB/CDB/PITR https://github.com/desalvo/multi-backup/actions/runs/44083604906 .

One-line English commit summary: Qualify limited POSIX file-content and PostgreSQL 16 logical restore as Ready and enforce same-SHA nonpublishing RC preflight.
