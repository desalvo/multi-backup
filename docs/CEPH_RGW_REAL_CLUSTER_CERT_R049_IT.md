# Multi Backup r049 — Certificazione CI Ceph RGW S3 reale

**Stato: test di integrazione obbligatori da eseguire prima di dichiarare PASS.**
Test CI: `.github/workflows/ceph-rgw-s3-real-r049.yml`

## Ambiente realmente avviato

GitHub-hosted Ubuntu 24.04; Kubernetes kind con 1 control-plane e **3 worker**.
Operator Rook-Ceph **v1.20.9**, daemon Ceph **v20.2.4**.
Tre MON con quorum 3/3, un MGR e **tre OSD BlueStore UP+IN**, collocati
su **tre diversi nodi kind**. Ogni OSD usa esclusivamente un nuovo
disco sparse da 7 GiB e un loop device allestito nel proprio container
effimero. Nessuna scansione/riscrittura dei dischi preesistenti del runner
è autorizzata. Il cluster viene sempre distrutto dopo il collaudo.

Il test interroga **ceph status -f json** e **ceph osd tree -f json**,
rifiutando configurazioni con quorum inferiore a 3, meno di tre
OSD up+in o meno di tre host CRUSH con OSD up. Ciò prova un vero
cluster Ceph multi-nodo (virtuale), non MinIO, un mock S3 o un
gateway S3 isolato privo di RADOS.

Il `CephObjectStore` usa pool metadata/data con replica **3**,
e il vero RADOS Gateway viene raggiunto mediante port-forward interno,
soltanto su 127.0.0.1:9000. Un CephObjectStoreUser effimero genera
le credenziali S3; sono ottenute da Kubernetes Secrets e non stampate
nei log. I test distruttivi sono limitati ai bucket del cluster
effimero; nessun endpoint remoto configurabile è accettato.

## Controlli CI effettivi

1. Convalida di quorum e OSD distribuiti sui tre worker.
2. Creazione bucket reale con Object Lock, versioning Enabled,
   retention COMPLIANCE di default; preflight Rust che li verifica.
3. Rifiuto di un altro bucket privo di Object Lock.
4. Dimostrazione della possibilità di cancellare oggetti normali
   con **le medesime credenziali** (controllo privilegi/endpoint).
5. Backup Rust cifrato age con archivio e manifest separati,
   presenza di versionId e retention COMPLIANCE su entrambi.
6. Verify con identità corretta; verify con chiave errata rifiutato.
7. Restore in directory nuova e confronto byte-per-byte,
   rifiuto della sovrascrittura.
8. DELETE della **versione protetta**, tentato dal proprietario
   del bucket e rifiutato per protezione WORM; l'errore deve
   essere un effettivo AccessDenied/Forbidden/retention e non
   un generico guasto di rete.
9. Richiesta di riduzione della retention rifiutata;
   conferma di COMPLIANCE e data di scadenza invariata.
10. Verifica finale dopo gli attacchi.

**Mai accettare job skipped o server RGW non pronto come certificazione.**
I runner senza risorse per il Ceph effettivo devono produrre FAIL.
Il test è indipendente da quello MinIO, che deve restare verde.

## Confini della certificazione

Il PASS attesta un vero cluster RADOS multi-nodo **virtualizzato sullo
stesso runner**, non resistenza alla perdita fisica di tre macchine
distinte, comportamento della rete di produzione, failover di
un'installazione reale su hardware o protezione da root dello
storage/Ceph cluster. Un amministratore dell'infrastruttura che
distrugga dischi o cancelli il cluster può comunque perdere i dati.

S3 Object Lock protegge le versioni, non necessariamente la
visibilità delle versioni in presenza di delete marker. Il
ripristino dopo delete marker e il catalogo indipendente autenticato
rimangono da certificare separatamente, così come multipart/resume,
HA del server Multi Backup, least-privilege e account test reali.

Il collaudo non va usato su cluster di produzione e non richiede
accesso a cluster dell'utente o chiavi reali. GPFS resta sperimentale.

## Produzione

Il job Ceph è isolato e non pubblica pacchetti. Dopo un PASS
documentato, il Production Gate e i job di release dovranno
richiedere formalmente la prova Ceph **senza** introdurre
automazioni che promuovano il software in seguito a SKIP.

Per un ulteriore livello di certificazione fisica, prevedere
un runner self-hosted con tre nodi distinti e bucket di test
dedicato, senza riutilizzare l'infrastruttura di produzione.
