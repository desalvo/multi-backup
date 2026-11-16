# Multi Backup r065 — PBS, PVE, VMware, oVirt: requisiti VM/CT

Stato verificato 16 novembre 2026: **PLANNED / NON CERTIFICATI**. Il catalogo Rust contiene `proxmox` (PVE e PBS accorpati), `vmware` e `ovirt`, tutti `backup_input=Planned`, `backup_output=Unsupported`; non esistono implementazioni dedicated Rust né collaudi reali di backup/restore per questi hypervisor. La presente è una specifica, non un rilascio.

## Requisiti obbligatori

Per **PVE, VMware e oVirt**: backup e restore di VM e workload container (CT) sia con guest **in esecuzione** (online/live) sia **spento** (offline/cold), laddove la piattaforma e il runtime possano fornirne la semantica. L'utente configura per ogni job la modalità e la policy: snapshot online crash-consistent, snapshot quiesced/application-consistent con guest agent quando disponibile, oppure stop/shutdown ordinato esplicitamente autorizzato e riavvio controllato. Nessun arresto, downgrade di consistenza o mancato thaw silenzioso.

- **PVE**: supporto VM QEMU e CT LXC nativi, modalità vzdump snapshot/suspend/stop, QEMU Guest Agent, eventuale PBS, snapshot cleanup e restore a **nuovo VMID/CTID**.
- **PBS**: plugin **autonomo** `proxmox-pbs` separato da `proxmox-pve`; sorgente di snapshot esistenti VM/CT/host e destinazione per backup quando supportato; inventario datastore, verifica chunk e archivi, export cifrato verso Multi Backup/vault S3 WORM e restore. Non copiare direttamente file raw dal datastore deduplicato ignorando il protocollo PBS.
- **VMware**: VM vCenter/ESXi via vSphere API/VADP, snapshot online/offline, CBT incremental quando applicabile, quiesce VMware Tools, configuration/disks/network inventory, restore in nuova VM/datastore. VDDK/API sotto diritti e licenze validi, senza bundling non autorizzato.
- **oVirt**: VM running/stopped via Engine REST API e backup/live snapshot API, volumi/storage domain e configurazione, restore in nuova VM, gestione dei dischi non snapshotabili/volumi condivisi.
- **CT su VMware/oVirt**: non assumere equivalenza con CT LXC nativi PVE. Coprire container mediante runtime o agente guest/host autorizzato (Docker/Podman/LXC/Kubernetes, in base alle capacità reali), conservando immagini, volumi, configurazione e riferimenti alla VM/host. Il collaudo container deve essere indipendente da quello VM.

Per tutti: schedulazione, parallelismo controllato, limiti CPU/RAM/rete, full e incrementale ove disponibile, metadati completi, cifratura, checksum, copie WORM, retention, verifiche periodiche, ripristino granularità VM/CT/disco/file se tecnicamente supportato, no-overwrite di default, annullamento sicuro e idempotenza dei task. Rilevare/registrare chiaramente limitazioni e livelli di consistenza; non dichiarare application-consistent senza verifica.

## Certificazione minima prima di READY

1. PVE disposable reale: VM QEMU + CT LXC; backup online/offline, quiesce, full/incremental dove disponibile, restore con nuovi ID, validazione dell'avvio, snapshot cleanup e fault injection.
2. PBS disposable reale: datastore dedicato, snapshot multipli, checksum, acquisizione e restore di VM/CT, isolamento da dati di produzione.
3. VMware vSphere/ESXi lab reale autorizzato: powered-on/off backup, CBT e restore completo, snapshot cleanup; container tramite runtime/guest testato separatamente.
4. oVirt Engine lab reale: snapshot running/stopped, trasferimento dei volumi e restore della VM con nuove identità, fail handling; container via runtime/guest verificato separatamente.
5. Test di dati che cambiano durante il backup, interruzione connessioni, policy consistency, freeze/thaw timeout, autorizzazioni, no-overwrite e ripartenza.
6. Test hypervisor reali *solo* su laboratorio disposable self-hosted autorizzato, non automaticamente a ogni commit GitHub. Nessuno skipped o mock equivale a PASS.

Restano fuori dalla prima RC pronta per produzione finché tali test non passano. Aggiornare catalogo Rust, GUI, documentazione e handoff quando si avvia l'implementazione.

Riferimenti: https://github.com/proxmox/pve-docs/blob/master/vzdump.adoc ; https://pbs.proxmox.com/docs/introduction.html ; https://developer.broadcom.com/xapis/virtual-disk-api/latest/vddkBkupVadp.9.3.html ; https://www.ovirt.org/documentation/administration_guide/
