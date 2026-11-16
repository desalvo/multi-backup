# Multi Backup — collaudo HA su tre host reali (r166)

**Stato: preflight automatizzato disponibile; collaudo distruttivo reale NON ancora eseguito.**

Il test già superato [43903067925](https://github.com/desalvo/multi-backup/actions/runs/43903067925) utilizza tre processi reali PostgreSQL 16/Patroni e tre etcd TLS in tre network namespace isolate su **un solo runner**, e prova partizione, promozione, ACK persistente, stop delle scritture del vecchio primario e reintegrazione. Non equivale a tre host indipendenti.

## Infrastruttura minima

Tre macchine/VM con kernel indipendenti e sulla rete privata del lab, PostgreSQL 16 + Patroni + etcd con quorum 2/3 e CA TLS, ciascuna con `/etc/patroni/patroni.yml`, `patronictl`, `systemctl`. Il runner operatore ha già SSH BatchMode con host-key rigorosamente pin-nate; nessuna password, token o certificato deve essere registrato nel repository.

Eseguire su **tre host di laboratorio identificati**:

```bash
python3 scripts/ci/ha-three-host-field-r166.py --self-test
python3 scripts/ci/ha-three-host-field-r166.py --hosts mb-lab-01 mb-lab-02 mb-lab-03
```

Il tool non modifica nulla: interroga tre host via SSH, controlla identificativi macchina e kernel distinti, entrambi i servizi attivi su tutti i nodi e tre viste Patroni coerenti con esattamente un primario. L'indipendenza fisica/rack/energia richiede attestazione manuale dell'operatore: identificativi diversi di VM **non** sono una prova di tre domini fisici.

La workflow `.github/workflows/ha-three-host-field-r166.yml` è **solo manuale**, richiede un runner `self-hosted` con etichetta `multi-backup-ha-field`; non usa minuti hosted o credenziali cloud inventate e non viene attivata automaticamente da push.

## Gate HA distruttivi da completare in laboratorio isolato

1. Attestare **tre domini di guasto realmente indipendenti** (host fisico, alimentazione, rete, storage, nessuna condivisione di fault domain non dichiarata).
2. Applicare una scrittura test in modalità **synchronous_mode_strict**, ricevere ACK e raccogliere LSN/tempo su un database di test dedicato.
3. Arrestare **esclusivamente** il primario del lab (con autorizzazione e piano di rollback esterno); verificare elezione del nuovo primario, persistenza di ogni transazione ACKed e capacità di nuova scrittura.
4. Isolare il vecchio primario dalla maggioranza DCS con firewall **di laboratorio** e verificare che non possa riconoscere ACK di scrittura; rilevare e rifiutare qualunque doppio primario.
5. Togliere quorum etcd (due membri su tre) e verificare che la modalità strict impedisca di certificare scritture senza DCS; ripristinare il quorum, controllare reintegrazione e eventuale `pg_rewind`.
6. Testare il client applicativo Multi Backup tramite HAProxy TLS `/primary`: riconnessione, job persistenti, lease/fencing e assenza di manifest doppi.
7. Eseguire backup/restore DB+data age nel vault WORM da una macchina diversa su un cluster di ripristino **vuoto**, poi verifica bit-a-bit e idempotenza fallita sul target già popolato.
8. Raccogliere evidenze con timestamp, SHA dei binari, log di Patroni/etcd/PostgreSQL, LSN, fotografie/inventory delle macchine e report attacco/recupero; non registrare credenziali o token.

Non arrestare servizi o applicare partizioni di rete su macchine in produzione. Non dichiarare il gate PASS se manca una prova documentata o se un dominio di guasto è condiviso.

## Stato GUI/server/agenti

Sul branch r166: GUI egui aggiunge monitor read-only, collegamento HTTPS da token su file privato e vista sintetica delle repliche, del DB e degli agenti; server espone contatori di agenti non identificanti su `/v1/agents` con token `monitor.read`; ricezione agente sostituisce il rename potenzialmente sovrascrivente con pubblicazione no-clobber atomica e test di concorrenza. Serve PASS CI sull'esatto commit e poi collaudi reali con agenti su più macchine: cifratura age, dimensione/hash, replay, revoca, restore e limiti per host.

**Non rilasciare stable, non dichiarare RC completa né fare merge su main prima di tutte le verifiche bloccanti.**


## Aggiornamento r168: runner self-hosted unico e senza duplicazioni

La guida operativa è `docs/HA_RUNNER_SELF_HOSTED_R168_IT.md`. Preferire un controller runner effimero dentro **repository privato** e usare `docs/examples/ha-private-runner.workflow.yml` come semplice wrapper del test già esistente. Il workflow fisico del repository pubblico mantiene la propria esistenza ma NON va collegato a infrastrutture sensibili senza un'esplicita analisi di sicurezza. L'automazione fisica esegue soltanto il preflight read-only, non ripete i fixture negativi già eseguiti nella CI hosted. Il failover fisico reale richiede un ulteriore gate autorizzato e resta aperto.
