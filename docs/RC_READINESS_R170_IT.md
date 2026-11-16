# Multi Backup — matrice RC verificabile r170 (16 novembre 2026)

**Fonte dei ruoli:** `rust/src/plugins.rs` sul branch `ci/r168-software-field-gates`; esattamente 33 plugin; 4 direzioni. **Fonte delle prove:** workflow su commit esplicito, in particolare [software gate r168](https://github.com/desalvo/multi-backup/actions/runs/43920969224) PASS sullo SHA `a9b02c93`. La nuova estensione di r170 resta **da verificare su SHA finale**: non chiamarla PASS finché non si osserva il job finale verde.

## Legenda

- `A` = `Preview` — esiste un percorso Rust dedicato, **non** automaticamente collegato al dispatcher generico o certificato per produzione
- `P` = `Planned` — contratto esistente, intero executor non pronto
- `—` = `Unsupported` — direzione intenzionalmente non supportata.
- `B-In` sorgente backup, `B-Out` destinazione backup; `R-In` sorgente restore, `R-Out` destinazione restore. Nell'implementazione corrente il catalogo deriva `R-In=B-Out` e `R-Out=B-In`, ma la certificazione pratica è separata.

## Readiness app

| Area | CI | Residuo |
|---|---|---|
| Core Rust age / chunk / recovery | r168 PASS | Integrazione plug-in generic dispatcher, installazione/restore multi-OS reale |
| Server Rust / OCI / Kubernetes | r168 PASS | Global shared-manifest writer fencing, DR fisico, agenti remoti su TLS |
| Agenti Rust / upload age + PG16 | r168 PASS | Network/TLS tra macchine vere, load/memory throttling remoto e restore provider |
| PostgreSQL16 | r168 PASS | Patroni/etcd su 3 host fisici con crash, split, quorum, rejoin |
| GUI Rust Linux Xvfb | r168 PASS solo avvio; r170 10-page sweep pending | E2E click-through, accessibility, Mac/Windows signed packages/update rollback |
| Monitor read-only REST | r168 unit; r170 real-PG handler pending | Certificare TLS da GUI su macchina diversa, CA e token remote |
| Ceph RGW / RustFS WORM | prove backend reali PASS | Protezione vault Linux da root locale e DR cross-host |
| Production Gate RC completo | NON PASS | Gate sicurezza, tutte le piattaforme installabili, scope plugin certificati |

## Plugin input/output/restore (33)

| Plugin | B-In | B-Out | R-In | R-Out | Evidenza e residuo |
|---|:---:|:---:|:---:|:---:|---|
| `github` | A | — | — | A | Mirror+metadata in anteprima; manca account/test cloud E2E |
| `dropbox` | A | P | P | A | Lettura incrementale; autenticazione/upload reale E2E aperti |
| `google-drive` | P | P | P | P | OAuth e API cloud non integrate |
| `onedrive` | P | P | P | P | Microsoft Graph/OAuth e upload da integrare |
| `posix` | A | A | A | A | FULL/INC age e restore reale PASS; generic dispatch/ACL completi aperti |
| `s3` | P | A | A | P | Ceph RGW/RustFS WORM reali PASS; input S3 generico ancora P |
| `webdav` | A | P | P | A | davfs2 autenticato come sorgente reale PASS; output non pronto |
| `nfs` | A | P | P | A | Backup e restore mounted reale PASS; output archivio P |
| `smb` | A | — | — | A | SMB3 backup e restore mounted reali PASS; non destinazione archivio |
| `cephfs` | A | P | P | A | Client/mount anteprima; CephFS reale da certificare |
| `glusterfs` | A | P | P | A | Backup/restore mount reali PASS; archivio output P |
| `lustre` | A | P | P | A | Guardrail PASS; server Lustre reale non certificato |
| `gpfs` | A | P | P | A | Solo interoperabilità NFS/SMB standard; IBM nativo escluso |
| `hdfs` | P | P | P | P | HDFS API/streaming da implementare |
| `afs` | A | A | A | A | vos dump/restore anteprima; prova cella AFS reale aperta |
| `alfresco` | P | P | P | P | REST documentale da implementare |
| `owncloud` | P | P | P | P | DAV/REST da implementare |
| `nextcloud` | P | P | P | P | DAV/REST da implementare |
| `rbd` | P | — | — | P | Snapshot block/restore nativi non verificati |
| `iscsi` | P | P | P | P | Snapshot LUN e fencing reale aperti |
| `fc` | P | P | P | P | Snapshot SAN/LVM locking e fencing aperti |
| `nvmeof` | P | P | P | P | Namespace/snapshot/restore bloccati in pianificazione |
| `proxmox` | P | — | — | P | Inventario PVE e media vzdump; mancano export/restore VM+CT reali |
| `pbs` | P | P | P | P | Inventario snapshot PBS; manca export/restore media reali |
| `kubernetes` | P | — | — | P | Backup risorse/PV/cluster da implementare (Kind server != plugin) |
| `vmware` | P | — | — | P | Inventory vCenter VM PASS unit; manca VADP/CBT, VM restore; no CT |
| `ovirt` | P | — | — | P | Inventory Engine VM PASS unit; manca disk-transfer/restore; no CT |
| `mysql` | A | — | — | A | MariaDB dump/restore criptato e binlog PITR reali PASS; dispatch preview |
| `postgresql` | A | — | — | A | PG16 dump/restore cifrato e WAL/PITR reali PASS; dispatch preview |
| `oracle` | P | — | — | P | RMAN policy fail-closed; Oracle vero non testato |
| `ssh` | P | P | P | P | SCP/SFTP protocollo reali PASS; adapter backup/restore generico P |
| `sshfs` | A | P | P | A | Lettura mount SSHFS reale PASS; output archivio P |
| `anti-ransomware-vault` | — | A | A | — | Output S3 COMPLIANCE/WORM restore reali; Linux root-resistant aperto |

## Gating reale e restrizioni

- **3 host fisici:** runner self-hosted effimero in repository *privato* distinto, il medesimo script preflight `scripts/ci/ha-three-host-field-r166.py`; non duplicare fixture. Il preflight non fa failover né prova power/network fault. Il guasto fisico richiede autorizzazione, macchine di laboratorio indipendenti e raccolta evidenze.
- **Vault root-resistant:** non dichiarare immune a root se le credenziali di scrittura, l'OS e il backend sono sotto il controllo dello stesso root. Per credenziali compromesse con retention forte utilizzare S3 Object Lock COMPLIANCE versioned e account/trust boundaries indipendenti; testare cancellazione anticipata, ripristino e rotazione credenziali.
- **Esclusioni GPFS:** solo profili client aperti NFS/SMB, non IBM Storage Scale nativo; nessun licenziamento IBM implicito.
- **VM/CT:** PVE QEMU e LXC solo media/inventory preview; VMware e oVirt trattano VM, non presunti container.
- **Mac e Windows:** senza CI installer firmata/notarizzata e test E2E non considerare la GUI multipiattaforma completata.
- **Release:** non fare merge su main, non emettere tag RC/stable, non promuovere un plugin a `Ready` sulla base di soli unit test. Nessun percentuale inventata.

One-line English commit summary: Define evidence-based four-direction readiness for all 33 Rust plugins without falsely promoting unqualified providers.
