# Multi Backup — laboratorio IBM Storage Scale Developer Edition / GPFS CES (r042)

**Stato:** kit di preparazione isolato su main; l'esecutore Rust GPFS e il workflow di prova su server IBM rimangono nella PR Draft #14 (r042), non ancora certificati.

**Stato:** da predisporre. Nessun test GPFS autentico è PASS prima che la CI dedicata abbia eseguito backup e restore sul cluster. Il plugin Rust gpfs usa il client Linux NFSv4 o SMB3 verso **IBM Cluster Export Services**, senza includere un client GPFS proprietario.

## Licenza

IBM documenta Developer Edition per valutazione e test, **non produzione**, fino a 12 TB per cluster, su **RHEL x86_64**, senza supporto IBM. Il toolkit di installazione è disponibile per Developer Edition. Prima di scaricare/avviare, leggere e accettare i termini applicabili all'esatta versione. Non distribuire codice/pacchetti IBM, EULA, licenze o credenziali in GitHub, Docker, archivi o installer pubblici.

Fonti ufficiali:
- https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=storage-scale-faq
- https://www.ibm.com/docs/en/storage-scale/6.0.0?topic=overview-storage-scale-product-editions
- https://www.ibm.com/docs/en/storage-scale/5.2.3?topic=services-configuring-enabling-smb-nfs-s3-protocol
- https://www.ibm.com/docs/en/storage-scale/6.0.0?topic=protocols-installation-prerequisites

## Macchine virtuali (dimensionamento raccomandato)

| VM | OS | CPU | RAM | Dischi |
|---|---|---:|---:|---|
| mb-gpfs-01 | RHEL 9.x x86_64 supportato da IBM | 6 vCPU | 16 GiB | 80 GiB sistema + 150 GiB NSD |
| mb-gpfs-02 | RHEL 9.x x86_64 supportato da IBM | 6 vCPU | 16 GiB | 80 GiB sistema + 150 GiB NSD |
| mb-gpfs-03 | RHEL 9.x x86_64 supportato da IBM | 4 vCPU | 12 GiB | 80 GiB sistema + 150 GiB NSD |
| mb-gpfs-ci | Ubuntu 24.04 x86_64 | 4 vCPU | 8 GiB | 80 GiB sistema |

Questi valori sono raccomandazioni di laboratorio, non minimi IBM. I tre nodi IBM formano cluster/quorum e possono ospitare NSD; 01 e 02 sono i **due nodi CES** per NFS/SMB e failover, 03 partecipa al quorum. NSD devono essere dischi separati dai dischi OS, nuovi e senza dati. La topologia e replica NSD 2 per dati/metadati si definiscono col toolkit IBM dopo aver verificato fault domain. Il budget <1 TB è sotto il limite Developer Edition di 12 TB.

Usare solo rete privata dedicata. Esempio: 10.77.42.11/12/13 per Storage Scale, 10.77.42.20 per CI, **due VIP CES** 10.77.42.30/31 distinte dagli indirizzi cluster. DNS forward/reverse stabili, sincronizzazione chrony, firewall e porte esatte della versione IBM; NFS/SMB NON pubblici. Le VM persistono tra CI; il runner è separato dal cluster.

## Installazione del software IBM (solo amministratore)

1. Installare RHEL con subscription, repo BaseOS/AppStream, kernel supportato dalla specifica IBM release (non assumere che tutte le minor RHEL 9 siano supportate), SSH amministrativo ristretto, DNS/chrony. Inventariare fisicamente i dischi NSD. Su ciascun nodo eseguire preflight read-only: bash scripts/gpfs-lab/00-preflight-rhel.sh.
2. Scaricare Developer Edition da IBM e verificare hash/firma, termini/entitlement. Usare il **toolkit IBM ufficiale** per distribuire i pacchetti su tre nodi, predisporre CCR, cluster/quorum, tre NSD, filesystem dedicato e fileset di test. Non includere binari IBM nel repository. Non eseguire mmcrnsd o mmcrfs su dispositivi non identificati.
3. Configurare un cesSharedRoot su GPFS separato (IBM suggerisce almeno 4 GiB), designare nodi CES 01/02 con le licenze/server designation richieste e le due VIP private, autenticazione mmuserauth, ACL e protocol packages della versione. Abilitare servizi CES NFS e SMB mediante toolkit e strumenti IBM, verificare HEALTHY.
4. Creare un fileset isolato e i dati con: MB_GPFS_FIXTURE_APPROVAL=CREATE-DISPOSABLE-R042-FIXTURE bash scripts/gpfs-lab/01-create-test-fileset-fixture.sh /gpfs/mbci/mb-r042-fixture01 . Il path NON deve esistere. Un amministratore crea quindi **snapshot GPFS reale** coerente del fileset e configura export NFSv4 e SMB3 read-only il cui mount esponga alla radice MB_EXPLICIT_TEST_FIXTURE_R042, test.bin e test.sha256. Non basta un mount RO su un dataset in scrittura.
5. Separatamente predisporre un piccolo mount scratch RW di prova con file marker MB_EXPLICIT_RECOVERY_TARGET_R042, senza dati di produzione. Il restore GPFS diretto non è ancora implementato: questo target è solo guardrail di CI.
6. Sui CES eseguire: bash scripts/gpfs-lab/02-audit-ces-server.sh. Attestare manualmente cluster ID, server CES, export, fileset, snapshot, versione e usage licenza. Trasferire sul runner il rapporto ridotto in /etc/multi-backup-ci/gpfs-ces-export-evidence.txt (root owner, 0644, deve contenere IBM_STORAGE_SCALE_CES_LAB), senza credenziali.

