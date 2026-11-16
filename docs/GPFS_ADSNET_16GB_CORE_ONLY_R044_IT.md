# GPFS su VM ADSNET da 16 GiB — piano core-first, 16 novembre 2026

## Risultati osservati (comunicati dall'operatore)

- Tre server RHEL 9.8 con subscription-manager release --show = 9.8; repository EUS BaseOS/AppStream per 9.8 disponibili.
- Kernel runtime uniforme: 5.14.0-687.5.3.el9_8.x86_64.
- Simulazioni dnf --assumeno con 19 nuove installazioni, circa 92 MB download, in un nodo opzionalmente upgrade di libgcc/libgomp/libstdc++ a 11.5.0-15.el9_8; non risulta alcun nuovo pacchetto kernel-runtime, né minor release 9.9.
- Il kernel-devel proposto è EXACT 5.14.0-687.5.3.el9_8; kernel-headers 5.14.0-687.67.1.el9_8 può essere più recente perché non è il modulo kernel-devel compilato per il kernel in esecuzione.
- Gli output terminano con Operation aborted in modo previsto perché la transazione DNF è soltanto simulata. **Nessun pacchetto installato sulla base di questi risultati.**
- L'operatore non dispone di RAM aggiuntiva per le quattro VM già predisposte.

## Installazione selettiva delle dipendenze

Su ciascuna VM RHEL 9.8, esaminata la simulazione e verificato che nessun pacchetto kernel-runtime/OS 9.9 sia introdotto:

~~~bash
sudo dnf install "kernel-devel-$(uname -r)" gcc gcc-c++ cpp ksh make
rpm -q "kernel-devel-$(uname -r)" gcc gcc-c++ cpp ksh make
bash scripts/gpfs-lab/09-adsnet-dependency-nsd-audit.sh 01 # solo VM 01
~~~

Sulla 02 usare parametro 02, sulla 03 parametro 03. Il comando install non è stato eseguito da ChatGPT e NON deve essere interpretato come autorizzazione a effettuare aggiornamenti indiscriminati. Alcune librerie GCC possono ricevere una revisione el9_8 (normale). Non aggiornare il kernel e non eseguire dnf upgrade senza analisi delle versioni. Se mancano specifici RPM coerenti con il kernel attivo, fermarsi: non sostituire silenziosamente il kernel.

## Limiti hardware e priorità

IBM Storage Scale FAQ distingue i requisiti GPFS base x86 (memoria molto inferiore a 16 GiB) e CES. CES richiede almeno 64 GiB per NFS-only e 128 GiB per SMB o protocolli multipli, più altri requisiti CPU/socket/rete. Le VM ADSNET hanno 16 GiB; **non sono idonee alla qualifica hardware IBM CES**. Non chiedere ulteriore RAM all'operatore.

**Piano A — eseguibile con RAM attuale**
1. Verificare disco dati candidato *per ogni VM* (01 /dev/sdb, 02 /dev/sdb, 03 /dev/sda) e firme senza scrivere; verificare seriali/WWN nel virtualizzatore e licenza esatta IBM Developer Edition.
2. Scaricare da IBM i pacchetti ufficiali per la release Developer Edition e usare toolkit IBM per preparare tre core node, senza designare CES in questa prima fase. Cluster, quorum, NSD, un GPFS filesystem e snapshot di un dataset piccolo dedicato; tool ufficiale come unica sorgente di provisioning.
3. Eseguire una prova Rust create-only cifrata su **un mount autentico FSTYPE gpfs** verificato localmente, manifest, SHA256/verifica catena e ripristino locale isolato. Per ora il provider r042 non è integrato in main, perciò nessun PASS e nessuna esecuzione automatica. La prova non pretende di certificare GPFS via CES.
4. Dopo che GPFS core e il filesystem di prova sono stati installati, l'operatore può eseguire solo localmente il controllo read-only `bash scripts/gpfs-lab/10-adsnet-observe-gpfs-core.sh`, che richiede mmlscluster/mmgetstate/mmlsnsd/mmlsfs e un mount locale FSTYPE=gpfs autentico. Per il momento manca un PASS hardware. Poi aggiungere fault injection / nodo NSD non critico, snapshot consistency, ACL/xattr/UID/GID e recheck recupero senza accedere ai dischi di sistema.

**Piano B — eventuale prova CES non qualificante**
Solo se le policy IBM e le risorse reali consentono di installare/avviare CES in modo controllato, tentare NFS di sola lettura su dataset piccolo, con ridotto carico; registrare in evidenza l'hardware sottodimensionato e non pubblicare certificazioni CES supportate. SMB e failover fisico non sono obbligatori per chiudere la qualifica GPFS core; test NFS/SMB generici già superati con server indipendenti in CI. Se CES rifiuta l'avvio, fermarsi senza modificare i limiti richiesti da IBM o forzare parametri di memoria.

## Da non confondere

- Il nome di dominio VIP CES risolto non significa che il servizio CES sia attivo.
- Un semplice NFS, SMB o Samba montato da un nodo Linux non certifica IBM GPFS né CES.
- I test locali GPFS native non qualificano automaticamente l'adapter GPFS-CES del catalogo r042.
- Il repository privato desalvo/multi-backup-labs può coordinare test in futuro ma non ha ancora evidenze GPFS IBM autentiche e NON deve ricevere root private SSH keys.

Fonti IBM:
- https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=storage-scale-faq
- https://www.ibm.com/docs/en/storage-scale/6.0.0?topic=gpfs-software-requirements
- https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=reference-mmbuildgpl-command
- https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=overview-storage-scale-product-editions
