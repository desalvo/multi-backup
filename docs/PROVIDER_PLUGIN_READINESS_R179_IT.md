# Multi Backup — sviluppo provider multipiattaforma r179
Data: 17 novembre 2026. Branch feature/r179-provider-output-hardening.

## Priorità e stato reale

I seguenti plugin sono richiesti per la prossima RC. In questa revisione è
implementato un adapter **B-Out cifrato age** verso filesystem Linux pre-montati
in modalità RW: NFS, SMB/CIFS, WebDAV/davfs2, CephFS, GlusterFS, Lustre,
IBM GPFS via NFS/SMB CES compatibilità protocollo, SSHFS. Non viene montato o
configurato automaticamente alcun client remoto. L'adapter richiede una
corrispondenza esatta con il filesystem kernel/FUSE previsto, non accetta
mount RO o locali mascherati, non sovrascrive, pubblica in staging
`.tar.gz.age` e `.sha256`, e verifica **leggendo e decifrando** gli oggetti
creati via mount prima di tornare successo. Un secondo comando B-Out→R-In
legge e verifica l'archivio cifrato remoto e ripristina verso una nuova
directory locale; NON equivale a ripristino diretto nel filesystem remoto.
Questa modalità non promette retention WORM né ACL/xattr/UID/mtime completi.

CLI: `multi-backup-native plugins mounted-backup --output nfs|smb|webdav|cephfs|glusterfs|lustre|gpfs|sshfs --mount /exact/rw --source /readonly/source --filename NEW.tar.gz.age --recipient age1... --identity /keys/age-id [--output-gpfs-access ces-nfs|ces-smb] --json`.
Per lettura: `plugins mounted-restore --input nfs|... --mount /exact/remote --filename NEW.tar.gz.age --identity /keys/age-id --destination /new/local/path [--input-gpfs-access ces-nfs|ces-smb]`.
Richiede GNU age nel PATH, identità privata separata. I secret non sono
trasmessi al server di backup in chiaro; viene inviato solo ciphertext.
NFS/SMB e GPFS protocollo non provano il funzionamento di GPFS nativo IBM.

## Tabella delle direzioni B-In/B-Out richieste e work remaining

| Provider | B-In | B-Out | Limitazioni necessarie per Ready |
|---|---|---|---|
| S3/Ceph RGW/RustFS | Preview pinned version | Preview WORM | B-In Ceph RGW specifico, batch/list, versioned restore-to-S3 |
| Dropbox | Preview incrementale | Preview upload cifrato | B-Out account isolato reale con secret/token, OAuth refresh e restore diretto |
| GitHub | Preview mirror+CI | Unsupported | account reale con git-lfs, Actions, test di creazione nuova repo |
| Ceph RBD | Planned | Planned (no executor) | RBD export-diff, snapshot coherence, new unmapped image, WWID and fencing tests required |
| iSCSI | Planned | Planned | immutabile snapshot LUN, WWID, fencing e LVM cluster preflight reale |
| Proxmox VE | Planned | Planned (no executor) | task+media verification, restore QEMU/LXC and new PVE backup-storage target not yet implemented |
| Proxmox Backup Server | Planned | Planned | datastore export verification, restore CT/VM/host su host di prova |
| Oracle RMAN | Planned | Unsupported | RMAN reale online/offline, ARCHIVELOG, dump age e restore |
| SSH/SCP/SFTP | Preview (singolo file via SSH firmato con SHA256) | Planned | OpenSSH reale in CI con host key pinned, SHA e age chunk stream; SCP/SFTP directory/B-Out restano Planned |
| WebDAV | Preview mounted RO | Preview mounted RW age | server HTTPS RW, ETag/PROPFIND, davfs2 cache coherence |
| NFS | Preview mounted RO | Preview mounted RW age | server NFS v3/v4 con ripristino e HA/corruzione test |
| SMB/Samba | Preview mounted RO | Preview mounted RW age | SMB3 ACL/ADS/consistency oltre a encrypted archive |
| CephFS | Preview mount | Preview mounted RW age | MDS/OSD CephFS real snapshot consistency |
| GlusterFS | Preview mount | Preview mounted RW age | replica/disperse, GFID/xattr consistency + field |
| Lustre | Preview mount | Preview mounted RW age | MDT/OST stripe metadata & server real |
| GPFS CES NFS/SMB | Preview protocol-only | Preview mounted RW age | client IBM CES reale non disponibile; standard protocol-only |
| SSHFS | Preview mount | Preview mounted RW age | SSH host key, agent/quiesce and real RW field |

## Validation gates

Unit tests fail-closed when a POSIX folder pretends to be NFS, SMB, davfs2,
GlusterFS, CephFS, GPFS, Lustre, or SSHFS. Extended the **existing** live
`network-filesystems-live-r042.yml` to exercise five genuine NFS/Samba/
GlusterFS/GPFS-protocol mounts for ciphertext B-Out, digest sidecar, live age
readback, fresh-directory restore, RO rejection and no-overwrite.
Do not call these PASS before the new SHA CI completes.
Vendor-specific CephFS/Lustre/GPFS, block, Oracle, and Proxmox require real
systems; cannot infer from a planner or protocol-compatible test.

The connected Dropbox account can confirm that Dropbox access is available,
but it is personal and MUST NOT be used to create/delete remotely during
CI without explicit isolated-account authorization. The manual-only
`provider=dropbox-output` E2E uses disposable credentials and records evidence.

## RC gate

Do not publish another RC or change existing RC1 artifacts based solely on
this feature-branch update. The branch remains Draft until exact-SHA CI PASS
and the specifically required provider field tests are satisfied. Notarization
is a later Stable concern; rollback installation and production signing remain
separate gates.

One-line English commit summary: Add authenticated mount-bound encrypted backup destinations and verified create-only recovery for NFS, SMB, WebDAV, CephFS, GlusterFS, Lustre, GPFS and SSHFS.


## r180 — ingresso SSH con verifica crittografica

Adapter Rust `ssh_input.rs`: host e utente validati, pinned known_hosts 0600, identità SSH 0600, percorso remoto assoluto con lessico privo di metacaratteri, nessuna shell interpolation, trasferimento in streaming su `ssh -T`. Un SHA-256 e una dimensione fissati dal chiamante vengono controllati **prima** della pubblicazione del manifest age. Il codice rifiuta output incompleti, exit code non zero, host key falsa e chiavi non autorizzate. CLI dedicata `plugins ssh-backup --config OWNER_ONLY.json --root STORE --identity AGE_ID --label NAME`. L'integrazione con il vero OpenSSH server è stata aggiunta al test precedente `ssh-sshfs-live-integration.yml`. SCP e SFTP rimangono test di protocollo, non adapter B-In certificati; nessun B-Out SSH diretto.


## r181 — RBD/PVE B-Out dichiarativi e fix hostkey SSH

I due nuovi B-Out RBD e PVE sono **solo Planned** e non possono eseguire alcuna scrittura. Le condizioni preparatorie includono snapshot RBD e immagine nuova non montata, oppure storage PVE abilitato ai media backup con ricevuta di task **e media** realmente verificati e restore VM/CT non distruttivo. Le capacità non sono equiparate a backup/restore E2E. `ssh_input.rs` usa l'opzione OpenSSH `-o UserKnownHostsFile=...` atomica con known_hosts pin 0600; il primo test CI aveva correttamente fallito per una forma errata dell'argomento, la revisione va verificata sul nuovo SHA.
