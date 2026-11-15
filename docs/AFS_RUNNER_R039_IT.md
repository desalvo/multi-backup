# Multi Backup r039: runner dedicato OpenAFS/Kerberos

## Scopo e stato

Questa configurazione predispone una VM effimera per la certificazione OpenAFS reale. Non rappresenta ancora un collaudo superato. Obiettivo: KDC Kerberos, VLDB, volserver, client AFS montato, vero dump full+incrementale, archivio Rust age, nuovo volume restore e confronto byte per byte.

Sistema scelto: VM Ubuntu 22.04 x86_64 appena creata con kernel GA 5.15-generic. Risorse suggerite: 4 vCPU, 8 GiB RAM, 50 GiB disco. Sono dimensionamenti indicativi e la compatibilità OpenAFS è verificata effettivamente con DKMS e modprobe, non data per scontata. Isolare la VM: nessun dato reale o accesso amministrativo alla produzione.

## Preparazione da eseguire sulla VM

~~~bash
uname -r
git clone --branch feature/r039-afs-agent-packages https://github.com/desalvo/multi-backup.git
cd multi-backup
sudo bash scripts/bootstrap-openafs-runner-vm.sh
~~~

Il bootstrap rifiuta il riuso della VM, verifica la versione del sistema e del kernel, impone la conferma DISPOSABLE-OPENAFS-VM, compila/carica il modulo OpenAFS, crea l'utente mb-afs-runner e abilita sudo passwordless solo nell'host sacrificabile. Non eseguire su host esistenti o di produzione.

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

Non equiparare questa prova a certificazione globale della PR #11: agenti reali, fencing HA, ACL/xattr POSIX e pacchetti firmati sono ancora gate distinti.
