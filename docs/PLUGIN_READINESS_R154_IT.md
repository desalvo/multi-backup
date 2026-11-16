> **Aggiornamento r170 (16 Nov 2026):** per lo stato attuale di tutti i 33 plugin e quattro direzioni, consultare `docs/RC_READINESS_R170_IT.md`. La matrice seguente è uno snapshot storico r154 e alcune sue note HA/CI sono superate.

# Multi Backup — readiness RC Rust, server, HA, agenti e tutti i 33 plugin

Data: 16 novembre 2026 — evidenze CI su SHA specifici, non ancora RC stabile.

## Perimetro di certificazione

La registry `rust/src/plugins.rs` distingue il *backup input* dal *backup output*. Il restore ha una matrice separata: poter ripristinare file su un mount NFS, Samba o GPFS protocol-only non equivale a implementare uno storage di backup remoto come output. Preview non significa pronto per produzione.

## Readiness app

| Area | Stato verificato | Blocco residuo |
| --- | --- | --- |
| GUI Rust (egui) | Catalogo 33 provider ricercabile, instradamento protetto, preferenze non segrete atomiche 0600; focused CI #43897809149 e #43900053687 PASS | DMG/PKG/EXE/RPM/DEB firmati, notarizzazione, aggiornamento installato e rollback reali, UI E2E |
| Server/agent Rust | Controllo credenziali agenti, lease PG, limite 1..64 job per replica (default 4) CI #43897999559 PASS | Prova multi-host, resource limit reali e fencing di commit manifest su storage condiviso |
| HA PostgreSQL 16, Patroni 3x, etcd TLS 3x | Crash primario, sync, rejoin, quorum fencing reali PASS #43883579584 su un host | Collega tre stack di rete indipendenti, real network partition, 3 macchine fisiche |
| S3 WORM vault + restore | Ceph RGW 3 worker real #43873411605 PASS, RustFS+PG #43866257791 PASS | Linux WORM/software append-only contro root, credenziali e DR cross-host |
| Prometheus/SNMPv3 | Native server HTTPS #43886988152 PASS; real SNMPv3 authPriv+native counters #43887445516 PASS | REST monitor.read token con PG real e VLAN |
| Native Rust backup core | Encrypted age FULL/INC and POSIX RESTORE #43890856610 PASS | E2E multi-platform installer/restore |
| GPFS NFS/SMB protocol client | 5 real kernel NFS/Samba/GlusterFS jobs PASS #43891408917; core CI #43891649347 PASS | IBM CES/ILM/fileset/ACL/snapshot/failover NOT tested; no IBM native certification |
| Production Gate | Partial gates green | Full Production/security on final exact SHA not yet PASS |

## Registry degli output backup — 33 plugin

| Plugin | INPUT backup | OUTPUT backup | Evidenza reale / limiti |
| --- | --- | --- | --- |
| `github` | preview | non supportato | Rust backup preview; real account test unconfirmed |
| `dropbox` | preview | pianificato | Rust client preview; live authenticated E2E still pending |
| `google-drive` | pianificato | pianificato | Planned OAuth connector; no E2E |
| `onedrive` | pianificato | pianificato | Planned Graph connector; no E2E |
| `posix` | preview | preview | Real FULL/INC encrypted and recovery PASS #43890856610 |
| `s3` | pianificato | preview | Real Ceph RGW COMPLIANCE WORM PASS #43873411605; RustFS+PG DR PASS #43866257791; generic S3 input planned |
| `webdav` | preview | pianificato | davfs2 authenticated read-only source PASS #43890856865; output pending |
| `nfs` | preview | pianificato | Real NFS FULL/INC and mounted create-only restore PASS #43891408917; generic encrypted NFS backup output pending |
| `smb` | preview | non supportato | Real Samba SMB3 FULL/INC and mounted create-only restore PASS #43891408917; dedicated encrypted SMB output unsupported |
| `cephfs` | preview | pianificato | Mount adapter preview; true CephFS field acceptance still pending |
| `glusterfs` | preview | pianificato | Real GlusterFS FULL/INC and mounted restore PASS #43891408917; archive output pending |
| `lustre` | preview | pianificato | Isolation guards PASS #43890856887; true server acceptance open |
| `gpfs` | preview | pianificato | Only standard NFS/SMB protocol client FULL/INC and create-only recovery PASS #43891408917; IBM CES/native GPFS NOT tested |
| `hdfs` | pianificato | pianificato | API/streaming planned |
| `afs` | preview | preview | Rust vos volume dump/restore preview; real cell acceptance open |
| `alfresco` | pianificato | pianificato | REST content model planned |
| `owncloud` | pianificato | pianificato | REST/DAV integration planned |
| `nextcloud` | pianificato | pianificato | REST/DAV integration planned |
| `rbd` | pianificato | non supportato | Ceph RBD block snapshot not field-tested |
| `iscsi` | pianificato | pianificato | LUN snapshot and fencing planned |
| `fc` | pianificato | pianificato | Fibre Channel SAN snapshot and fencing planned |
| `nvmeof` | pianificato | pianificato | NVMe-oF block snapshot/restore planned |
| `proxmox` | pianificato | non supportato | Rust PVE QEMU/LXC vzdump task + UPID status unit PASS #43885350815; archive/media/restore on real PVE open |
| `pbs` | pianificato | pianificato | Rust PBS VM/CT/host snapshot listing unit PASS #43885350815; encrypted export+restore open |
| `kubernetes` | pianificato | non supportato | Kubernetes/OpenShift backup/restore resources/PV planned |
| `vmware` | pianificato | non supportato | Rust vCenter VM-only inventory unit PASS #43887673635; VADP/CBT backup+restore missing; no CT |
| `ovirt` | pianificato | non supportato | Rust oVirt Engine VM-only inventory unit PASS #43887673635; disk transfer backup+restore missing; no CT |
| `mysql` | preview | non supportato | Rust logical dump+new-target restore preview; real DB acceptance pending |
| `postgresql` | preview | non supportato | Real PG16 and RustFS encrypted paired DB/data recovery PASS #43866257791; PITR pending |
| `oracle` | pianificato | non supportato | RMAN fail-closed checks PASS #43890856887; real Oracle field test pending |
| `ssh` | pianificato | pianificato | SSH/SCP/SFTP real protocol PASS #43890856669; generalized backup target integration planned |
| `sshfs` | preview | pianificato | Real SSHFS read-only FUSE backup PASS #43890856669; output pending |
| `anti-ransomware-vault` | non supportato | preview | Rust S3 WORM real Ceph/RustFS PASS; separate root-resistant Linux vault field hardening remains |

