# Multi Backup r213 — laboratori oVirt 4.5 e VMware vSphere

**Data:** 17 novembre 2026. **Branch:** feature/r213-ovirt-vmware-laboratories, impilata sopra r212. Nessun merge su main, nessuna release.

## Architettura dei tre livelli

| Livello | oVirt | VMware | Certificazione |
|---|---|---|---|
| CI GitHub-hosted | Fixture TLS Engine + IMAGEIO, API matching, raw bytes e age | govc/vcsim ufficiali, OVF import e VMDK/age binary roundtrip | Solo codice e protocolli simulati |
| Host self-hosted KVM annidato | VM EL9 Engine e VM EL9 oVirt Host, successiva installazione oVirt 4.5 | VM ESXi 8 installabile da ISO legittimamente fornita; successivo VCSA con credenziali/licenza proprie | Provider software reale, solo una volta installato e testato |
| Provider/HA reale | Engine, nodi KVM e storage isolati, VM usa-e-getta | ESXi/vCenter con diritto all'API di backup VADP/CBT e VM usa-e-getta | Backup/restore completi, boot e dati con prove reali |

**Licensing VMware:** ESXi 8.0 U3e gratuito esiste, ma Broadcom dichiara che non abilita VADP-based backup. Non si può usare quella licenza per certificare live/incremental VADP/CBT. Servono un'installazione licenziata o una valutazione consentita, media acquistati/scaricati legalmente, ed eventuale vCenter. Né ISO né licenze sono archiviate in Git.

**Requisiti stimati:** 64 GiB RAM e NVMe adeguato se i laboratori girano in sequenza. Per simultaneità considerare **96–128 GiB RAM**, **400–600 GiB SSD** liberi e 16–24 thread CPU, con AMD-V/SVM o Intel VT-x abilitati e virtualizzazione annidata. Esempi: Engine 8 GiB/4 vCPU, Host 16 GiB/8 vCPU, ESXi nested 16 GiB/8 vCPU; vCenter richiede RAM e disco supplementari secondo la versione dell'appliance. Queste sono stime per il laboratorio, non un benchmark.

**Isolamento:** rete libvirt mb-r213-isolated senza elemento forward, subnet 192.168.213.0/24. Non collegare la rete ad interfacce fisiche produttive. Per installare RPM in una rete senza routing è necessario un mirror locale, oppure temporaneamente un gateway limitato alle repository attendibili da configurare e rimuovere manualmente.

## File predisposti

- scripts/labs/provision-hypervisor-r213.py: modalità plan non mutante e provision create-only controllata.
- docs/labs/r213-lab-config.example.json: percorsi immagini EL9 qcow2, ISO ESXi, SHA-256 da sostituire, cloud-init, rete.
- docs/labs/r213-isolated-network.xml: libvirt network privata senza forward.
- docs/labs/r213-cloud-init.example.yaml: template senza password, SSH public key da sostituire e hostname separati.
- .github/workflows/hypervisor-nested-lab-r213.yml: controllo hosted di sicurezza e job reale solo tramite workflow_dispatch, environment protetto e runner dedicato.
- scripts/ci/ovirt-https-imageio-fixture-r213.py e scripts/ci/ovirt-https-imageio-r213.sh: API Engine+IMAGEIO su vero TLS, chunk age e 6 errori forzati. Questa è una fixture, NON è oVirt Engine vero.
- .github/workflows/ovirt-protocol-lab-r213.yml: CI del protocollo. Rimanenti: VMware vcsim/OVF e vendor field workflows dalla r212, e oVirt field script r209.

## Procedura sicura su host Linux dedicato (non eseguita nella CI hosted)

1. Installare qemu-kvm, libvirt, virt-install e qemu-img. Verificare CPU AMD-V/VMX, /dev/kvm e nested KVM. Il runner deve essere non-root ma autorizzato a usare libvirt system. Preparare un filesystem temporaneo cifrato dedicato, fuori da produzione.
2. Come amministratore approvato, validare e creare la rete privata libvirt:

    sudo virsh net-define docs/labs/r213-isolated-network.xml
    sudo virsh net-start mb-r213-isolated

   Non configurare forwarding automatico e non usare la rete libvirt default.
