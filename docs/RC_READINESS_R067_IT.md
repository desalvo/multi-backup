# Multi Backup 0.1.0 — RC readiness, r067 (16 November 2026)

## Stato reale e metodologia

Questa è una valutazione basata su sorgenti GitHub, file di CI e risultati **completati** (non uno stato prodotto dal numero di linee Rust). Ogni PASS è legato alla versione e al profilo testato; un PASS su un precedente SHA non certifica automaticamente la nuova testa. Tutte le PR restano draft; nessun nuovo tag RC/stable è stato pubblicato.

| Workstream | Implementazione | Collaudi reali | RC perimetro minimo |
| --- | --- | --- | --- |
| Core cifrato, POSIX, dedup, resumable, restore | Rust esteso, CLI e agenti | roundtrip POSIX e unit/fault suites precedenti verdi | Verifica release su SHA finale, disaster drill multi-GB e compatibilità archivio |
| Desktop Rust macOS/Windows/Linux | eframe + installer preview | build e smoke precedenti verdi | Installare/avviare/aggiornare/rollback su target reali; Apple notarization e Windows firma |
| Server Rust REST/WebDAV/agent | API/UI e cluster inventory | Rust focused 43839135843 PASS | Audit auth/TLS, installer RPM/DEB, readiness/failover reale |
| HA workload e cluster | PostgreSQL leases, workers e storage location aware | 43836100718 PASS sul profilo 2-replica; nuove prove PG 43839136135 FAIL con token SQL Type | PG test r067 e prove Patroni/etcd primario→replica, split-brain, reintegro e fencing |
| Schema/database interno | quattro migrazioni checksum SHA256, backup cifrato in S3 WORM e schedule | test PG reale in correzione | Restore da DB+dati su cluster vuoto, point-in-time/rollback, lease>30min, coerenza recovery |
| Monitoring | Prometheus:9502; API TLS:8443 token; SNMP configurabile | compilazione server; manca certificazione HTTPS/SNMP | Live authorization, revoca, TLS, monitor senza dati sensibili e isolamento DB esterno |
| Vault S3 MinIO | Object Lock, versioning, cifratura e restore | PASS full CI 43819587305 e indipendente | Catalogo autenticato e recovery da delete-marker |
| Vault S3 RustFS | versione 1.0.3, COMPLIANCE e restore | PASS indipendente 43837819453 | HA/large dataset/retention failure drills |
| Ceph RGW S3 | attestation multi-OSD KIND reale | 43832467180 **FAIL**: 3 OSD UP+IN e RGW, ma 3 OSD stessi CRUSH host | OSD fisicamente/virtualmente collocati su tre nodi K8s distinti; S3 WORM verify/restore su RGW |
| Network filesystem / DB PITR / Oracle | adapter Rust con profili Preview e field guards | differenti scenari reali superati in workflow dedicati | Garantire metadati, on-disk compatibilità, restore su infrastruttura isolata |
| GitHub / Dropbox | codice Rust e test locali; laboratori field manuali | account reali non certificati | Credenziali test isolate, E2E backup→restore e throttling/retry |
| PVE / PBS / VMware / oVirt | Catalogo `Planned`, senza motori completi | Nessuna acceptance reale | Esclusi dalla prima RC limitata o realizzazione E2E preventiva |
| GPFS/IBM CES | experimental nonblocking | field certification non completata | Escluso dalla RC, non deve bloccare gli altri |

## Blocchi per una prima RC **a perimetro limitato**

1. **BLOCCANTE:** tutti i test Rust/server sul commit finale, inclusi PostgreSQL lease/token/migrazioni; non propagare FAIL o SKIP come successo.
2. **BLOCCANTE:** installer e avvio nativi macOS/Windows/Linux; RPM/DEB bare-metal smoke install e uninstall; rispetto delle politiche di firma/canale.
3. **BLOCCANTE:** disaster recovery combinato database server + dati/vault, cifratura e integrità, perdita del primario e recovery senza doppio writer. Testare su lab, non su dati utente.
4. **BLOCCANTE:** sicurezza reale API e monitor, auth/revoca, SSL, isolamento e ingress per PostgreSQL/etcd.
5. **BLOCCANTE:** full Production Gate green sull'ultimo SHA prima di merge/pubblicazione e chiusura ordinata della catena PR #10–14, #20–22.
6. **BLOCCANTE SOLO SE CEPh È IN PERIMETRO:** certificare Ceph RGW su vero cluster di tre OSD distribuiti + S3 Object Lock. Può rimanere fuori da una RC limitata insieme a hypervisor/GPFS, ma va dichiarato come non pronto.
7. **BLOCCANTE SOLO SE IN PERIMETRO:** account E2E Dropbox/GitHub e hypervisor. Nessun provider simulato è “ready”.

