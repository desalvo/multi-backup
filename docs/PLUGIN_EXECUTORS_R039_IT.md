# Esecutori Rust e plugin di backup/restore — r039+ (IT)

Aggiornato il 15 novembre 2026. Stato: **preview nel ramo PR #11, non RC certificata**. La dichiarazione di un plugin nel catalogo non rende funzionanti backup, restore e tutti i trasporti.

## Percorsi implementati

| Operazione | Sorgente | Destinazione e vincoli |
| --- | --- | --- |
| Backup filesystem | POSIX, NFS, SMB, SSHFS, GPFS CES, **CephFS, GlusterFS, Lustre** | Chunk store POSIX cifrato age/zstd, con parità 8+2. Fonti remote solo su Linux, mount esatto in sola lettura, attestazione snapshot; GPFS richiede ces-nfs/ces-smb |
| Restore filesystem | Chunk store POSIX verificato | Nuova directory POSIX oppure su mount RW esatto NFS, SSHFS, **CephFS, GlusterFS, Lustre**. Mai sovrascrivere |
| Backup AFS dedicato | Volume specifico OpenAFS, vos dump full o incremental, local/SSH agentless | Chunk cifrati age/zstd senza file dump in chiaro, manifest solo dopo exit status 0 del processo vos |
| Restore AFS dedicato | Catena di dump AFS verificata | Nuovo volume in cella/server/partizione espliciti, prima full con overwrite abort, poi incrementali ordinati soltanto sul nuovo volume creato nello stesso job |

Sono **8 sorgenti** e **6 target** filesystem in anteprima nel dispatcher, oltre all'adapter AFS specifico. Non sono 14 provider autonomi certificati: i 32 plugin catalogati e le 682 combinazioni teoriche restano per la maggior parte non eseguibili. Github e Dropbox conservano percorsi Rust dedicati indipendenti.

## Input CephFS, GlusterFS e Lustre

Esempi di CLI (sostituire i path e l'identità age):

~~~bash
multi-backup-native plugins probe --plugin cephfs --mount /mnt/cephfs --json
multi-backup-native plugins backup --input cephfs --output posix --source /mnt/cephfs --root /backup/store --identity /secrets/age-key --label cephfs --source-snapshot-attested --json
multi-backup-native plugins restore --input posix --output lustre --root /backup/store --identity /secrets/age-key --id HASH_VERIFICATO --destination /mnt/lustre/RECOVERY_NUOVO
~~~

Mount ammessi: CephFS di tipo ceph oppure fuse.ceph-fuse; GlusterFS fuse.glusterfs; Lustre lustre. Il programma non monta filesystem né crea snapshot. Verifica l'esatto mountpoint e flag RO per l'input e RW per l'output, ma il flag di attestazione non prova la consistenza delle applicazioni, le ACL/xattr/stripe o il comportamento in failover. Il restore crea una nuova directory figlia diretta del mount RW, usando staging+rename sullo stesso mount. GPFS CES resta per ora fonte di backup, non target restore diretto.

## AFS: dump volume in streaming cifrato

Profilo di input di esempio (volume-job.json):

~~~json
{"id":"afs-job-01","cell":"example.org","volume":"user.original",
 "auth":{"type":"existing_token"},"execution":{"type":"local"},
 "mode":{"kind":"full"}}
~~~

~~~bash
multi-backup-native plugins afs-backup --job-json volume-job.json --root /backup/store --identity /secrets/age-key --label afs-full --max-transfer-bps 20971520 --json
~~~

Per un incrementale usare: mode = {"kind":"incremental","since":"11/15/2026 01:00","parent_backup_id":"HASH_COMPLETO_DEL_PRECEDENTE"} nel JSON. È richiesta la verifica dell'epoch OpenAFS reale da parte dell'operatore: il formato data valido non prova che i dump non abbiano lacune. L'identità cella/volume è vincolata alla catena. Se vos dump fallisce non viene pubblicato alcun manifest; i chunk cifrati non più referenziati saranno eliminabili solo dal GC protetto.

Via SSH agentless, execution può essere: {"type":"ssh_agentless","host":"afs1.example.org","user":"afsbackup","identity":"/secrets/id_ed25519","known_hosts":"/etc/multi-backup/known_hosts","afs_server":false}. Multi Backup disabilita configurazioni SSH arbitrarie, proxy/forwarding e autenticazione password. Per localauth remoto sono richiesti host fileserver attestato e site esplicito. Keytab locale e localauth diretto non sono ancora esecutori completati/certificati. L'agente pull/push deve eseguire un profilo esplicitamente approvato sul server.

## AFS: restore su volume nuovo e conferma esplicita

Profilo target (target-new.json):

~~~json
{"cell":"example.org","volume":"new.recovered","server":"afs1.example.org",
 "partition":"/vicepa","auth":{"type":"existing_token"}}
~~~

~~~bash
multi-backup-native plugins afs-restore-plan --target-json target-new.json --json
multi-backup-native plugins afs-restore-new --target-json target-new.json --root /backup/store --identity /secrets/age-key --id HASH_DI_UN_BACKUP_AFS --confirm-target example.org:afs1.example.org:/vicepa:new.recovered --json
~~~

Il nome nuovo deve essere inesistente, massimo 22 caratteri e non terminare con .backup/.readonly. L'intera catena age e parità viene verificata prima di chiamare vos restore. Full usa overwrite abort (mai sovrascrivere volume preesistente, nessun ID imposto), gli incrementali usano overwrite incremental soltanto dopo il full riuscito nello stesso processo. Se il restore fallisce il volume nuovo parziale deve essere ispezionato e ripulito manualmente: nessuna cancellazione o ripetizione automatica rischiosa.

Test CI: rust/tests/afs_stream_roundtrip.rs esegue CLI Rust, age-keygen reale e processo vos **simulato** con full/incremental, stream di ripristino e rifiuto di conferma errata. Non sostituisce test su cella OpenAFS reale (token, keytab, site, ACL, VLDB, failover, snapshot).

## Resta da completare

Adapter realmente operativi per Google Drive, OneDrive, S3/RGW, WebDAV, Alfresco, ownCloud/Nextcloud, HDFS, RBD e SAN, VM/hypervisor, Kubernetes, PostgreSQL/MySQL/Oracle; ripristino remoto SSH/agent di AFS; gestore keytab, cgroups hard e pieno fencing distribuito. Nessun plugin viene dichiarato pronto in produzione in assenza di veri backup/restore e test negativi.

Vedere anche PLUGIN_MATRIX.md, PLUGIN_API.md, AGENTS_AFS_R039_IT.md, RC_CERTIFICATION_MATRIX.md e DEVELOPMENT_STATUS.md.
