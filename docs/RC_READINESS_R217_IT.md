# Multi Backup r217 — RC0 utilizzabile: Windows senza GPU e 16 percorsi READY circoscritti

Revisione 18 novembre 2026. Branch feature/r217-win-warp-installable-rc, derivato da r216. PR r217 in bozza. Nessun tag, pubblicazione, merge in main o distribuzione stabile.

## Evidenze accertate

- **r215 interamente verde:** 18/18 suite sul commit 9a009f5f, run https://github.com/desalvo/multi-backup/actions/runs/44202991407; nove plugin READY solo per precise operazioni, bundle CLI+agente Linux.
- **r216:** 23 test verdi; run https://github.com/desalvo/multi-backup/actions/runs/44211992378 concluso FAILURE. Tutte le 9 famiglie di laboratori genuine, server/PG/agent/GUI Linux/Kubernetes, DMG macOS ARM64+Intel e DEB Ubuntu24 risultano verdi; l'installer nativo Inno Windows è compilato e installato, ma il renderer eframe/wgpu 0.30 non riconosce una GPU compatibile sul runner Windows2025: Wgpu(NoSuitableAdapterFound). Non affermare una RC utilizzabile su Windows prima della prova corretta.

## r217: correzione Windows

- GUI tutta Rust/egui, non wrapper Python. Su Windows senza GPU, il backend WGPU preesistente usa esplicitamente adapter fallback=false: la r217 crea un vero device Microsoft D3D12 **WARP** con fallback=true, Device+Queue e passa WgpuSetup::Existing a eframe. Se hardware vero è disponibile mantiene quello; su host senza GPU seleziona WARP. Se neppure WARP funziona l'app fallisce davvero e NON viene promossa la RC.
- Il test windows-warp-r217.yml usa l'installer Inno su Windows2025, attiva forzatamente WARP per collaudare questo percorso, richiede l'avvio del renderer reale e le dieci pagine GUI (inclusa About) più CLI, age e restore Rust. Niente bypass dei controlli renderer o finte finestre verdi.
- Installazioni r217 da verificare sulla medesima source SHA: Ubuntu24 AMD64 .deb (installazione vera, Xvfb/GUI, backup/ripristino age, negazione overwrite e purge); macOS arm64+x86_64 ad-hoc-signed .dmg (app nativa copiata e lanciata); Windows x64 unsigned Inno EXE (installazione silent su directory nuova, vero renderer WARP, GUI pagina-per-pagina). A ciascun pacchetto SOURCE_SHA e checksum; firmware/signature release ufficiale assenti.

## Plugin 16/33 READY — ciascuno solo nello scope indicato

I 13 precedenti: POSIX bytes fermati e restore create-only; MinIO S3 one-object pinned import; CephFS mount/snapshot regular-file content; Ceph RBD snapshot pinned/new image; kernel iSCSI TGT source/new LUN; MySQL/MariaDB InnoDB single DB full; PostgreSQL16 single DB full; Oracle RMAN media .bkp (NON restore DB automatico); MinIO COMPLIANCE WORM due IAM non-admin (NON root-immune); WebDAV HTTPS BasicAuth RO+dati/ciphertext PUT; SSH un file con host key/digest; SSHFS FUSE attested bytes/nuova directory; Lustre Rocky9 vero kernel MDT+OST, byte content / ciphertext.

Tre nuovi candidati r217:
- **NFSv3**: server nfsd reale, client kernel RO/RW su share fissata, fonte attestata e fermata; backup FULL+INC, age, restore su nuovo percorso montato, B-Out solo ciphertext; no NFSv4/Kerberos, ACL/xattr/uid/gid, snapshot online, failover.
- **SMB3/Samba**: server Samba reale con utente autenticato, mount kernel CIFS RO/RW e contenuto file ordinario; FULL+INC, age, restore su nuovo percorso e ciphertext B-Out; no NTFS ACL/ADS/durable handles/snapshot, cluster HA.
- **GlusterFS**: vero servizio glusterd e client FUSE su singolo volume temporaneo; byte FULL+INC, ripristino in nuova directory e archivi ciphertext B-Out; nessuna certificazione GFID, replica/distributed cluster, quorum, brick failure, ACL/xattr/metadata completa.