## Runner e CI

Sul runner installare Rust, Git, age/age-keygen, jq, nfs-common, cifs-utils e coreutils. Montare tramite VIP CES lo **stesso snapshot GPFS** in due percorsi differenti: NFSv4 RO /mnt/mb-gpfs-nfs-ro e SMB3 RO /mnt/mb-gpfs-smb-ro. Le credenziali SMB vanno in un file root-owned 0600 locale, fuori GitHub. Montare il target scratch RW isolato. Non esporre nessun portale IBM al runner.

- Preflight NFS: bash scripts/gpfs-lab/03-runner-preflight.sh /mnt/mb-gpfs-nfs-ro /mnt/mb-gpfs-scratch ces-nfs
- Preflight SMB: bash scripts/gpfs-lab/03-runner-preflight.sh /mnt/mb-gpfs-smb-ro /mnt/mb-gpfs-scratch ces-smb
- Solo dopo, autorizzare esplicitamente il runner con: sudo env MB_GPFS_LAB_APPROVAL=APPROVED-DISPOSABLE-R042-ONLY bash scripts/gpfs-lab/04-authorize-lab-host.sh

**Sicurezza obbligatoria:** creare un repository GitHub **PRIVATO e separato**, ad esempio desalvo/multi-backup-labs, senza pull request esterne; registrare la VM runner ESCLUSIVAMENTE in quel repository, **mai nel repository pubblico desalvo/multi-backup**. GitHub avverte che i runner self-hosted associati a repository pubblici possono essere compromessi da workflow di PR non fidate, anche in presenza di Environment approvati: https://docs.github.com/en/actions/reference/security/secure-use . Usare etichette self-hosted e gpfs-ci nel repository privato. L'Environment privato gpfs-field-lab deve avere un reviewer obbligatorio e policy solo main. La workflow è soltanto manuale e deve fare checkout del commit SHA ESATTO corrispondente al main pubblico approvato, dopo averlo verificato; non deve mai eseguire sorgenti di PR o hash arbitrari. Il template completo, nella PR Draft #14, è scripts/gpfs-lab/PRIVATE_WORKFLOW_TEMPLATE.yml: copiarlo, dopo review, nella cartella .github/workflows/ del repository PRIVATO. La workflow nel repository pubblico esegue soltanto lint su runner GitHub-hosted e non esegue alcun job GPFS proprietario. Impostare nell'Environment privato MB_GPFS_NFS_RO_MOUNT e MB_GPFS_SMB_RO_MOUNT (distinti), MB_GPFS_SCRATCH_RW_MOUNT, MB_GPFS_SNAPSHOT_ID. La CI configura anche MB_FIELD_SNAPSHOT_ATTESTATION=COHERENT_READONLY_SNAPSHOT_R042 e MB_GPFS_CES_BACKEND_ATTESTED=IBM_STORAGE_SCALE_CES_LAB, ma NON sostituisce le prove indipendenti CES e snapshot. Nessun token IBM è consegnato alla CI.

## Test / limiti

FULL snapshot CES NFS e SMB in sola lettura -> chunk cifrati age/zstd -> verifica catena -> restore locale nuovo -> sha256/cmp; rifiuto di mount falsificato, snapshot non attestato, identità non autorizzata, overwrite e mismatch. Dopo questa prima certificazione servono ACL NFSv4/SMB, xattr, UID/GID/mtime, metadata IBM fileset/ILM, restore diretto GPFS su nuovo target, veri failover CES/NSD e tolleranza alle interruzioni. Il test di NFS su un server ordinario NON certifica GPFS. Il plugin resta preview finché il report di campo non conferma l'origine IBM.

Nessuna release o merge automatico. La Developer Edition non va usata per backup aziendali di produzione.
