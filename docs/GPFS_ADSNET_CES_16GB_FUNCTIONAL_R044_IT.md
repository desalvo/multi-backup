# IBM GPFS + CES con 16 GiB per VM — qualificazione funzionale

**16 novembre 2026.** Obiettivo: prova end-to-end reale di IBM Storage Scale Developer Edition usando 3 nodi RHEL 9.8 da 16 GiB e un client Ubuntu, senza dichiarare conformita hardware CES o misurare performance.

## Prerequisiti verificati dall'operatore

- SSH incrociato con StrictHostKeyChecking: **18/18 PASS**.
- DNS 4 nodi + 2 VIP (01 .45 e 02 .46): **6/6 PASS**. DNS non equivale a VIP gia assegnati da IBM.
- Su tutte le VM RHEL 9.8: running kernel 5.14.0-687.5.3.el9_8.x86_64 e pacchetti kernel-devel esattamente omonimo, gcc, gcc-c++, cpp, ksh, make **installati e verificati**.
- `wipefs --no-act` sul candidato NSD di ogni VM restituisce **nessuna firma visibile**. I candidati sono 01/02: /dev/sdb, 03: /dev/sda; confermare separatamente mapping WWN/seriale con hypervisor prima di ogni scrittura. Nessuna autorizzazione alla formattazione.
- Subscription-manager vincolato alla release 9.8 e repository EUS 9.8 funzionanti. Non aggiornare minor release o kernel automaticamente.

## Interpretazione della RAM

IBM indica come raccomandazione **64 GiB per CES NFS** e **128 GiB quando SMB o piu protocolli sono abilitati**, ma documenta un evento `total_memory_small` classificato TIP, non invariabilmente fatale. Il tentativo su 16 GiB e sensato per esercitare **solo funzionalita** a carico minimo, non per certificare piattaforma hardware supportata. Un servizio potrebbe comunque non avviarsi o andare in OOM: e un risultato diagnostico, non semplicemente una misura di lentezza. Non rimuovere controlli IBM o simulare CES con Samba/NFS generici.

Riferimenti: https://www.ibm.com/docs/en/storage-scale/6.0.0?topic=events-gpfs e https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=storage-scale-faq

## Fasi operative

1. **Core:** acquisire da IBM Developer Edition e toolkit ufficiale per RHEL 9.8, verificare EULA, pacchetti e versione (tutti e tre i nodi stesso livello). Eseguire precheck toolkit senza azioni distruttive. Verificare dispositivi NSD e WWN. Autorizzare separatamente la successiva creazione NSD/GPFS.
2. **Filesystem:** creare un unico GPFS filesystem di prova, un fileset piccolo e un dato di test. Verificare `mmgetstate -a`, `mmlsfs all` e mount autentico `findmnt -t gpfs`. Non usare dati di produzione.
3. **CES NFS:** configurare prima NFSv4 sui nodi 01/02 e un CES VIP tramite toolkit. Definire cesSharedRoot separato, idealmente almeno 4GiB; limitare a un client e un dataset piccolo. Verificare `mmces state show`, `mmces address list` e `mmhealth node show` e lato Ubuntu mount reale NFS RO. Monitorare OOM e pressione memoria prima, durante e dopo i test.
4. **CES SMB:** se NFS e stabile, abilitare SMB3 e autenticazione autorizzata, testare prima con un solo accesso (nessuna prova di performance o failover). Se le risorse non bastano, registrare blocco di avvio/errore/OOM e procedere senza forzare limiti IBM.
5. **Backup/restore Rust:** con codice r042 **prima integrato e approvato su main**, provare da snapshot IBM coerente via CES NFS e SMB: encrypted FULL, verify-chain, file identity, restore nuovo isolato, controllo hash, no-overwrite, error recovery. Il runner CI deve essere esclusivamente nel repo privato `desalvo/multi-backup-labs`, con marker SHA root-owned approvato fuori CI.
6. **Rapporto:** emettere per NFS e SMB stati distinti PASS funzionale, FAIL funzionale, BLOCKED risorse/ambiente, NOT TESTED, con versione IBM/kernels/export e risorse. Non proclamare supporto IBM o performance qualificate.

## Osservabilita CES senza modifiche

Su ognuno dei nodi 01/02, **solo dopo installazione e configurazione IBM**:

~~~bash
bash scripts/gpfs-lab/11-adsnet-observe-ces-lowmem.sh 01  # solo nodo 01
bash scripts/gpfs-lab/11-adsnet-observe-ces-lowmem.sh 02  # solo nodo 02
~~~

Questo comando e read-only: segnala memoria disponibile, pressione, processi con RSS maggiore, servizi IBM, VIP e eventi OOM recenti. Un `total_memory_small` isolato non invalida un test funzionale, mentre servizi unhealthy, perdita VIP, OOM, hash non uguali o ripristino non riuscito lo invalidano.

## Vincoli

- Nessuna creazione NSD, formattazione, installazione IBM o cambiamento di IP viene eseguito dal codice del progetto.
- Non registrare il runner GitHub pubblico sui nodi IBM e non salvare password o chiavi IBM in repository.
- Non confondere snapshot creato e montato RO con prova indipendente di consistenza applicativa.
- Lo scenario non e adatto a produzione, non e una certificazione IBM, e non copre throughput/concorrenza/HA sotto carico.