## Note di sicurezza e gate RC

1. No stable tag, signed release or main merge until full exact-SHA Production Gate is green.
2. Patroni network isolation CI is **NOT yet PASS**: hosted runner virtual-bridge L2 ARP forwarding failed. Current dedicated P2P netns path is under acceptance. Three physical hosts remain separate.
3. PVE/PBS native backup archive export and real create-only VM/LXC restore still pending; VMware/oVirt VM-only native backup and restore still pending and no CT allowed without documented support.
4. GPFS may enter only the **open NFS/SMB client protocol compatibility** profile; IBM Storage Scale native server behavior unqualified.
5. Rust GUI still requires actual signed cross-platform native installers, update/rollback and visual/UX acceptance.

English commit summary: Integrate tested Rust GUI, bounded server workers and GPFS NFS/SMB while recording every plugin readiness gate.


## r157 — genuine three-namespace real network Patroni/etcd acceptance integrated

- [Real HA run 43901963939](https://github.com/desalvo/multi-backup/actions/runs/43901963939) **PASS** with three actual PostgreSQL 16/Patroni and three mutually TLS-authenticated etcd members, each on an independently routed Linux network namespace and point-to-point veth stack on one disposable hosted runner.
- Six namespace reachability probes PASS; witness transaction ACKed on strict synchronous primary; real `iptables DROP` severed the primary from both peers; surviving etcd majority elected a new primary; previously ACKed WAL remained readable; old isolated primary did NOT acknowledge a write; after healing the old leader rejoined as replica and exactly one primary remained. Explicit teardown cleans only ephemeral host subnetwork/forwarding and namespaces.
- Earlier [functional HA 43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) remains PASS for leader process crash, strict no-sync write fencing, both standby rejoin and DCS majority-loss blocking.
- This is **NOT 3 physical hosts/racks or certified external network appliance fault domains**; it certifies genuine isolated Linux network stacks and real peer network partitions. This new integration triggers a one-shot exact-SHA rerun on the active RC feature branch.
- This r157 work does not complete native installer notarization/signatures, hypervisor backup media+restore or final Production Gate. No main merge or release tag.

One-line English commit summary: Integrate real isolated Patroni PG16 and TLS etcd partition, strict fencing and confirmed failover acceptance.


## r158 — server/agent replica capacity and admission startup fail-closed (16 Nov 2026)

- Rust server validates `MULTI_BACKUP_REPLICA_MAX_JOBS` is 1..64 and, when execution is explicitly enabled, that `MULTI_BACKUP_CLUSTER_MAX_ACTIVE_JOBS` is a valid 1..64 number **not exceeding the replica's published PostgreSQL worker capacity**. Invalid configurations abort the server startup instead of accepting jobs the process cannot safely execute.
- Actual native worker already obtains and holds a local semaphore permit before any durable PG job claim and through lease heartbeat and final status commit (r148). A saturated replica leaves pending jobs in PostgreSQL for other eligible workers. Rust unit tests cover boundary/rejection cases. [Focused server CI 43902435010](https://github.com/desalvo/multi-backup/actions/runs/43902435010) **PASS**.
- This is admission control, not a certified global exactly-once transaction/storage write fence; real multi-replica network failure testing and manifest atomic fencing remain open. No signed RC/stable release has been issued.

One-line English commit summary: Reject mismatched replica admission limits at server startup before distributing jobs.


## r160 — extend PVE Rust recovery prerequisites with actual remote backup media inventory

- Native `plugins pve-backup-media --config-json CONFIG --node NODE --storage STORAGE --guest-id VMID --json` lists provider-identified PVE `vzdump` backup media using the read-only endpoint `GET /api2/json/nodes/NODE/storage/STORAGE/content?content=backup&vmid=VMID`. Bounded HTTPS/no redirect, owner-only scoped token; rejects other guest's VMID, another datastore, unsafe volume paths, duplicates, oversized or malformed responses and cross-provider credentials.
- Information includes VM/CT subtype when source explicitly provides it, format, volume ID, size/time, protected flag and provider verification outcome. Crucially `multibackup_archive_verified=false`, `exported_to_multibackup=false` and `multibackup_restore_certified=false` regardless of the provider state.
- [Focused Rust core unit CI 43904400603](https://github.com/desalvo/multi-backup/actions/runs/43904400603) **PASS** on source r159. A real authenticated Proxmox cluster, ciphertext archive capture, restore-new VM/CT and verification against source remain RC/field blockers.
- Integrated into current Rust RC development line, not main or a release tag; full final Production Gate remains mandatory.

One-line English commit summary: Integrate tested PVE native backup-media inventory with strict per-guest verification provenance.
