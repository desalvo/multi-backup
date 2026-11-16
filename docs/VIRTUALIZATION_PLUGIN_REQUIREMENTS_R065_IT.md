# Multi Backup r065 — PBS, PVE, VMware, oVirt: requisiti VM/CT

Stato verificato 16 novembre 2026: **PLANNED / NON CERTIFICATI**. Il catalogo Rust contiene ID distinti `proxmox` (PVE, ID storico conservato), `pbs`, `vmware` e `ovirt` in stato Planned; PVE/VMware/oVirt non sono destinazioni generiche e PBS è Planned sia come sorgente sia come destinazione; non esistono implementazioni dedicated Rust né collaudi reali di backup/restore per questi hypervisor. La presente è una specifica, non un rilascio.

## Requisiti obbligatori

Per **PVE**, backup e restore di VM QEMU e CT LXC nativi, online/live e offline/cold, nei tipi e nelle modalità supportati. Per **VMware e oVirt**, backup e restore **delle VM** online/live e offline/cold tramite API native. **CT disabilitati per default**: potranno essere aggiunti soltanto se la specifica versione dell'hypervisor supporta ufficialmente quel tipo di CT e le corrispondenti modalità native di backup e restore, con capacità rilevate e collaudate. L'utente configura per ogni job la modalità e la policy: snapshot online crash-consistent, snapshot quiesced/application-consistent con guest agent quando disponibile, oppure stop/shutdown ordinato esplicitamente autorizzato e riavvio controllato. Nessun arresto, downgrade di consistenza o mancato thaw silenzioso.

- **PVE**: supporto VM QEMU e CT LXC nativi, modalità vzdump snapshot/suspend/stop, QEMU Guest Agent, eventuale PBS, snapshot cleanup e restore a **nuovo VMID/CTID**.
- **PBS**: plugin **autonomo** `proxmox-pbs` separato da `proxmox-pve`; sorgente di snapshot esistenti VM/CT/host e destinazione per backup quando supportato; inventario datastore, verifica chunk e archivi, export cifrato verso Multi Backup/vault S3 WORM e restore. Non copiare direttamente file raw dal datastore deduplicato ignorando il protocollo PBS.
- **VMware**: VM vCenter/ESXi via vSphere API/VADP, snapshot online/offline, CBT incremental quando applicabile, quiesce VMware Tools, configuration/disks/network inventory, restore in nuova VM/datastore. VDDK/API sotto diritti e licenze validi, senza bundling non autorizzato.
- **oVirt**: VM running/stopped via Engine REST API e backup/live snapshot API, volumi/storage domain e configurazione, restore in nuova VM, gestione dei dischi non snapshotabili/volumi condivisi.
- **CT su VMware/oVirt**: **non** usare runtime, agenti guest/host, Docker, Podman, LXC o Kubernetes per simulare CT nativi di VMware/oVirt. L'opzione CT sarà nascosta/disabilitata finché l'API ufficiale della versione installata non espone un tipo di CT con modalità esplicite e supportate di backup **e** restore. Ogni combinazione `(versione hypervisor, tipo CT, backup online/offline, restore)` dovrà essere verificata su server reale. In assenza di supporto nativo, il plugin restituisce `unsupported`, senza fallback. Il backup della VM che ospita container rimane un backup della **VM**, non dei singoli CT.

Per tutti: schedulazione, parallelismo controllato, limiti CPU/RAM/rete, full e incrementale ove disponibile, metadati completi, cifratura, checksum, copie WORM, retention, verifiche periodiche, ripristino granularità VM/CT/disco/file se tecnicamente supportato, no-overwrite di default, annullamento sicuro e idempotenza dei task. Rilevare/registrare chiaramente limitazioni e livelli di consistenza; non dichiarare application-consistent senza verifica.

## Certificazione minima prima di READY

