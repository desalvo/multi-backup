# Laboratorio GPFS ADSNET — inventario VM, SSH e preflight (r043)

**Stato:** le VM sono predisposte e, secondo l'operatore, le chiavi SSH sono già state distribuite fra i nodi 01/02/03. I collegamenti non sono stati ancora verificati dal progetto e GPFS non è stato installato da questi script. Nessuna procedura del kit modifica dischi, rete, firewall, repository o chiavi.

| Host | OS | Risorse | Ruolo |
|---|---|---|---|
| github-runner-01.vm.adsnet.it | RHEL 9.8 | 6 vCPU / 16GB; OS 80GB + dati 150GB | IBM Storage Scale NSD, admin, CES |
| github-runner-02.vm.adsnet.it | RHEL 9.8 | 6 vCPU / 16GB; OS 80GB + dati 150GB | IBM Storage Scale NSD, CES |
| github-runner-03.vm.adsnet.it | RHEL 9.8 | 6 vCPU / 16GB; OS 80GB + dati 150GB | IBM Storage Scale NSD/quorum |
| github-runner-04.vm.adsnet.it | Ubuntu 24.04.5 LTS | 4 vCPU / 8GB; OS 80GB | Runner GitHub Actions PRIVATO, no SSH root ai server GPFS |

Nomi DNS VIP: github-runner-vip-01.vm.adsnet.it, github-runner-vip-02.vm.adsnet.it. Devono essere IP riservati, distinti dai quattro IP fissi; l'assegnazione e il failover saranno gestiti da IBM CES, quindi non configurarli manualmente sulle interfacce.

## Eseguire le verifiche non distruttive

Il codice dei nuovi script si trova nel ramo r043 e sarà integrato in main solo dopo CI e revisione. Per ora clonare il ramo tramite un terminale sul server:

~~~bash
git clone --branch feature/r043-adsnet-gpfs-ssh-topology-preflight --single-branch https://github.com/desalvo/multi-backup.git /tmp/multi-backup-gpfs-preflight
cd /tmp/multi-backup-gpfs-preflight
~~~

Su github-runner-01, 02, 03 e 04 rispettivamente, eseguire soltanto il comando corrispondente alla VM:

~~~bash
bash scripts/gpfs-lab/06-adsnet-vm-preflight.sh 01
bash scripts/gpfs-lab/06-adsnet-vm-preflight.sh 02
bash scripts/gpfs-lab/06-adsnet-vm-preflight.sh 03
bash scripts/gpfs-lab/06-adsnet-vm-preflight.sh 04
~~~

Dal runner Ubuntu 04:

~~~bash
bash scripts/gpfs-lab/07-adsnet-dns-check.sh
~~~

Su ciascuno dei tre RHEL, dopo aver verificato le impronte delle host key SSH e registrato i server in known_hosts di root (nessun prompt/accettazione cieca):

~~~bash
sudo bash scripts/gpfs-lab/08-adsnet-verify-ssh-mesh.sh
~~~

Ogni esecuzione fa 6 connessioni SSH non interattive: FQDN e short name di 01/02/03, incluso sé stesso. Tre esecuzioni producono 18 controlli. IBM richiede SSH passwordless dal nodo admin e dai nodi CES verso il cluster e con FQDN e short name; una matrice completa evita errori del toolkit. BatchMode e StrictHostKeyChecking=yes impediscono password e nuove host key non verificate. Se il controllo segnala Host key verification failed, NON disabilitare il controllo: confrontare il fingerprint con quello dalla console del virtualizzatore e registrarlo manualmente in root known_hosts.

**Nessuna chiave SSH privata deve essere scambiata tra VM**, e il nodo 04 non deve disporre di accesso SSH root a 01/02/03.

## Prerequisiti IBM prima di ogni operazione sui 150 GB

Confermare i dischi con seriale/WWN, verificare assenza di PV/VG/LV, filesystem e mount, e autorizzare separatamente ogni operazione distruttiva del toolkit o di mmcrnsd/mmcrfs. Non eseguire dnf update senza vincolo di minor release: RHEL 9.8 potrebbe diventare 9.9. Controllare la matrice IBM esatta tra Developer Edition, kernel e portability layer. Questi test non certificano il diritto di licenza o il kernel.

## GitHub privato predisposto

Il repository https://github.com/desalvo/multi-backup-labs esiste ed è privato, ed è stato inizializzato con README e workflow .github/workflows/gpfs-ces-field.yml. Registrare unicamente github-runner-04.vm.adsnet.it come runner self-hosted sul repository privato, con nome FQDN e label gpfs-ci; non registrare i nodi 01/02/03. La workflow privata resta manuale e verifica la revisione esatta di main pubblico approvata fuori CI da root sul nodo 04. Niente software IBM, chiavi SSH o credenziali nel repository.

Il test autentico GPFS potrà iniziare soltanto dopo installazione ufficiale IBM, CES NFS/SMB sui due VIP, snapshot/fileset GPFS coerente esportato in lettura e autorizzazione del runner. Una generica condivisione NFS leggibile NON è certificazione GPFS.

Riferimenti: [IBM installation toolkit, passwordless SSH](https://www.ibm.com/docs/en/storage-scale/6.0.1?topic=toolkit-preparing-use-installation) e [IBM protocol installation prerequisites](https://www.ibm.com/docs/en/storage-scale/6.0.0?topic=protocols-installation-prerequisites).
