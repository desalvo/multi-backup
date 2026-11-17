# Multi Backup r182 — collaudi provider ancora parziali
Data: 17 novembre 2026 • Branch: feature/r182-cephfs-lustre-oracle-field-ci

## Evidenze reali ricontrollate (non simulazioni)

- **AFS/OpenAFS: CI PASS del percorso volume.** [run 43747477666](https://github.com/desalvo/multi-backup/actions/runs/43747477666): CI su VM dedicata con **vero KDC Kerberos, cella AFS, VLDB, volserver, volume FULL+incrementale e restore su nuovo volume**, SHA revisionato d80675fc. Il blob di rust/src/afs_volume.rs 2df693ce5b6dac442f836abbf0f87b4bffbd1fd9 e lo script reale test-real-afs-cell.sh 98cbdd9d0b2dc27912a18a759dd003e65de0c707 sono **identici** alla revisione r181. Non equivale a certificazione universale di ogni versione futura della CLI e non richiede di consumare la VM a ogni PR.
- **Oracle Database Free: CI PASS limitato a backup piece RMAN/PDB.** [run 43908946986](https://github.com/desalvo/multi-backup/actions/runs/43908946986): vero container Oracle Free, attivazione ARCHIVELOG in istanza usa-e-getta, RMAN FULL PDB, byte di backup piece cifrati con Rust age, restore dell'archivio in nuova directory, sostituzione dei piece originali e **RMAN RESTORE PLUGGABLE DATABASE VALIDATE**. Non è un restore fisico eseguito su un'altra PDB e **non** è un adapter Oracle nativo integrato nel dispatcher. La stessa script e gli stessi motori Rust hanno blob identici nel nuovo codice; r182 aggiunge rifiuto con identità age errata e fa rieseguire la CI.
- **Lustre kernel/client: CI PASS del lab VM QEMU (non del nuovo executor E2E).** [run 43762169090](https://github.com/desalvo/multi-backup/actions/runs/43762169090): vero Lustre in Rocky9 QEMU, MDS+3 OST, due stripe e mount RO reale. NON erano ancora inclusi Rust encrypted backup, output né recovery.
- **S3 Ceph RGW/MinIO/RustFS e POSIX, NFS/SMB/GlusterFS, SSH/SFTP:** riutilizzare le CI reali già verdi riportate nei documenti r170-r181. Non attribuire un PASS a una nuova direzione solo per analogia.

## Nuovi laboratori reali r182 (l'esito è PENDING finché non risulta verde sul nuovo commit)

### CephFS
Nuovo workflow .github/workflows/cephfs-kind-real-r182.yml.
Riusa l'unico esistente script Rook/Ceph kind con tre worker **OSD veri su loop dedicati, quorum MON e CRUSH verificati**. Per questo workflow l'opzione MB_CEPHFS_LAB=1 installa un vero CephFilesystem a replica 3 con MDS attivo e un pod Ubuntu 24.04 isolato, client privilegiato con ceph-fuse, e gli copia il **binario Rust compilato dalla stessa SHA**. tests/cephfs/rook-real-client-r182.sh esige due mount CephFS, backup Rust B-In autenticato verso age chunks, restore create-only su CephFS, SHA byte-identico, B-Out age archive su RW CephFS, readback age, rifiuto sovrascrittura e input senza attestazione. Nessuna directory POSIX locale può sostituire il mount CephFS.

**Limiti:** test di file test quiescenti; non implica snapshot CephFS coerenti in presenza di scritture, ACL/xattrs/owner, replica WAN, durabilità contro perdita di 2+ nodi o fiducia root.

### Lustre
Nuovo workflow .github/workflows/lustre-qemu-rust-e2e-r182.yml.
Riusa la QEMU Rocky9 del già verde Lustre LTVM pinned upstream, ma compila un **Rust x86_64-unknown-linux-musl statico dalla SHA corrente**, avvia un server HTTP temporaneo del solo runner per consegnare binario e age statici alla VM e verifica **SHA256 fissati dall'host CI**. tests/lustre/ltvm-guest-rust-r182.sh rifiuta mock, verifica Lustre kernel, vero MDS/OST, mount RO, stripe 2; esegue backup Rust age B-In da RO, verify-chain, restore create-only RW Lustre, hash identico, B-Out archive cifrato sul mount Lustre, readback age e rifiuto overwrite.

**Limiti:** mount RO di fixture quiescenti NON è snapshot MDT/OST coordinato. Metadata Lustre (stripe sul target, uid/gid, ACL, xattr, mtime) non sono garantiti da questo adapter.

### Oracle Free
Il workflow esistente oracle-free-live.yml deve essere rilanciato sul nuovo SHA mediante l'estensione di scripts/test-oracle-free-container.sh con il test negativo sull'identità age. **Non** si usa un Oracle commerciale o un account di produzione.

## Resto del catalogo: collaudi possibili e limiti

| Gruppo | Evidenza attuale | Gap RC / disponibilità lab |
|---|---|---|
| POSIX e chunk store | CI real FULL/INC, parity, restore | produzione metadati estesi |
| GitHub B-In | Rust preview + test account manuale previsto | account di test e CI Actions/LFS end-to-end |
| Dropbox B-In/B-Out | lettura e output cifrato Rust, test negativi locali | token scrittura di account **usa-e-getta**; NON usare account personale connesso |
| WebDAV | server HTTPS reale RO + davfs2 B-In PASS | WebDAV RW e metadati ETag, permessi, lock da provare |
| CephFS | vero Rook 3-OSD CI r182 predisposto | green live CI necessaria |
| Lustre | vero KVM/QEMU Lustre smoke PASS, Rust E2E r182 predisposto | green live CI necessaria |
| GPFS NFS/SMB | test reali NFS/Samba per protocollo PASS | non è IBM CES nativo |
| AFS | reale campo KDC/cella/VLDB+volume PASS invariato | revisione CLI distinta da SHA originale, supporto distribuzione |
| PVE/PBS | inventario/token e task/UPID Rust preview | trasferimento media, nuova VM/CT restaurata e verifica, richiede ambienti |
| VMware/oVirt | inventory VM preview | guest disk backup/restore reali con licenze/hypervisor |
| Oracle | RMAN Oracle Free reale PASS pieces/VALIDATE | native adapter & isolated PDB actual restore |
| SSH/SCP/SFTP | real sshd e SSH pinned age B-In single file PASS | directory/incrementale e output SFTP diretto |
| SSHFS | live FUSE SSHFS RO e restore PASS | B-Out encrypted mounted protocol dedicato non certificato |
| Vault WORM | real S3 COMPLIANCE MinIO/Ceph RGW/RustFS | indipendenza di root/authority e HA multi-site |
| Ceph RBD / iSCSI / FC / NVMe-oF | solo pianificazione SAN/LVM | snapshot real block export/import, fencing, restore NEW LUN |
| Servizi cloud GDrive/OneDrive/... | cataloghi | adapter e consent/licenze reali |

**Regola:** CI PASS deve riferirsi a un test effettivamente eseguito e riuscito, non alla presenza dello script. AFS e Oracle hanno già risultati reali conclusi; CephFS/Lustre E2E non sono automaticamente verdi perché viene predisposto il laboratorio.

One-line English commit summary: Provision genuine CI CephFS and Lustre labs for Rust encrypted backup/restore, retest Oracle Free and record verified OpenAFS field evidence.

## r183 — aggiunta CI reale B-Out WebDAV e SSHFS

- \`scripts/test-real-webdav-r046.sh\`: oltre al RO server originale usa un **secondo server rclone WebDAV HTTPS RW** su 127.0.0.1 con autenticazione, monta con vero davfs2, esegue l'output cifrato create-only, conferma la ricevuta, effettua **GET HTTPS indipendente dalla cache FUSE** confrontandone i byte e verifica recovery age su directory nuova. Rifiuta mount RO e sovrascrittura; certificazione ancora subordinata all'esito del workflow \`webdav-live-r046.yml\`.
- \`scripts/test-real-ssh-sshfs.sh\`: riusa vero OpenSSH con hostkey pinned e client FUSE SSHFS RW, cifra archivio age con il nuovo B-Out montato, verifica readback e recovery locale, rifiuta overwrite; certificazione subordinata a \`ssh-sshfs-live-integration.yml\`.
- Questi sono adapter ciphertext-only su mount, non API WebDAV native con ETag/lock completo o un SFTP B-Out generico. Nessun PASS senza CI eseguita.


## r184–r187 — field outcomes and blockers

- **Lustre FULL + B-Out functional CI PASS** [#43993244568](https://github.com/desalvo/multi-backup/actions/runs/43993244568) on c134bcc1: verified actual Rocky9 QEMU Lustre MDT, OST2 stripe, real RO/RW kernel mounts; exact-source Rust static binary and SHA-pinned age; B-In FULL in encrypted chunks, verified create-only Lustre restore, mount ciphertext B-Out, independent age readback, wrong-target/overwrite rejection. File metadata preservation and coordinated MDT/OST snapshots remain open, so this is a **scoped CI PASS**, not global production Ready.
- **SSHFS B-Out CI PASS** [#43992703261](https://github.com/desalvo/multi-backup/actions/runs/43992703261): authenticated true SSHFS RW mount, age ciphertext output and remote age readback, fresh-target restore, duplicate target denied. SSH direct SFTP output remains separate and unqualified.
- **WebDAV r184 fail** [#43993122481](https://github.com/desalvo/multi-backup/actions/runs/43993122481): davfs2 FUSE lacks the hardlink/rename semantics used by tempfile.persist_noclobber. New \`mounted_output.rs\` WebDAV-only staging encrypts locally, uses O_EXCL remote ciphertext copy and publishes checksum marker LAST, with no delete/retry/overwrite. Initial retry [#43993669335](https://github.com/desalvo/multi-backup/actions/runs/43993669335) found even \`fsync()\` can acknowledge locally before remote HTTPS GET reflects uploaded data. The r187 *test* now polls authenticated HTTPS independently for both full ciphertext and checksum marker, bounded 120s per file; remains **Preview**, and the app must still not claim independent remote durability without ETag/API acknowledgement.
- **CephFS live** [#43992291655](https://github.com/desalvo/multi-backup/actions/runs/43992291655) is the genuine Rook/Kubernetes 3-OSD + MDS lab. Do not promote CephFS to CI PASS until it completes successfully.
- **Oracle Free** [#43992293341](https://github.com/desalvo/multi-backup/actions/runs/43992293341) scoped Oracle Free PDB RMAN piece roundtrip + restore VALIDATE PASS.
- **AFS** [#43747477666](https://github.com/desalvo/multi-backup/actions/runs/43747477666) genuine KDC/VLDB/volserver cell PASS with exact unchanged adapter module; no need to duplicate costly physical VM CI on every PR.

One-line English commit summary: Harden DAVFS2 create-only ciphertext publication and enforce independent HTTPS availability checks while preserving real Lustre, SSHFS, Oracle and AFS acceptance evidence.
