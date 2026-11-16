# Multi Backup r039 — accettazione POSIX, SSH/SCP/SFTP, SSHFS e OpenAFS

## Stato verificabile

- **POSIX:** PASS CI [43678452447](https://github.com/desalvo/multi-backup/actions/runs/43678452447), sul commit 497e20f9. Il test esegue backup FULL e incrementale usando il vero binario Rust e il filesystem del runner, store age/zstd, file binari, Unicode, file vuoti, 10 MiB di dati casuali, selezione di singolo file, rifiuto di overwrite, symlink e FIFO, chiave age errata, integrità della catena e riparazione di un chunk cifrato realmente corrotto mediante Reed–Solomon 8+2. Corretto il difetto preesistente: durante la riparazione gli shard più corti non erano padded alla dimensione della stripe, quindi apparivano falsamente erosi.
- **SSH, SCP, SFTP:** workflow `.github/workflows/ssh-sshfs-live-integration.yml` e script `scripts/test-real-ssh-sshfs.sh`. Una macchina OpenSSH reale avviata su loopback, chiavi Ed25519 e host key pinning, SCP moderno (protocollo SFTP), SCP legacy, SFTP batch, SSH stdout, confronti byte per byte, rifiuto della host key contraffatta e di una chiave client non autorizzata. Questo è un test reale del *trasporto*, non un esecutore Rust `ssh` generico: il catalogo `ssh` resta `Planned`.
- **SSHFS:** lo stesso script `sshfs` monta una directory SFTP su FUSE in sola lettura, richiede esattamente `fuse.sshfs` in `mountinfo` e lo snapshot dichiarato, esegue un vero backup Rust cifrato e ripristina su un secondo mount SSHFS **RW** verso un nuovo percorso. L'esecuzione fallisce se /dev/fuse o i privilegi di mount non sono disponibili; non usa un filesystem locale come sostituto. Il mount è un test di integrazione, non prova di ACL/xattr remoti, riconnessione o snapshot atomico del server.
- **AFS:** il test simulato full+incrementale è PASS, ma il test autentico su GH hosted falliva su openafs-modules-dkms per il kernel 7.0.0-1017-azure. Il workflow `.github/workflows/afs-real-cell.yml` è stato trasferito su runner dedicato esclusivamente manuale. **La certificazione AFS reale resta aperta** fino a un run PASS sullo SHA approvato.
- **Database:** PostgreSQL FULL e WAL PITR, MariaDB FULL e binlog PITR, Galera tre nodi e cross-node PITR già verificati in precedenti workflow verdi; la gestione PITR automatica di produzione, continuità log e retention rimangono funzioni distinte non certificate.

## Come eseguire la cella AFS autentica

1. Predisporre una **VM Ubuntu dedicata e sacrificabile** con kernel supportato da OpenAFS DKMS; verificarlo realmente con `linux-headers-$(uname -r)`, `openafs-modules-dkms` e `modprobe openafs`. Non usare server AFS/Kerberos esistenti o dati reali.
2. Registrare su GitHub un runner self-hosted Linux x86_64 con etichetta `openafs-ci`, utente non root e `sudo -n` autorizzato **solo su questa VM**. Il workflow è soltanto `workflow_dispatch` per impedire esecuzione automatica di PR potenzialmente non attendibili sul runner privilegiato.
3. Sulla VM, dopo aver confermato che sia sacrificabile, creare esplicitamente il sentinel:

```bash
sudo install -d -m 0755 /etc/multi-backup-ci
printf '%s\n' DEDICATED_DISPOSABLE_OPENAFS_CELL_VM |
  sudo tee /etc/multi-backup-ci/allow-disposable-afs >/dev/null
sudo chmod 0644 /etc/multi-backup-ci/allow-disposable-afs
```

4. Da GitHub Actions avviare manualmente `Multi Backup - authenticated OpenAFS Kerberos field certification` selezionando il branch autorizzato e inserendo `expected_sha` (SHA completo a 40 cifre). Il workflow rifiuta altri SHA, runner non self-hosted, assenza del sentinel e VM già usate. Esegue KDC, VLDB, volserver, mount reale, kinit/aklog, `vos dump` FULL+incrementale, catena age, `vos restore` in volume nuovo e confronto dei byte.
5. **Distruggere/ricreare la VM dopo ogni esecuzione.** Non ripristinare il sentinel usato senza ricreazione dell'host. Conservare log e SHA come evidenza per il merge.

## Cosa impedisce attualmente il merge stable della PR #11

La PR #11 è impilata sulla PR #10 (che è ancora draft). Il merge richiede una base r038 approvata, CI verde sullo SHA esatto e prove AFS reali. L'assenza di conflitti Git **non** significa certificazione. Altre verifiche sostanziali per una release **stable**, e non un semplice merge di sviluppo: agenti push/pull e revoca su host separati, interruzioni durante streaming/backup e resume, test Kubernetes con due o più repliche e storage veramente condiviso, fencing fisico dei writer e GC distribuita, TLS/certificati/replay/failover, pacchetti firmati e installazioni/upgrade/rollback, operazioni SSH/SSHFS sotto guasto, ACL/xattr/owner/mtime POSIX e accuratezza PITR continua con GTID/multi-file.

La PR rimane **draft finché i gate obbligatori non sono superati**; non creare tag stable o immagini dichiarate di produzione soltanto perché i test locali sono verdi. Nel frattempo è possibile fare review tecnica dei singoli commit e dei workflow.
