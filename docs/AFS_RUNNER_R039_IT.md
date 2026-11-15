# Multi Backup r039: runner dedicato OpenAFS/Kerberos

## Scopo e stato

Questa configurazione predispone una VM effimera per la certificazione OpenAFS reale. Non rappresenta ancora un collaudo superato. Obiettivo: KDC Kerberos, VLDB, volserver, client AFS montato, vero dump full+incrementale, archivio Rust age, nuovo volume restore e confronto byte per byte.

Sistema **preferito**: VM Debian 12 (Bookworm) x86_64 appena creata con kernel standard **6.1.x-amd64**, NON kernel di backports. Alternativa già prevista: Ubuntu 22.04 GA 5.15-generic. Risorse suggerite: 4 vCPU, 8 GiB RAM, 50 GiB disco. Sono dimensionamenti indicativi e la compatibilità OpenAFS è verificata effettivamente con DKMS e modprobe, non data per scontata. Isolare la VM: nessun dato reale o accesso amministrativo alla produzione.

## Preparazione da eseguire sulla VM

~~~bash
cat /etc/os-release          # Debian 12, versione 12 (bookworm)
uname -r                     # 6.1.x-amd64
git clone --branch feature/r039-afs-agent-packages https://github.com/desalvo/multi-backup.git
cd multi-backup
sudo bash scripts/bootstrap-openafs-runner-vm.sh
~~~

Il bootstrap rifiuta il riuso della VM, verifica la versione del sistema e il kernel standard Debian 12 (6.1.x-amd64) oppure Ubuntu 22.04 GA (5.15.x-generic), e installa la corretta libreria ICU Debian (libicu72), impone la conferma DISPOSABLE-OPENAFS-VM, compila/carica il modulo OpenAFS, crea l'utente mb-afs-runner e abilita sudo passwordless solo nell'host sacrificabile. Non eseguire su host esistenti o di produzione.

## Registrazione del runner

Aprire GitHub: desalvo/multi-backup > Settings > Actions > Runners > New self-hosted runner. La pagina mostra un token di registrazione a breve validità. Non copiarlo in Git o nella chat.

~~~bash
sudo -iu mb-afs-runner
git clone --branch feature/r039-afs-agent-packages https://github.com/desalvo/multi-backup.git
cd multi-backup
bash scripts/register-openafs-runner-once.sh
~~~

Lo script installa il runner ufficiale actions/runner con asset SHA256 verificato e lo registra come ephemeral con etichetta openafs-ci. Chiede interattivamente il token e avvia run.sh in primo piano. Un solo job; dopo il job distruggere la VM. Ephemeral deregistration NON cancella i keytab o i dati della VM.

## Avvio della certificazione

GitHub richiede che il workflow manuale workflow_dispatch esista su main: quindi è necessario un piccolo dispatcher fiduciario su main, con checkout di un commit SHA approvato e actor autorizzato, senza fondere PR #11. Il workflow OpenAFS in PR #11 non basta da solo. Il job dedicato non deve essere avviato su evento pull_request. Dopo l'avvio dalla pagina Actions, verificare un effettivo PASS per KDC/VLDB, vos dump e restore, token, manifest cifrato e confronto byte-identico.

Il dispatcher principale su main accetta l'OS Debian 12 kernel 6.1-amd64 (oltre a Ubuntu 22.04 GA), ma viene aggiornato solo dopo avere fissato il nuovo SHA della PR. Riportare il nuovo valore esatto expected_sha mostrato nella documentazione o nel workflow (non il precedente SHA 4754e5a5).
 
**Nota pratica Proxmox:** selezionare ISO Debian 12 amd64; non installare headers di un kernel diverso da quello riportato da uname -r; OpenAFS su Debian 12 usa il pacchetto ufficiale openafs-modules-dkms 1.8.9-1+deb12u1, e l'effettiva compilazione è il gate di ingresso. VM sacrificabile, fuori dalla rete di produzione. 
     
Non equiparare questa prova a certificazione globale della PR #11: agenti reali, fencing HA, ACL/xattr POSIX e pacchetti firmati sono ancora gate distinti.
