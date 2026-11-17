# Multi Backup r212 — VMware offline OVF, credenziali, CI e field acceptance

**Data:** 17 novembre 2026. Branch: `feature/r212-vmware-native-provider-and-field-gates`, base r211. **NON fare merge o release** prima dei gate reali. La prova `vcsim` è un simulatore dell'API vSphere, **non** un ESXi/vCenter fisico.

## Funzionalità implementate
- Rust `rust/src/vmware_ovf.rs`: due modalità `export` e `import` con profili **diversi** e separazione delle credenziali.
- Export **offline** della VM `poweredOff` identificata da path d'inventario e MoRef `vm-123` fissi; verifiche identità/stato prima, dopo e al momento di pubblicare il backup; uso del vero `govc export.ovf` da `govmomi`. Niente snapshot automatici, VADP o CBT fittizi.
- Pacchetto OVF e VMDK esportato su **staging locale plaintext temporaneo**. I file devono essere regolari, non symlink, nomi/profili limitati, un solo .ovf, almeno un .vmdk; quindi **stream tar diretto** nello storage age chunk/parity del software, senza un secondo archivio tar plaintext sul disco. Verifica dello storage cifrato prima di confermare successo.
- Restore create-only: verifica autentica dei chunk age, ripristino tar in una directory temporanea, verifica di ciascun membro (nessun symlink, hardlink, path traversal, duplicate, oversize), prova di assenza della VM destinazione, `govc import.ovf` verso datastore e resource pool scelti; verifica nuova VM in stato spento. **Mai --force, sovrascrittura, autocancellazione o accensione implicita.**
- Eseguibile govc pinning SHA256, percorso assoluto regular-file no-symlink, credenziali JSON soltanto da file owner-only, variabili `GOVC_*` e proxy ambient ereditati esclusi dal processo, TLS verifica CA senza `GOVC_INSECURE`, nessuna password in argv o log.

## Limitazione di sicurezza importante: file OVF temporanei
`govc export.ovf` deve creare .ovf/.vmdk **in chiaro** prima del caricamento age: lo staging deve essere su **volume locale cifrato** (LUKS/FileVault/BitLocker), directory padre già esistente e permessi 0700, quota sufficiente a contenere l'intera VM esportata; dopo chiusura il software rimuove i file temporanei. L'eliminazione logica **NON garantisce secure erase** su SSD o snapshot del filesystem. La separazione dal Vault WORM rimane necessaria. Il profilo vCenter NON include le chiavi di decrittazione dei vecchi backup oltre all'identità age fornita dalla recovery.

## Esempi di profilo (sostituire valori con segreti generati e realmente protetti)
Percorso `/etc/multi-backup/vmware/export.json` (0600), utente ESXi/vCenter **solo export**:
```json
{
  "role": "export",
  "endpoint": "https://vc.lab.example",
  "username": "mb-export@vsphere.local",
  "password": "REPLACE_FROM_PRIVATE_SECRET_MANAGER",
  "tls_ca_file": "/etc/multi-backup/vcenter-root-ca.pem",
  "govc_binary": "/usr/local/bin/govc",
  "govc_sha256": "REPLACE_WITH_64_HEX_DIGEST_OF_THE_REAL_BINARY",
  "datacenter": "DC0",
  "vm_path": "/DC0/vm/mb-lab-source",
  "vm_moref": "vm-42"
}
```
Percorso `/etc/multi-backup/vmware/import.json` (0600), utente vCenter **solo creazione nuovi VM**:
```json
{
  "role": "import",
  "endpoint": "https://vc.lab.example",
  "username": "mb-import@vsphere.local",
  "password": "REPLACE_FROM_PRIVATE_SECRET_MANAGER",
  "tls_ca_file": "/etc/multi-backup/vcenter-root-ca.pem",
  "govc_binary": "/usr/local/bin/govc",
  "govc_sha256": "REPLACE_WITH_64_HEX_DIGEST_OF_THE_REAL_BINARY",
  "datacenter": "DC0",
  "destination_folder": "/DC0/vm/mb-lab-restore",
  "datastore": "mb-lab-datastore",
  "resource_pool": "/DC0/host/Cluster-Lab/Resources"
}
```
I segreti sopra sono **placeholder**. Con `--profile-json` in un file 0600 e staging su FS cifrato 0700, eseguire:
```bash
multi-backup-native plugins vmware-ovf-backup \
  --profile-json /etc/multi-backup/vmware/export.json \
  --root /var/lib/multi-backup/encrypted-store \
  --identity /etc/multi-backup/age-private.key \
  --stage-root /var/lib/multi-backup/encrypted-stage \
  --label "VMware offline full" --json

multi-backup-native plugins vmware-ovf-restore-new \
  --profile-json /etc/multi-backup/vmware/import.json \
  --root /var/lib/multi-backup/encrypted-store \
  --identity /etc/multi-backup/age-private.key \
  --id BACKUP_ID --stage-root /var/lib/multi-backup/encrypted-stage \
  --new-vm mb-lab-restore-NEW \
  --confirm-new-vm vmware:mb-lab-restore-NEW:mb-lab-datastore --json
```
**Non usare** le credenziali admin vCenter e non usare VM di produzione durante i field test. La variabile di profilo `role` limita il processo Rust; solo le ACL reali del servizio VMware possono dimostrare che un account sia davvero escluso dall'altra azione. Tali prove negative provider-side restano necessarie.

