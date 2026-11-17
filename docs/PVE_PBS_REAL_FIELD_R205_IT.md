# PVE / PBS — laboratorio reale B-In, restore VM/CT e lettura VM disk

Questa procedura valida **server PVE e PBS autentici**. Non usa risposte simulate, non sostituisce PBS con un filesystem locale e non certifica VM o CT quando viene superata soltanto la compilazione Rust.

## Ambiente autorizzato

- Runner Linux **self-hosted**, sul nodo PVE di laboratorio, etichettato `self-hosted, linux, multi-backup-pve-pbs-field`, con ambiente GitHub protetto `multi-backup-pve-pbs-disruptive-lab`, approvazione manuale e possibilità di `sudo -n` su comandi PVE selezionati.
- Un PVE reale (con QEMU, LXC, `pvesh`, `pvesm`, `qmrestore` e `pct`), un PBS reale e lo storage PBS già aggiunto al nodo PVE. Il client ufficiale `proxmox-backup-client` deve essere disponibile sul runner.
- Due sorgenti piccole **di solo laboratorio**, una VM e un CT, con il tag `mb-field-fixture`. Non indicare VMID di produzione. Nuovi VMID diversi e non assegnati devono essere riservati al solo esperimento.
- File `/etc/multi-backup/pve-pbs-field-lab.json` root-owned, group-readable (`0640`) al runner dedicato, contenente **solo nomi, IDs e percorsi di altri file protetti**, non segreti:
```json
{
  "node": "pve-lab-01",
  "source_vm_id": 1100,
  "source_ct_id": 1101,
  "new_vm_id": 2100,
  "new_ct_id": 2101,
  "backup_storage": "local",
  "restore_storage": "local-lvm",
  "pbs_storage": "pbs-ci",
  "pve_api_config": "/etc/multi-backup/pve-api-owner-only.json",
  "pbs_source_config": "/etc/multi-backup/pbs-vm-disk-owner-only.json",
  "pbs_vm": {
    "source_vmid": 1200,
    "new_vmid": 2200,
    "time": "2026-11-17T07:30:00Z"
  },
  "pbs_ct": {
    "source_vmid": 1201,
    "new_vmid": 2201,
    "time": "2026-11-17T07:30:00Z"
  }
}
```
Valori inventati di esempio: scegliere gli identificativi e l'orario effettivi dei backup presenti in PBS. I file PVE/PBS contenenti token/identità/password devono essere protetti e **non** inseriti nel repository o nei log.

## Workflow e risultati richiesti

Da GitHub Actions lanciare manualmente `.github/workflows/pve-pbs-live-field-r205.yml`, compilando il parametro di conferma **`YES_ISOLATED_PVE_PBS_TEST_GUESTS`**. Il job eseguibile non può partire su un runner GitHub pubblico e richiede l'approvazione dell'ambiente protetto.

Il test verifica in sequenza: `vzdump` QEMU e LXC veri in modalità snapshot e task UPID conclusi `OK`; inventario e SHA-256 dei media reali; importazione in age chunks mediante i plugin Rust; `qmrestore` e `pct restore` **senza force** verso nuovi VMID; lettura via client PBS autentico e restore offline dell'immagine VM cifrata con verifica SHA-256; infine restore nativo da backup PBS PVE (VM e CT) verso altri nuovi VMID.

Il job lascia intenzionalmente **fermi** i guest ricreati. La semplice presenza di una configurazione PVE dopo il restore **non prova il boot né la consistenza applicativa**. Prima di marcare il requisito READY servono avvio in rete isolata, verifiche del filesystem e dati applicativi nei guest e controllo dell'identità/ACL, seguito da cancellazione **manuale soltanto delle fixture**, mai degli originali.

Il restore PBS→PVE tramite ID di backup nativo usa lo storage PBS già configurato in PVE; **non corrisponde** al restore completo da un semplice disco `.img.fidx` salvato negli age chunks di Multi Backup. Quest'ultimo rimane da completare con metadata di configurazione, dischi multipli, agenti e verifica del guest.

### Stato

La validazione su runner hosted controlla solo sintassi e rifiuto di esecuzioni non autorizzate. **Senza il runner del laboratorio e i fixture autentici non esiste un certificato di restore provider end-to-end.**

One-line English commit summary: Add protected real PVE/PBS provider acceptance for native online vzdump, encrypted media import and create-only VM/CT recovery.
