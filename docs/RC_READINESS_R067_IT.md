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


## r083 — WORM-pinned isolated disaster recovery script hardening (16 Nov 2026)

- Fixed offline `scripts/db-disaster-restore.sh`: pinned S3 manifest versions must be used for **both verify and restore**. The DB variable `MULTI_BACKUP_DR_DB_MANIFEST_VERSION` now reaches restore; the paired data archive supports `MULTI_BACKUP_DR_DATA_MANIFEST_VERSION` on verify and restore. This permits access to retained versions hidden by S3 delete markers when the operator supplies the recorded version IDs.
- Fail closed if the stage root is not operator-owned or is not empty, and reject a data manifest pin without a paired data snapshot. Existing guards still require isolated NEW EMPTY PostgreSQL primary, explicit opt-in, local service stopped, private 0700 staging, and no overwrite.
- Seven isolated shell drill tests verify pinned flags, unpinned mode, empty-root, standby, existing user objects, missing confirmation, and unmatched data receipt; all 7 PASS locally. The lightweight CI policy workflow runs `bash -n` plus the drill on relevant changes.
- **Not a live DR qualification**: these tests use mocked commands. Combined DB+data consistency and genuine pg_dump→age→S3 WORM→pg_restore in an isolated PostgreSQL cluster remain a blocking RC acceptance. Patroni HA / split brain / release signing / final Production Gate also remain outstanding.

One-line English commit summary: Preserve pinned WORM manifest versions across database and data disaster restores, refuse unsafe staging, and add seven isolated RC regression drills.


## r084 — disposable real DB+data disaster-restore acceptance lab (16 Nov 2026)

- Added `scripts/ci/db-disaster-rustfs-live.sh` to the existing **real RustFS S3 WORM** CI suite (reuses its actual disposable RustFS 1.0.3 bucket/container to avoid a second expensive provider spin-up). The test provisions a separate ephemeral PostgreSQL 16 instance and a source/empty target database; streams a real `pg_dump -Fc` into age, commits both the encrypted PostgreSQL dump and associated data archive through the native Rust S3 WORM adapter.
- The test creates unversioned S3 delete markers over **both** retained manifests, invokes the hardened offline DR script with independent pinned `manifest_version` receipts, compares restored SQL rows and files byte-for-byte, and checks refusal of a second restore to a now-populated target. Host-only credentials, loopback-only ports, container cleanup, ephemeral keys and fresh data.
- **Qualification is pending** until the real GitHub Actions job completes successfully on this SHA. A syntactically correct script does NOT count as a successful PostgreSQL/RustFS test. Even PASS will certify only this ephemeral RustFS+PG16 lab, not synchronized production DB+data snapshots, Patroni failover, Ceph RGW, physical failure domains, release signing or cross-OS installers.

One-line English commit summary: Add a real ephemeral PostgreSQL-to-RustFS WORM disaster-restore drill with pinned DB/data receipts and fail-closed repeated-restore checks.


## r085 — real PG16 + RustFS WORM disaster-recovery evidence (16 Nov 2026)

- **PASS on exact functional SHA `aca51a4ed6d126b3e2af7e1c94a8fbfea412995e`**: GitHub Actions https://github.com/desalvo/multi-backup/actions/runs/43866257791 ran real RustFS 1.0.3 S3 Object Lock COMPLIANCE plus ephemeral real PostgreSQL 16. The logs confirm `DB logical restore completed into new empty cluster`, `PASS real isolated PG16 -> age -> RustFS WORM -> pinned DB+data restore`, and final RustFS negative-delete/retention suite PASS. The current lightweight static CI https://github.com/desalvo/multi-backup/actions/runs/43866257960 is also **PASS**.
- Scope of certification: encrypted PostgreSQL logical dump, separate data backup, S3 WORM retained-version recovery after paired manifest delete markers, byte-exact data/row verification and fail-closed repeat restore, all on **one disposable GitHub-hosted runner**, not a live production environment.
- Does NOT certify coordinated transactional point-in-time snapshots of a production DB with an independently running filesystem data backup, server application resume, three-node Patroni/etcd, recovery on distinct failure-domain hosts, Ceph RGW, OS installer signatures or native update/rollback.
- Remaining narrow RC release gates: final SHA full Production Gate, release installation/update/rollback/firma on supported native target platforms, HTTPS monitor token revocation/SNMPv3 and network isolation acceptance, review of PR stacking/mergeability and a documented limited-provider scope. HA RC additionally requires real 3-node primary kill, DCS partition, fencing, synchronous standby promotion, no split brain, node rejoin and client failover.
- Ceph 3-node kind RGW trial https://github.com/desalvo/multi-backup/actions/runs/43848726571 remains **FAIL** despite running MON/OSD/RGW; 89 PG remained remapped and a 320s readiness deadline expired after CRUSH reconciliation. Ceph remains outside limited RC, GPFS experimental and non-blocking; PVE/PBS backup engines, VMware/oVirt, GitHub/Dropbox authenticated field integrations are not certified.

