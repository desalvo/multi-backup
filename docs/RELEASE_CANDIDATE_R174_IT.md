# Multi Backup 0.1.0 RC1 — release candidate di prova (r174)

**Data:** 16 novembre 2026. **Codice binario:** commit immutabile `1f1a726a3ffed7b46a7498b99891bfb935570865`, Production CI [#43932195582](https://github.com/desalvo/multi-backup/actions/runs/43932195582), **32 job richiesti PASS**; gli altri 3 job (pubblicazione automatica, GUI Python legacy e Docker Hub) erano esclusi intenzionalmente.

**RC1 non è stable e non è certificata per un deployment produttivo.** Le build sono native Rust, compilate da CI nelle piattaforme dichiarate, e includono GUI, CLI e agenti. I pacchetti sono assemblati esattamente dagli artifact già collaudati su un unico SHA; il collector rifiuta artefatti legacy/Python, file vuoti, duplicazioni, platform matrix incompleta o symlink. SHA-256 e manifest JSON accompagnano la RC.

## Decisione di policy esplicita: notarizzazione non bloccante per la RC

Il committente ha autorizzato la **RC con app macOS firmata ad-hoc ma NON notarizzata**. L'eccezione vale solo per il canale `rc` e **non** promuove automaticamente una versione `stable`: il gate Stable mantiene Developer ID, notarizzazione e firma Windows/packaging dove richieste dalla politica di distribuzione. Su macOS Gatekeeper potrebbe segnalare il programma come proveniente da sviluppatore non verificato: installare solo in ambienti di prova, verificando prima il file `SHA256SUMS`, e autorizzare una singola app dalle impostazioni Privacy e Sicurezza, quando consentito dall'amministratore. **Non** disattivare Gatekeeper globalmente.

La CI prova la creazione, installabilità tecnica e controlli di integrità dei pacchetti; **non** dimostra l'esecuzione interattiva su ogni variante OS di ogni programma firmato/notarizzato.

## Capacità software coperte da test CI

- **Core Rust:** cifratura age, backup full/incremental, dedup, chunk/chain, integrità, recovery, fault-injection e controllo dell'accesso ai dati.
- **GUI Rust:** unit test, rendering nativo Linux/Xvfb di tutte le dieci schermate più About, recupero dello stato del server tramite monitor.read e gestione sicura delle preferenze.
- **Server Rust:** immagine OCI hardened, gateway autenticato, PostgreSQL16 reale, due pod sul cluster Kind con tre nodi, sessioni condivise, controllo della revoca e PostgreSQL TLS verify-full.
- **Agenti Rust:** Ed25519, approvazione/revoca, anti-replay, age encrypted upload, receipt PostgreSQL durevole, collisione upload senza overwrite, restore in test reali PG16.
- **Storage protetto:** S3 Object Lock WORM e restore reali verificati nei backend coperti dai workflow; non equivale a immunità da root quando credenziali/storage non sono indipendenti.
- **Pacchetti:** macOS arm64/x86_64 (DMG e agenti PKG), Windows x64 (GUI installer e agent EXE), Linux Debian 12/13 amd64/arm64 (DEB), Rocky/Alma Linux 8/9/10 (RPM), Linux Ubuntu24 runtime preview. Totale 27 famiglie Rust richieste.
- **Telemetria e HA funzionale:** il gate RC r174 esegue gli stessi workflow hosted Prometheus HTTPS, Net-SNMPv3 authPriv e Patroni/etcd con netns/iptables già presenti. Il loro PASS riguarda **il laboratorio CI**, non un test remoto con guasto fisico.

## Limiti obbligatori e scope di esclusione

1. **HA su tre host fisici non certificata.** Il failover/partition NetNS usa servizi reali PostgreSQL16, Patroni e etcd, ma un solo runner fisico. Il laboratorio reale richiede runner self-hosted effimero in repository privato, tre host indipendenti e autorizzazione ad arrestare processi/rete e verificare fencing/rejoin. Vedi `docs/HA_RUNNER_SELF_HOSTED_R168_IT.md`.
2. **Agenti su rete esterna non certificati.** La firma, la cifratura e le ricevute PostgreSQL sono state verificate in test reali; resta un E2E attraverso TLS e WAN tra host diversi, con backup e restore finali.
3. **Vault root-resistant non certificato.** Un root Linux che controlla insieme l'OS e le credenziali amministrative può manomettere protezioni solo software locali. Per una prova anti-ransomware forte utilizzare un backend indipendente S3 COMPLIANCE con Object Lock e privilegi/retention separati: le proprietà di resistenza devono essere collaudate sul deployment specifico.
4. **Non tutti i 33 plugin sono pronti.** Il catalogo contiene `Preview`, `Planned` e `Unsupported` — *nessun provider è marcato globalmente `Ready`*. Le funzioni non integrate restano fail-closed. I protocolli NFS/Samba/GPFS-CES sono compatibilità standard, non certificazione nativa IBM GPFS. PVE/VMware/oVirt non sono dichiarati collaudati end-to-end se manca l'ambiente vendor.
5. **Firma / notarizzazione / auto-update:** i pacchetti RC possono essere non firmati o ad-hoc. La firma publisher Windows, notarizzazione Apple, installazione E2E su ambienti utente e rollback updater restano gate aperti per il canale stable.
6. **Disponibilità e monitoraggio sul campo:** Prometheus e SNMPv3 sono testati in CI con endpoint loopback; il comportamento in reti reali, VLAN, distribuzione HA e protezione amministrativa va ancora qualificato.

Questa **RC complessiva software di prova** include tutte le componenti compilate e una matrice di installazione completa, **non** conferisce automaticamente capacità operative ai provider `Planned` e **non** certifica deploy HA fisico. Per la matrice completa vedi `docs/RC_READINESS_R170_IT.md`.

## Integrità, provenienza e policy di rilascio

- GitHub [run #43932195582](https://github.com/desalvo/multi-backup/actions/runs/43932195582) è il gate sorgente degli artefatti.
- Il gate r174 valida **esattamente 32 successi**, gli **esatti 3 skip autorizzati** e **27** gruppi di artifact Rust, verifica che non siano scaduti e che lo SHA associato sia quello atteso.
- Prima della pubblicazione r174 esegue ancora le suite reali Prometheus HTTPS, Net-SNMPv3 e Patroni NetNS (non raddoppia le suite core/packaging già superate).
- `release-manifest.json` specifica `stable_certified:false`, `macos_notarized:false`, `physical_3_host_ha_certified:false`; `SHA256SUMS` permette di verificare ogni archivio. Non installare pacchetti se l'hash non coincide.
- **Nessuna pubblicazione Docker Hub** né promozione automatica stable o main: la tag RC è una prerelease separata.

**One-line English commit summary:** Package immutable CI-qualified Rust native binaries into a clearly scoped RC1 prerelease without requiring macOS notarization.