3. Fornire due immagini cloud qcow2 Enterprise Linux 9, file cloud-init diversi per Engine e host con permessi 0600, un'immagine ESXi autorizzata solo se richiesta. Calcolare gli SHA-256 reali; sostituire gli zero segnaposto nel JSON. Copiare la configurazione in /etc/multi-backup-ci/r213/config.json con permessi 0600 e proprietà del runner.
4. Creare la directory vuota /var/lib/multi-backup-ci/r213 con permessi 0700 e proprietario runner. Creare manualmente un sentinel root-owned 0600: /etc/multi-backup-ci/hypervisor-lab-r213.allow con contenuto esatto YES_CREATE_ONLY_DISPOSABLE_OVIRT_VMWARE_LAB. Il sentinel non deve essere creato dalla CI.
5. Controllare il piano prima di qualsiasi operazione:

    python3 scripts/labs/provision-hypervisor-r213.py plan --config /etc/multi-backup-ci/r213/config.json

6. Dopo l'approvazione manuale dell'Environment multi-backup-hypervisor-disruptive-lab su runner [self-hosted, linux, multi-backup-hypervisor-lab], eseguire workflow_dispatch con conferma YES_CREATE_ONLY_DISPOSABLE_OVIRT_VMWARE_LAB. Il provisioner verifica hash, sentinel, rete isolata e assenza di nomi e immagini preesistenti. Crea soltanto:
   - mb-r213-ovirt-engine, guest EL9 con 8 GiB;
   - mb-r213-ovirt-host, guest EL9 con 16 GiB e passthrough CPU;
   - mb-r213-esxi8, se enable_esxi=true, guest nested avviato dall'ISO autorizzata.

   Non effettua overwrite, delete, virsh destroy o modifica della rete. Un errore parziale può lasciare VM appena create, da ispezionare manualmente.
7. Completarne l'installazione reale **dentro i guest**: su oVirt Engine EL9 seguire la guida ufficiale oVirt 4.5, di norma abilitare centos-release-ovirt45, installare ovirt-engine e avviare engine-setup; predisporre database, FQDN e DNS; registrare host oVirt, storage dedicato e VM usa-e-getta. Per ESXi completare l'installer interattivo e aggiungere eventualmente vCenter/VCSA con licenza idonea; la VM nested non garantisce la supportabilità di un vero ESXi fisico.
8. Collegare i field test in repository: scripts/ci/ovirt-real-field-r209.sh e scripts/ci/vmware-vcenter-real-field-r212.sh con profili e approvazioni richieste, **mai con credenziali di produzione**.

## Gate per promuovere a READY

**oVirt:** vero Engine con backup READY e imageio, VM con almeno due dischi, checksum indipendenti e checkpoint completo, esportazione cifrata, creazione di nuova VM, ripristino di tutti i dischi e metadati, boot e verifica dati guest. Il percorso esistente nella r212 è B-In di UN disco e non soddisfa ancora questo gate.

**VMware:** backup da vero ESXi/vCenter, OVF/NFC di VM spenta, importazione in VM nuova, boot e hash dati; con licenza compatibile, snapshot live applicativamente consistenti e VADP/CBT incremental e full-chain, cross-host restore e prove negative. Il simulatore vcsim non implementa ExportVm NFC e non può certificarlo.

**HA e fault:** simulare una VM KVM che cade non equivale a test su tre host fisici e fencing indipendente. Serviranno host/runner realmente separati per questo livello.

## Collaudi già verificati

- oVirt HTTPS protocol fixture su Rust: https://github.com/desalvo/multi-backup/actions/runs/44134574398 — PASS. NON provider Engine autentico.
- VMware govc/vcsim + OVF import simulato: https://github.com/desalvo/multi-backup/actions/runs/44095177198 — PASS.
- VMware VMDK qemu-img → age → restore → confronto RAW byte-per-byte: https://github.com/desalvo/multi-backup/actions/runs/44095656045 — PASS.
- VMware real field host guard: https://github.com/desalvo/multi-backup/actions/runs/44091122559 — PASS solo sicurezza; test vCenter reale non eseguito.
- Stato aree r211: docs/PROVIDER_FIELD_CERTIFICATION_R211_IT.md.

Fonti: https://ovirt.org/documentation/installing_ovirt_as_a_standalone_manager_with_local_databases/ ; https://ovirt.org/documentation/planning_and_prerequisites_guide/ ; https://knowledge.broadcom.com/external/article/399823 ; https://github.com/vmware/govmomi

One-line English commit summary: Prepare guarded nested oVirt and VMware VM labs with pinned offline media and validate the native oVirt IMAGEIO TLS protocol.