## Approccio a basso consumo CI

- Eseguire sul push della patch soltanto `rust-focused.yml` e test PostgreSQL PG16 su adattatori pertinenti; niente Oracle/Ceph/bare-metal 12 target.
- Packaging bare-metal: prima smoke su 4 target rappresentativi, matrice estesa una sola volta sullo SHA candidato e non per ogni commit.
- Ceph: non rilanciare l'ennesimo test kind con loop-host condivisi; verificare preventivamente isolamento OSD, poi un solo field test mirato.
- Stabilizzare API/manifest, congelare il commit candidato, quindi singola full CI con Production Gate e prove di release.

## Valutazione del lavoro rimanente

Non esiste una percentuale di completamento attendibile per una “RC completa di tutti i plugin”: molte integrazioni sono ancora semplici contratti `Planned`. Per una **RC ristretta** (Rust core + POSIX + backend S3 MinIO/RustFS verificati + server singolo) i blocchi chiave sono **5 gruppi** sopra: test del nuovo server, packaging firmato/installato, recovery reale, sicurezza, gate finale. Per una **RC HA** si aggiunge failover Patroni/etcd e write fencing end-to-end. Per una **RC con tutti i plugin** resta un programma sostanziale di implementazione E2E per hypervisor, provider cloud, OpenAFS, Ceph e storage enterprise; non è corretto considerarla imminente.

Repo: https://github.com/desalvo/multi-backup . Ultima full CI completa nota verde: https://github.com/desalvo/multi-backup/actions/runs/43819587305 ; ma NON sullo SHA finale.

## r069 — plugin hypervisor: separazione fonte PVE/PBS

La separazione formale di `proxmox` (PVE) e `pbs` (Proxmox Backup Server) è completata nel catalogo Rust, con test fail-closed. È un passo di modellazione dell'interfaccia, NON un motore di backup: entrambe le integrazioni restano Planned e senza prova E2E reale. VMware/oVirt continuano VM-only; CT soltanto da API ufficiali dell'hypervisor e collaudo per modalità.

## r070 — bare-metal RPM build failure fix (16 Nov 2026)

