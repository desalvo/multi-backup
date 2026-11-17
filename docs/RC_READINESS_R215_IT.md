# Multi Backup r215 — READY circoscritti e gate verso RC

Data: 18 novembre 2026. Branch feature/r215-scoped-provider-ready-and-rc, stacked sopra r214. Nessun merge su main, tag, stable o release.

## Perimetro concreto del READY

READY non dichiara pronto un intero prodotto/protocollo. Si riferisce SOLO a una specifica operazione B-In/B-Out/restore realmente implementata e collaudata. Ciascun plugin pronto dichiara il campo JSON ready_scope. Il planner generico executable=true rimane riservato alla coppia POSIX→POSIX effettivamente compilata: i provider dedicati non acquisiscono per questo poteri di scrittura.

| Plugin | READY ruoli (B-In/B-Out/R-In/R-Out) | Qualificazione e limiti |
|---|---|---|
| posix | R/R/R/R | File regolari su albero fermo, age/parità e restore create-only. Niente ACL/xattr, owner/mode/date, symlink/hardlink, directory vuote, consistenza applicativa live |
| s3 | R/Preview/Preview/Planned | Import di UN oggetto version-pinned MinIO, lunghezza e SHA256 indipendenti. Niente bulk, ACL, restore-to-S3 o vendor non verificati |
| cephfs | R/Preview/Preview/R | Montaggio CephFS autentico e snapshot sorgente fermo attestato, SOLO contenuti file regolari e restore nuova directory. Niente coerenza multi-FS, metadati completi o guasti fisici |
| rbd | R/Unsupported/Unsupported/R | Snapshot RBD pinned da ID numerico, export e SHA, nuova image con readback. Niente guest quiesce, diff/CBT o fencing |
| iscsi | R/Planned/Planned/R | Linux vero, LUN RO pinned da IQN/WWID+SHA, nuova LUN locale TGT isolata e readback. Niente SAN remoto, CHAP/multipath, LVM live o fencing |
| mysql | R/Unsupported/Unsupported/R | Un solo DB InnoDB MariaDB11.4 o Oracle MySQL8.4, dump FULL transazionale, age, nuovo DB, verifica dati. Niente GTID/PITR, globali, DDL concorrenti o recovery Galera completo |
| postgresql | R/Unsupported/Unsupported/R | Un DB PostgreSQL16 logico FULL, age, nuovo DB e confronto righe. Niente ruoli/globali, PITR coordinato Patroni o HA fisico |
| oracle | R/Unsupported/Unsupported/R | Preservazione media RMAN .bkp già creati, verifica age e restore media in nuova directory. NON esegue automaticamente DB RESTORE/RECOVER. Oracle Free reale PDB/CDB/PITR validato separatamente; niente RAC/ASM/TDE |
| anti-ransomware-vault | Unsupported/R/R/Unsupported | Solo MinIO S3 Object Lock COMPLIANCE, archivio age e manifest versionati, due utenti non-admin e recupero version-pinned. NON resilienza provata a root/host, guasti fisici, operatori vendor, né immunità totale ransomware |

9 plugin READY su 33 solo negli scope riportati; 24 sono ancora Preview, Planned o Unsupported in tutte le direzioni.

## Gate r215, esecuzione reale sul medesimo commit

- Full software-readiness-r168.yml: Rust core/server/GUI, PostgreSQL/agent/HA, 3 nodi Kubernetes kind e OCI.
- Real database-live-integration.yml: PostgreSQL16 e MariaDB11.4 age encrypted backup/restore.
- Native scoped-ready-r215.yml: cargo test plugin contract, vero POSIX encrypted restore e prova negativa del planner.
- Sei LAB obbligatori e riusati, non simulati: mysql84-real-r209.yml; cephfs-kind-real-r182.yml (RBD+CephFS 3 OSD); iscsi-live-r198.yml (kernel/TGT); oracle-free-live.yml (RMAN/PDB/CDB/PITR); s3-worm-iam-real-r211.yml (due credenziali MinIO COMPLIANCE); s3-minio-live-r048.yml (oggetto MinIO versione-pinned con digest indipendente, backup age e restore).
- Python tests r215, sicurezza Ruff/Bandit/pip-audit; non ammettere alcuna suite skipped/cancelled/failure. Il manifest finale include source SHA, run GitHub e esiti dei sei LAB e verifica i 9 READY con scripts/ci/rc-preflight-r215.py.
- Solo quando tutti i job sono SUCCESS si compila Linux CLI e agent da SHA esatto, con SHA256SUMS e artefatto CI pre-RC. Non si pubblica release, tag o stable.

## Blocchi aperti

VMware VADP/CBT/guest boot reali, oVirt full VM, PVE/PBS full VM/CT, HA su tre host fisici distinti e fencing, Oracle RAC/ASM/TDE, MySQL GTID/PITR, SAN remoto, Lustre MDT/OST coordinato, GPFS IBM nativo; test Mac Apple Silicon/Intel DMG, Windows installers, RPM/DEB con integrità, firma e provenance dallo stesso SHA. Non riusare vecchio publisher r174/r175 con SHA storico.

## Evidenze precedenti, da riconfermare sullo SHA r215

- r214 full CI Linux PASS: https://github.com/desalvo/multi-backup/actions/runs/44187545286
- MinIO WORM IAM due principal: https://github.com/desalvo/multi-backup/actions/runs/44083904597
- iSCSI kernel nuova LUN: https://github.com/desalvo/multi-backup/actions/runs/44084216177
- CephFS/RBD 3 OSD: https://github.com/desalvo/multi-backup/actions/runs/44040852185
- Oracle Free CDB/PDB/PITR: https://github.com/desalvo/multi-backup/actions/runs/44083604906
- SQL MariaDB/MySQL/PG: https://github.com/desalvo/multi-backup/actions/runs/44072156248

One-line English commit summary: Qualify nine bounded provider READY scopes with five genuine exact-SHA vendor labs and fail-closed pre-RC gating.