1. PVE disposable reale: VM QEMU + CT LXC; backup online/offline, quiesce, full/incremental dove disponibile, restore con nuovi ID, validazione dell'avvio, snapshot cleanup e fault injection.
2. PBS disposable reale: datastore dedicato, snapshot multipli, checksum, acquisizione e restore di VM/CT, isolamento da dati di produzione.
3. VMware vSphere/ESXi lab reale autorizzato: backup **VM** powered-on/off, CBT, restore e snapshot cleanup. CT solo se il prodotto e la versione installata ne supportano API native di backup e restore, nei tipi e nelle modalità effettivamente certificati; altrimenti non disponibili.
4. oVirt Engine lab reale: backup e restore **VM** running/stopped, trasferimento volumi e nuove identità. CT solo per le API native di tipi e modalità effettivamente supportati/certificati nella specifica release oVirt; altrimenti non disponibili.
5. Test di dati che cambiano durante il backup, interruzione connessioni, policy consistency, freeze/thaw timeout, autorizzazioni, no-overwrite e ripartenza.
6. Test hypervisor reali *solo* su laboratorio disposable self-hosted autorizzato, non automaticamente a ogni commit GitHub. Nessuno skipped o mock equivale a PASS.

Restano fuori dalla prima RC pronta per produzione finché tali test non passano. Aggiornare catalogo Rust, GUI, documentazione e handoff quando si avvia l'implementazione.

Riferimenti: https://github.com/proxmox/pve-docs/blob/master/vzdump.adoc ; https://pbs.proxmox.com/docs/introduction.html ; https://developer.broadcom.com/xapis/virtual-disk-api/latest/vddkBkupVadp.9.3.html ; https://www.ovirt.org/documentation/administration_guide/

## Rettifica r066 — CT condizionati alle sole capacità native dell'hypervisor

VMware VADP è documentato per le macchine virtuali; la pagina oVirt sul supporto container è un documento storico di progettazione, non una certificazione di funzionalità corrente né di backup CT. Pertanto **VMware e oVirt restano VM-only** per l'attuale perimetro del plugin. L'implementazione futura deve eseguire capability discovery per versione e resource type ed esporre CT soltanto dopo verifica di API ufficiali e test reali delle **singole modalità online/offline e restore**. Nessun plugin di runtime/container e nessun agente in-guest costituiscono un sostituto del supporto CT nativo.

Riferimenti di verifica: https://developer.broadcom.com/xapis/virtual-disk-api/latest/vddkIntro.3.5.html ; https://www.ovirt.org/develop/release-management/features/integration/container-support.html ; https://ovirt.org/documentation/administration_guide/

## r069 — catalogo separato PVE/PBS senza false funzionalità

Nel catalogo Rust il plugin `proxmox` rappresenta **soltanto PVE** (VM QEMU e CT LXC nativi); `pbs` rappresenta **soltanto Proxmox Backup Server** (datastore, gruppi di snapshot e archivi). L'ID PVE storico resta invariato per compatibilità con le configurazioni esistenti. Entrambi rimangono `Planned` e il dispatcher non può eseguire backup o restore generici: ciò evita di far interpretare l'inventario dei provider come un adapter certificato. L'accesso a PVE non autorizza implicitamente PBS e viceversa. CT VMware/oVirt restano disabilitati.

## r073 — read-only native Rust PVE/PBS inventory (16 Nov 2026)

New `rust/src/virtualization_inventory.rs` supports explicit read-only PVE QEMU/LXC guest discovery and PBS datastore listing over verified HTTPS and scoped API token from owner-only (0600) JSON config. Redirects refused; response bounded to 1 MiB; token redacted from all output. CLI: `multi-backup-native plugins virtualization-inventory --config-json OWNER_ONLY.json --json`. The descriptor for both providers remains **PLANNED, NON-EXECUTABLE**: this API inventory is NOT a VM/CT backup, snapshot, restore, or live-account certification. Offline unit tests verify native guest types, fail-closed protocols and duplicate identities. No hypervisor field run without disposable infrastructure and explicit credentials.