The full 12-target Rust server packaging smoke run [43838871158](https://github.com/desalvo/multi-backup/actions/runs/43838871158) finished: **11 SUCCESS, Rocky Linux 9 x86_64 FAILURE**. Rust server compiled on failed target too; rpmbuild failed because `%{_unitdir}` is undefined in the Rocky 9 container. Replaced that macro with explicit `/usr/lib/systemd/system` for install and filelist, added a Python regression test and a cheap Rocky9 `rpmspec -P` syntax job that does NOT rebuild the entire 12 target package matrix. This is a fix pending confirmation, not yet a green final installation certification.

## r071 — PostgreSQL DB backup fencing and long-running lease (16 Nov 2026)

Fixed the server-internal backup writer's fixed 30-minute lease: `renew_db_backup_lease` now extends only a still-valid UUID job+token lease on PostgreSQL; a dedicated 45s heartbeat keeps large `pg_dump`/age/S3-WORM jobs alive, and loss of the lease/PG quorum prevents a success record. The encrypted database dump and S3 object are checked against the current lease before and after remote write; a write committed during connectivity loss is an orphan requiring explicit reconciliation, not an automatic success. Genuine PG16 CI adds wrong-token, late renewal, completed-token replay and fail-closed expired-writer checks. This change does not certify simultaneous database+data snapshot consistency or 3-node Patroni failover. Full RC still requires signed packages, isolated disaster recovery, and final SHA gate.

## r072 — Ceph RGW CRUSH host evidence (16 Nov 2026)

Real CI 43832467180 showed three OSDs UP+IN, 3-MON quorum and RGW on distinct *virtual kind Kubernetes workers* but failed the mandatory 3-host CRUSH tree check. Added strict disposable-kind-only alignment: before a native CRUSH move, verify three real running pods, unique OSD IDs, their actual Kubernetes node assignments, exact unique pre-created loop block devices, active Ceph OSDs and MON quorum. Any mismatch fails closed; final independent 3-host attestation remains mandatory, no mocked PASS or physical failure-domain claim. Offline guard tests run in the cheap CI policy workflow. Full Ceph test remains explicit opt-in and Ceph is NOT certified until a real PASS.

## r073 — read-only native Rust PVE/PBS inventory (16 Nov 2026)

New `rust/src/virtualization_inventory.rs` supports explicit read-only PVE QEMU/LXC guest discovery and PBS datastore listing over verified HTTPS and scoped API token from owner-only (0600) JSON config. Redirects refused; response bounded to 1 MiB; token redacted from all output. CLI: `multi-backup-native plugins virtualization-inventory --config-json OWNER_ONLY.json --json`. The descriptor for both providers remains **PLANNED, NON-EXECUTABLE**: this API inventory is NOT a VM/CT backup, snapshot, restore, or live-account certification. Offline unit tests verify native guest types, fail-closed protocols and duplicate identities. No hypervisor field run without disposable infrastructure and explicit credentials.

## r076 — one-shot genuine Ceph RGW WORM live acceptance

The PR label was accepted but did not create a Ceph workflow run on a non-default nested feature PR. A **single explicit one-shot request file** `.github/ci-requests/ceph-rgw-r076.request` now triggers exactly the isolated existing 30-minute Rook-Ceph 3-MON/3-OSD RGW workflow on the feature branch. It runs the new actual OSD-pod/device CRUSH alignment guard before the independent unchanged host attestation and actual S3 Object Lock COMPLIANCE/age backup, verify, restore and adversarial delete tests. The marker cannot launch future repeated tests unless deliberately edited. This is still three separate virtual kind worker nodes on one GitHub physical runner, so it cannot certify three independent physical fault domains. No PASS until actual successful job logs; do not execute other expensive matrices.

## r077 — SNMP standalone support without PostgreSQL schema mutations

SNMP polling now uses a validated **read-only PostgreSQL connection constructor** and never calls the startup auto-migration function. A standalone server without shared PostgreSQL produces its own safe local server metrics instead of failing the SNMP extend command. Shared-cluster SNMP remains a read-only inspection of the running schema. Unit tests ensure the standalone output has one local node, no primary database and no manifest secrets. This is not a substitute for real Net-SNMP authPriv integration and external network-isolation acceptance.

## r078 — native Rust release without Python rebundling (16 Nov 2026)

The previous main `ci.yml → build-native.yml` publication path still rebuilt Python/PyInstaller GUI/CLI installers despite compiling true native Rust preview packages. **Redirected `native-release` to `.github/workflows/build-rust-release.yml`**, a reusable same-run artifact collector: it does not rebuild any GUI/CLI/agent, requires the complete Rust-only Mac/Windows/RPM/DEB/agent job matrix and Production Gate/real MinIO/Kubernetes passes, rejects missing/duplicate/legacy assets, regenerates checksums and a provenance release manifest, then updates ONLY a rolling *early-release* from main. On stable tags it fails closed pending Developer ID notarization, Windows signature and GPG. The old PyInstaller workflow remains in the repository as a legacy reference but is not called by production release. Legacy Mac Python smoke is now opt-in on workflow_dispatch, reducing 2 expensive Mac jobs on ordinary full CI. Legacy Python-screen capture/manual publication remains an explicit separate gate, not included as misleading Rust user documentation in published native assets. Static Production Gate tests updated to enforce the correct real Rust publication path; new collector unit tests added. **No release published/merged from this draft branch; real final full CI on final SHA still required.**

## r079 — Ceph real pod node/device mismatch and fast RC policy tests (16 Nov 2026)

Ceph live run [43846329965](https://github.com/desalvo/multi-backup/actions/runs/43846329965) is FAIL: 3 MON quorum, 3 real OSD UP+IN and RGW, but two Rook OSD pods were scheduled on the same virtual kind worker, with their ROOK_BLOCK_PATH mismatching the original worker/device allocation. The previous CRUSH-only alignment correctly REFUSED as unsafe. New `scripts/ci/ceph-osd-node-align.py` checks exact 3 genuine UP OSD IDs, pod names/status, unique real loop paths and intended workers before issuing a narrowly scoped `kubectl patch deployment rook-ceph-osd-N` nodeSelector on disposable kind, then retests and applies previously guarded CRUSH repair. No mock cluster, no lowering 3 MON/3 OSD/3 CRUSH hosts, no production cluster modifications. Cheap regression tests added. Also added Rust release collector offline tests and `scripts/check-production-gate.py` to the cheap CI policy job so future release wiring mistakes do not waste a full CI. **No Ceph PASS** until a fresh real S3 WORM run passes with correct pod/CRUSH placement.

## r081 — private-only PostgreSQL/Patroni/etcd network exposure gate

Added cheap, fail-closed static tests for Kubernetes Service/Pod exposure, HAProxy's localhost-only 6432 primary routing, Patroni's private VLAN 5432/8008 listeners, TLS-authenticated private etcd endpoints and the server's strict TLS-to-proxy DSN. This protects against accidental publication of DB ports 5432, etcd 2379/2380 or Patroni 8008 by changed example manifests. **It is a static release policy, NOT proof of actual firewalld/nftables, cloud network ACL isolation, host scanning or Patroni HA failover**: these need a disposable real deployment before HA RC.

## r071-r082 — evidenza RC verificata, 16 novembre 2026

- PostgreSQL: r071 aggiunge heartbeat lease 45s e fencing anti-replay per backup DB nel WORM; CI PostgreSQL reale https://github.com/desalvo/multi-backup/actions/runs/43843182470 e Rust https://github.com/desalvo/multi-backup/actions/runs/43843182685 sono PASS. SNMP standalone read-only evita le migrazioni dello schema al polling; PG https://github.com/desalvo/multi-backup/actions/runs/43846716012 e Rust https://github.com/desalvo/multi-backup/actions/runs/43846715919 PASS nei job eseguiti.
- Bare metal: RPM nativo Rust installato, testato con vere dipendenze DNF e disinstallato su Rocky Linux 9 x86_64, preservando dati: PASS https://github.com/desalvo/multi-backup/actions/runs/43846119136. Matrice 12 OS/CPU, aggiornamenti e certificazioni firma non ancora finali.
- RustFS: real server 1.0.3 con backup/verify/restore age e Object Lock COMPLIANCE PASS https://github.com/desalvo/multi-backup/actions/runs/43844964347. MinIO PASS precedente. Ceph RGW https://github.com/desalvo/multi-backup/actions/runs/43846329965 FAIL nonostante MON, OSD e RGW reali: pod OSD non collocati sulle rispettive allocazioni disco-worker; correzione guardata r079 e singolo collaudo reale https://github.com/desalvo/multi-backup/actions/runs/43848726571 avviato. NON dichiarare Ceph certificato prima di esito positivo.
- PVE/PBS: Rust API read-only per inventario QEMU/LXC e datastore, con HTTPS e token scoped, test focused https://github.com/desalvo/multi-backup/actions/runs/43845460116 PASS. Nessun backup live/offline/restauro PVE/PBS pronto. VMware/oVirt VM-only Planned, CT solo su API native vendor documentate e validate per versione.
- Distribuzione Rust: il workflow release ora raccoglie senza ricompilazione PyInstaller i veri artefatti GUI/CLI/agent prodotti dalla stessa Production CI, con matrice obbligatoria, checksum e manifest; stable resta bloccata fino alle firme. Workflow legacy mantenuto soltanto come riferimento e non chiamato dal nuovo publisher. Non ancora eseguita full CI di release sul SHA finale.
- Sicurezza: gate statici anti-esposizione PostgreSQL 5432, Patroni 8008, etcd 2379/2380 e HAProxy loopback 6432, TLS verificato, guard OSD. CI leggera https://github.com/desalvo/multi-backup/actions/runs/43849085464 PASS. Non sostituisce scansioni reali dall'esterno né failover Patroni.
- RC: ancora necessari DR combinato database+dati su cluster vuoto, real Patroni 3-nodi/split-brain/failback, test sicurezza monitoring REST/SNMPv3 e rete esterna, installer/update/rollback e firme, ultima full Production CI sullo SHA finale, merge ordinato della catena PR. GitHub/Dropbox test-account e hypervisor non qualificati restano fuori da RC limitata.

One-line English commit summary: Harden PostgreSQL WORM leases, certify Rocky 9 Rust RPM installation, replace Python release repackaging with verified Rust artifacts, gate private database networking, and extend guarded Ceph/PVE-PBS acceptance.
