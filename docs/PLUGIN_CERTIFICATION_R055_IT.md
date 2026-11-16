# Multi Backup r055 — matrice di certificazione effettiva

Stato 16 novembre 2026. PASS di un workflow reale vale esclusivamente per il profilo effettivamente testato; non promuove un adattatore bidirezionale PLANNED/PREVIEW a READY.

| Provider | CI reale | Perimetro verificato | Non certificato |
| --- | --- | --- | --- |
| POSIX | PASS 43819587170 | Rust full/incremental, recovery | ACL/xattr e snapshot generale |
| NFS, SMB, GlusterFS | PASS 43819587223 | Server/mount reali e recovery | Replica/snapshot/metadata fedeltà completa |
| WebDAV davfs2 HTTPS | PASS 43819587120 | Ingest da mount autenticato RO | Output WebDAV, ETag, metadata |
| SSH, SCP, SFTP, SSHFS | PASS 43819587193 | Autenticazione, recupero su servizio reale | ACL cross-platform e policies distribuite |
| PostgreSQL, MariaDB | PASS 43819587187 | Full dump cifrato Rust e restore | Tutte le versioni, roles/extensions |
| PostgreSQL WAL, MariaDB binlog | PASS 43819587987 | PITR reale | Disaster drill multi-node a lungo termine |
| Galera | PASS 43819587315 | Cluster 3 nodi e PITR | Fault injection sotto produzione |
| Oracle Free RMAN | PASS limitato 43819587238 | Backup-piece cifrato e RMAN VALIDATE | Oracle adapter generic, CDB/PDB full recovery |
| MinIO WORM | PASS 43819587240 | Object Lock COMPLIANCE, age, verify/restore, delete denial | Manifest esterno firmato, marker, multipart |
| RustFS WORM | PASS 43819587169 | RustFS 1.0.3 reale, COMPLIANCE, verify/restore, delete denial | Cluster multi-node e root-admin |
| Ceph RGW WORM | FAIL 43819587159 | Rook ha avviato i pod ma non raggiunto 3 MON/3 OSD | S3 Object Lock Ceph reale finché non PASS indipendente |
| GitHub, Dropbox | NON CERTIFICATO account reali | Rust client/code e test locali | Account prova, restore, CI/LFS, rate limits |
| OpenAFS | NON CERTIFICATO laboratorio reale | Rust vos tools in anteprima | Cella Kerberos/AFS reale e restore volume |
| CephFS, RBD, iSCSI, FC, NVMe-oF | NON CERTIFICATO | Contratto/guard Rust | Snapshot, fencing, restore isolato |
| Google Drive, OneDrive, HDFS, Alfresco, ownCloud, Nextcloud | NON CERTIFICATO | Contratto/planner | OAuth/metadata/versioning/restore reale |
| Proxmox, VMware, oVirt, Kubernetes come sorgente | NON CERTIFICATO | Contratto/planner | Snapshot workload e restore reale |
| GPFS IBM CES | ESPERIMENTALE, escluso dalla RC | Preflight statici e piano field | Test CES/IBM validi |

Riferimenti completi: https://github.com/desalvo/multi-backup/actions/runs/43819587240, https://github.com/desalvo/multi-backup/actions/runs/43819587169, https://github.com/desalvo/multi-backup/actions/runs/43819587159.

Regole: non scambiare uno skipped o un FAIL per PASS; ogni prova deve registrare SHA e job; non eseguire test distruttivi su account/storage utente senza laboratorio disposable isolato; non attivare generic plugin per provider pianificati.