## CI e gate VMware
- Hosted: `.github/workflows/vmware-govc-vcsim-r212.yml` ricompila **govc e vcsim** dalla release taggato `govmomi v0.56.2`, prepara una CA e un certificato SAN IP, conferma TLS e passa per backup encrypted OVF+VMDK / restore nuova VM con `age`, collisioni/chiavi/ruoli/VM accesa rifiutati. **SIMULATORE**.
- Manuale: `.github/workflows/vmware-vcenter-real-field-r212.yml` richiede runner isolato `multi-backup-vmware-field`, GitHub environment approvato `multi-backup-vmware-disruptive-lab`, input `YES_ISOLATED_VMWARE_NEW_VM`, sentinel `/etc/multi-backup-ci/vmware-field/allow-disposable-vmware` con testo `DEDICATED_VMWARE_TEST_VM_ONLY`, profili protetti distinti e storage test. Crea una VM nome unico e **la lascia spenta**. Nessun cleanup automatico distruttivo.
- Test guest boot e contenuto, disponibilità in caso di host/datastore fault, VMware VADP/CBT incrementali a caldo, backup sotto modifica concorrente, vSphere HA e licenza/API reali **NON COPERTI**. Un `PASS` di vcsim non rende VMware READY.

## Stato macro-aree precedenti r211
Rimangono valide le evidenze puntuali in `docs/PROVIDER_FIELD_CERTIFICATION_R211_IT.md`. Non modificare a READY generale:
- Vault IAM: PASS MinIO due policy effettivamente distinte, fisico/root fault non provato.
- Oracle Free PDB/CDB/PITR, CephFS snapshot concorrenti + ACL/xattr, RBD nuovo snapshot, iSCSI nuova LUN, PostgreSQL/MariaDB/MySQL/Galera percorsi testati in CI sono **Collaudati funzionalmente** ai soli perimetri verificati.
- PVE/PBS/oVirt: real hypervisor provider restore non verificato.
- Lustre: backup/restore kernel QEMU positivo; snapshot coordinati/failure-domain non certificati.
- VMware: offline OVF rust adapter in collaudo, field fisico non eseguito; VADP/CBT non implementato.

**One-line English commit summary:** Add pinned-TLS VMware OVF encrypted full backup and new-VM restore with split principals, official simulator acceptance and protected real-vCenter field gates.

## r212.2 — import OFFLINE indipendente, compatibilità dell'API simulata
L'API `vcsim` upstream govmomi **non implementa** `ExportVm`/lease NFC, quindi la CI simulator non può certificare un export vCenter. In compenso `plugins vmware-ovf-package-backup` accetta una directory privata già esportata con OVF, VMDK e manifest, e richiede un'impronta **indipendente** esadecimale SHA-256 calcolata sui nomi ordinati, delimitatore NUL, lunghezza file LE64 e SHA256 del contenuto per ciascun membro. Verifica la stessa impronta PRIMA e DOPO il trasferimento age, rifiuta i membri non regolari/unsafe e non pubblica un backup se i file cambiano. La ricevuta dichiara sempre `source_provenance_attested=false`: questa strada NON attesta l'origine da una VM VMware reale. Il vero `govc export.ovf` su vCenter resta nel workflow manuale sul provider autenticato e nel test del codice di export.

La CI `vmware-govc-vcsim-r212.yml` compila `govc` e `vcsim` dallo stesso tag ufficiale `v0.56.2`, imposta una CA reale di test, genera un piccolo VMDK streamOptimized con `qemu-img` e OVF associato, verifica fingerprint falso, salvataggio age, restore importazione nuova VM su simulator, collisione, age key errata, ruoli e diniego sul backup della VM accesa. Il test automatico non sostituisce boot/content su vero ESXi.

### Comando input pacchetto OFFLINE
```bash
multi-backup-native plugins vmware-ovf-package-backup \
  --root /var/lib/multi-backup/encrypted-store \
  --identity /etc/multi-backup/age-private.key \
  --stage-root /var/lib/multi-backup/encrypted-stage \
  --source-ovf-dir /var/lib/multi-backup/encrypted-stage/pre-exported-vm \
  --source-moref vm-42 --expected-fingerprint ACTUAL_SHA256_64_HEX \
  --label "Offline OVF import without source provenance" --json
```