Il vero test esistente .github/workflows/network-filesystems-live-r042.yml è riutilizzabile tramite workflow_call e contiene cinque matrici: tre provider reali citati più due emulazioni protocollo GPFS CES via normali server NFS/SMB. Le ultime **NON qualificano IBM Storage Scale**, che rimane Preview. Richiedere success sullo stesso SHA, altrimenti i tre nuovi percorsi non diventano certificati.

## Gate r217

- Rust tests che verificano *esattamente* 16 READY e gli altri 17 senza READY, nessuna esecuzione generic adapter differente da POSIX↔POSIX.
- script rc-preflight-r217.py, test negatives, 10 gruppi di laboratori autentici inclusa matrice NFS/SMB/Gluster (e GPFS compat), sicurezza Ruff/Bandit/pip-audit, software completo Rust/server/agent/Postgres/GUI/Kind, quattro installer sullo SHA r217.
- Verificatore crittografico del bundle r217 dopo download dei quattro artefatti; fallire se SHA, checksum, log o un test mancano.
- Solo con TUTTI SUCCESS: artefatto scaricabile CI installable-rc0 completo di manifest e checksum. *Non* una release pubblica, firmata o stabile.

## r217 Ceph operator/CRUSH — guasto CI reale del 18 novembre 2026

- Run #44235174489 sul commit d37d5ecf5bd5c3aee0c604d463e43ad9a5d0d83c: 30/32 job SUCCESS, ma CephFS/RBD FAILURE e fan-in RC conseguentemente DENIED. I quattro installer nativi, NFS/SMB/Gluster e i restanti laboratori hanno superato i job. Non esiste per quel run un bundle RC approvato.
- Diagnosi dal log: il cluster ha realmente MON 3/3 e OSD UP+IN 3/3, ma il pod OSD-2 è stato ricreato dopo il primo riposizionamento CRUSH. Lo status finale ha 89 PG active+clean+remapped e CRUSH con due OSD sul worker2 e nessuno sul worker3: non è una certificazione ammissibile.
- Correzione: il validatore CRUSH ora richiede il container OSD realmente ready+running; la correzione CRUSH può ripetersi al massimo quattro volte dopo nuove attestazioni indipendenti, con distanza minima fra tentativi e vincolo obbligatorio al contesto disposable kind. Si preserva il requisito assoluto dei tre host distinti e tutti i PG active+clean, zero remapped. La CI r217 ora viene attivata anche da modifiche a script/validatori/test Ceph.
- Verifica necessaria: nuova full CI sul SHA definitivo, tutti i job SUCCESS inclusi CephFS/RBD, test negativi e bundle con quattro installer verificati. Fino allora 16/33 READY resta un obiettivo r217, non la promozione automatica di un test fallito.

## Blocchi per READY globale

- Nessuna prova di fault/fencing su **tre host fisici distinti**; quindi infrastruttura di produzione HA non completamente READY.
- VMware reale vCenter/ESXi VADP/CBT con boot VM, oVirt full VM, PVE/PBS VM/CT diretto e test di boot.
- Oracle RAC/ASM/TDE, MySQL GTID/PITR integrato, SAN remoto CHAP/multipath, snapshot Lustre MDT/OST coordinati e ACL/xattr/mode/proprietari completi non certificati.
- Apple Developer ID/notarizzazione, Windows Authenticode, Debian12/13/Rocky/Alma 9/10 RPM DEB signed native matrix, SBOM e rollback validato.
- NESSUNA affermazione che i nuovi provider siano READY prima della full CI r217 verde.

Handoff cumulativo: docs/HANDOFF.md e docs/DEVELOPMENT_STATUS.md aggiornati ad ogni revisione.

Commit one-liner: **Qualify 16 bounded providers and restore GPU-less Windows native rendering through real D3D12 WARP with exact-SHA installable RC gates.**