One-line English commit summary: Record real PostgreSQL and RustFS WORM disaster-recovery PASS evidence while keeping HA, Ceph and signed RC publication gated.


## r086–r112 — Ceph RGW and real HA acceptance engineering (16 Nov 2026)

- **Ceph RGW**: r086–r088 repair the Rook OSD pod's *actual* `ROOK_CRUSHMAP_HOSTNAME` as well as its Kubernetes nodeSelector and require three distinct CRUSH hosts, real OSD IDs/devices, 3 MON quorum, 3 OSD UP/IN and all PGs `active+clean` without remapping. The real Rook/Ceph 20.2.4 lab [43868812396](https://github.com/desalvo/multi-backup/actions/runs/43868812396) confirmed three distributed OSDs/89 clean PGs, but the first guard incorrectly read `num_remapped_pgs` from `pgmap` rather than `osdmap`: false-negative. r097 repaired this with negative tests. [43870989879](https://github.com/desalvo/multi-backup/actions/runs/43870989879) again proved 3 MON, 3 real OSD, 89 clean PG, RGW live but was blocked by another conservative false-negative: Ceph 20.2.4 status contains `monmap.num_mons=3` without optional `monmap.mons`. r107 repaired the strict MON count/quorum parser and added compact-format regression tests; cheap CI [43873254293](https://github.com/desalvo/multi-backup/actions/runs/43873254293) PASS. One-shot real Ceph [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) checks genuine RGW Object Lock COMPLIANCE, denied retained-version deletion/shortening and age encrypted pinned manifest rescue after delete marker. **Do not certify Ceph WORM without the final end-to-end job PASS.** Three kind workers share one physical runner, not independent physical failure domains.
- **HAProxy**: r089–r090 add actual HAProxy/TLS three-backend test verifying `GET /primary`, valid server CA/SNI, automatic routing change, no stale write fallback and invalid-SAN refusal. [43869197926](https://github.com/desalvo/multi-backup/actions/runs/43869197926) PASS, but responders intentionally simulate Patroni.
- **Patroni PostgreSQL16 / etcd**: r091–r112 introduce a real disposable three-process PG16/Patroni + three-member mutually authenticated TLS etcd test. Once the loopback certificate included its actual 127.0.0.1 source IP SAN (without disabling verification), real etcd quorum and three genuine PostgreSQL instances with streaming replicas were achieved [43871725242](https://github.com/desalvo/multi-backup/actions/runs/43871725242). The HA release gate remains FAIL: a leader crash happened before DCS publication of a sync candidate or the test incorrectly interpreted PostgreSQL/Patroni readiness. r103–r111 separate elected write primary from DCS-confirmed sync election and accept real `streaming` state, with independent `GET /sync` check; one-shot [43874152163](https://github.com/desalvo/multi-backup/actions/runs/43874152163) still requires final examination. Do not claim failover, rejoin or split-brain fencing certified until the actual code path passes.
- r105/r106 add cheap fail-closed CI regression guards for HTTPS/mTLS/private PostgreSQL/etcd, synchronous strict, one-shot targeted real HA tests, no synthetic PASS, and Python syntax. Lightweight runs [43872963782](https://github.com/desalvo/multi-backup/actions/runs/43872963782) and [43873629288](https://github.com/desalvo/multi-backup/actions/runs/43873629288) PASS.
- Earlier isolated PG16+RustFS WORM database/data disaster restore [43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) remains PASS within its real single-runner profile. Final candidate SHA full Production Gate, native installers/signatures/notarization, monitoring REST HTTPS/SNMPv3, private network field acceptance, HA three-independent-host validation and PR-stack reconciliation **still block an HA RC**. GPFS experimental/nonblocking. PVE/PBS backup engines and VMware/oVirt VM integrations remain planned/unqualified; inventory-only is not backup.

One-line English commit summary: Repair real Ceph CRUSH and compact status attestation, add Patroni HTTPS primary routing and genuine mTLS etcd/PG HA fault-injection gates, and preserve fail-closed RC qualification.


## r115 — GENUINE Ceph RGW S3 WORM multi-node virtual acceptance PASS (16 Nov 2026)

- **CERTIFIED for the EXACT tested Ceph/Rook profile:** [real GitHub Actions #43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605), tested SHA `05c128a57f4050c927d975bf59b390005d0471b0`, **SUCCESS**, including actual Rook 1.20.9 / Ceph 20.2.4, three MON in quorum, 3 real UP/IN OSDs on three distinct virtual kind worker/CRUSH host nodes and 89 placement groups all `active+clean` (zero remapped).
- The very same real S3 RGW account confirms delete of unprotected control object, Ceph S3 bucket versioning and Object Lock COMPLIANCE preflight; Rust CLI age-encrypted backup, authenticated verify and byte-identical restore; denies deletion of retained object *version* and shortening of retention, verifies retention unchanged, rejects wrong age identity and overwrite, creates malicious unversioned manifest delete marker, verifies unpinned lookup fails and pinned manifest version restores ciphertext byte-for-byte. Logs include `CEPH REAL CLUSTER: PASS`, `PASS: real Ceph RGW S3 COMPLIANCE retention, denied deletion, Rust age restore and pinned recovery after delete marker`, and `PASS: real multi-node Ceph RGW S3 native Rust WORM acceptance`.
- **Scope limitation:** 3 virtual kind workers on one physical hosted runner, ephemeral loop-backed OSD devices, not 3 physically independent hosts, server/admin-root compromise certification, sustained multi-TB resilience or Ceph storage fault-domain disaster recovery. This is genuine Ceph RGW S3 interoperability/WORM functional evidence; it must not be described as physical HA certification.
- Remaining RC/HA release blockers are real Patroni failure+rejoin and quorum fencing, full Production Gate on final SHA, signed/notarized native installers, live HTTP REST+SNMPv3/network security acceptance and PR mergeability. GPFS experimental; hypervisor backup/restore and live Dropbox/GitHub accounts unqualified.

One-line English commit summary: Certify genuine three-worker Ceph RGW Object Lock WORM backup, deletion denial and pinned age restore with real CI PASS.


## r116–r121 — Patroni sync-mode fault injection and real SNMPv3 transport acceptance (16 Nov 2026)

- Real Net-SNMP v3 SHA/AES **authPriv** on `127.0.0.1:11611` with owner-only ephemeral credential files, correct scoped read-only view and extend output: actual hosted CI [43875372585](https://github.com/desalvo/multi-backup/actions/runs/43875372585) **PASS**. Actual wrong auth-key rejected; v2c `public` rejected with v3-only configuration; process bound only on loopback. The metrics producer in this isolated protocol test is a synthetic echo replacement, so it does **not** qualify full Rust-native metrics delivery or external SNMPv3 network deployments.
- Patroni real 3-etcd mTLS and 3-PG16 HA [43874152163](https://github.com/desalvo/multi-backup/actions/runs/43874152163) **FAIL** as a complete gate, but the log confirms a pre-crash Patroni DCS-approved synchronous standby and automatic new-primary promotion after the former leader crash. The strict-mode CI originally demanded a new synchronous standby immediately before allowing former-primary rejoin. r114 corrected the acceptance order: verify durable acknowledged row on newly elected primary, reject unsafe writes when no synchronous standby, rejoin old leader and wait for DCS-qualified sync, then test protected writes and DCS majority-loss fencing.
- Subsequent complete real test [43874914217](https://github.com/desalvo/multi-backup/actions/runs/43874914217) **FAIL** during startup due inconsistent role/state representation of the DCS sync standby. r119 now consults actual Patroni `GET /sync` 200 as the authoritative DCS role check, retains PostgreSQL sync-state confirmation before crashing primary, waits for safe initial election and records redacted /cluster/replication diagnostics on a failure. One-shot real run [r120 one-shot run pending confirmation](https://github.com/desalvo/multi-backup/actions/workflows/patroni-etcd-real-ha-r093.yml) is NOT CERTIFIED in this status document until its exact run ID and outcome are checked. Never equate HAProxy mock-responder acceptance with full Patroni/etcd distributed failover.
- Ceph real Rook-Ceph RGW WORM [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) remains PASS on exact SHA and scope in r115; full signed RC and physical-host HA acceptance remain separate.

One-line English commit summary: Qualify real Ceph RGW WORM and SNMPv3 authPriv, and harden genuine Patroni synchronous failover and quorum-fencing acceptance without relaxing release gates.


## r141 — tested VMware/oVirt native read-only VM inventory merged into active RC development

- Integrated the Rust native `hypervisor-inventory` CLI and module from [r137/PR #27](https://github.com/desalvo/multi-backup/pull/27); source [Rust CI #43886521933](https://github.com/desalvo/multi-backup/actions/runs/43886521933) **PASS**, including native core, desktop and server component tests. vCenter `GET /api/vcenter/vm` with session header and oVirt Engine REST v4 `GET /ovirt-engine/api/vms` with Bearer use TLS, 0600 owner-only config, bounded responses, no redirects or mutation. **VM-only inventory**; CT remains disabled and provider-specific live/offline backup+restore needs real field tests. Remote list may be paginated/incomplete; output explicitly says `inventory_complete=false`.
- Consolidated PVE QEMU/LXC native `vzdump` task start/status and PBS vm/ct/host snapshot read-only inventory in r134; both previously passed real Rust unit CI and are in active branch. These are not certified encrypted archives or create-only restores.
- Independent confirmed tests: Ceph RGW Rook 3 virtual workers full S3 COMPLIANCE WORM [43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) PASS; PostgreSQL16/RustFS age+paired DB/data WORM offline disaster restore [43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) PASS; functional 3x actual Patroni PG16 + 3x TLS etcd promotion/strict fencing/rejoin/quorum-loss [43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) PASS, **same ephemeral host** (not 3 distinct physical host/network-fault certification). Integrated real native Rust Prometheus HTTPS/bearer/X.509 [43886988152](https://github.com/desalvo/multi-backup/actions/runs/43886988152), focused Rust [43886988008](https://github.com/desalvo/multi-backup/actions/runs/43886988008), real PostgreSQL [43886987919](https://github.com/desalvo/multi-backup/actions/runs/43886987919) PASS on r138.
- New SNMPv3 with genuine native Rust metric generator requested in r140; its own job result must be checked (earlier SNMPv3 transport-only [43875372585](https://github.com/desalvo/multi-backup/actions/runs/43875372585) PASS). **No release**: final Production Gate has not passed on the latest branch; earlier full workflow [43881879293](https://github.com/desalvo/multi-backup/actions/runs/43881879293) failed on missing Python PyYAML fixed in r127, but a fresh final-sha Production run must confirm.
- Remaining RC gates: true independent 3-host network isolation, split-brain/fencing and failback/rejoin; real HTTPS REST monitor.read token lifecycle and external VLAN TLS; real PVE/PBS backup archive/restore and VMware/oVirt native VM backup/restore, or explicitly excluded from a limited RC; macOS/Windows/Linux native install/update/rollback including signature+notarization; end-to-end Production/security gate on exact final SHA; manual operator acceptance. GPFS suspended and excluded from all RC gates.

One-line English commit summary: Consolidate tested VMware and oVirt native VM inventory and refresh evidence-based release gate inventory.


## r142 — final targeted acceptance ledger and RC gating, 16 November 2026

The latest consolidated source SHA is r141 `8ebfcde4c12f0212111fab462af19ba722db715f`, with **all three Rust focused component jobs PASS** in [#43887673635](https://github.com/desalvo/multi-backup/actions/runs/43887673635): encrypted core, native desktop compile and server tests. The r140 genuine SNMPv3 authPriv + **actual Rust native monitor-snmp-status** acceptance [#43887445516](https://github.com/desalvo/multi-backup/actions/runs/43887445516) is also **PASS**; wrong key and SNMPv2c were rejected on the isolated loopback lab. The earlier SNMP-only test's synthetic metric limitation is now addressed.

Consolidated code-level acceptance:
- Real 3-process Patroni/PG16 + 3-process etcd mTLS strict synchronous leader crash/promotion/rejoin/DCS fencing: [#43883579584](https://github.com/desalvo/multi-backup/actions/runs/43883579584) PASS; single host only
- Rook/Ceph RGW actual 3 virtual Kubernetes worker/OSD+MON WORM COMPLIANCE/negative deletes: [#43873411605](https://github.com/desalvo/multi-backup/actions/runs/43873411605) PASS
- PostgreSQL16 and RustFS WORM age encrypted paired database/data disaster restore: [#43866257791](https://github.com/desalvo/multi-backup/actions/runs/43866257791) PASS
- Native Rust Prometheus HTTPS verified CA + bearer and wrong-token/CA/plaintext rejections on integrated r138: [#43886988152](https://github.com/desalvo/multi-backup/actions/runs/43886988152) PASS
- Native Rust PVE QEMU/LXC single-VM/CT backup task submit/poll and PBS VM/CT/host snapshots: [#43885350815](https://github.com/desalvo/multi-backup/actions/runs/43885350815) PASS, integration r134
- Native Rust VMware and oVirt VM-only read-only inventories: [#43887673635](https://github.com/desalvo/multi-backup/actions/runs/43887673635) PASS, integration r141; no CT claims
- Genuine Net-SNMPv3 with native Rust producer: [#43887445516](https://github.com/desalvo/multi-backup/actions/runs/43887445516) PASS, integration r140

**No complete RC:** entire full Production/security gate still NOT PASS on final SHA (earlier failed environment bootstrap fixed in r127 but no final full run); signed/notarized cross-platform native installers and update/rollback, 3-independent-host PostgreSQL/etcd partition/fencing, real REST monitor.read HTTPS+PG lifecycle, and real native hypervisor backup archive+create-only restore remain open. GPFS suspended and excluded from required RC gates. No main merge, release tag or stable publication occurred.

One-line English commit summary: Record green native Rust/SNMP integration evidence and explicit remaining RC release gates.
