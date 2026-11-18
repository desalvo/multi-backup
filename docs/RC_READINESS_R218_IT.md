# Multi Backup r218 — matrice di pre-RC verificabile

**Data:** 18 novembre 2026
**Branch di lavoro:** `feature/r218-mobile-clients`
**Pull request:** [#55](https://github.com/desalvo/multi-backup/pull/55) (draft)
**Sorgente delle nuove funzionalità:** `c8e05cdf64fdcdab123181c856b24e16f9c45225`
**Licenza:** EUPL-1.2

## Criterio READY

Il catalogo r218 ha **34 plugin**. Soltanto **16** hanno almeno un'operazione
READY precisamente delimitata, con esclusioni esplicite: *16/34 (47,1%) di
provider con un ambito READY*, **non** 47,1% di completamento del prodotto.
Gli altri 18 rimangono Preview/Planned/Unsupported. I test sintetici non
certificano interi provider. La matrice r217 rimane intatta; r218 ha ora
`.github/workflows/scoped-ready-r218.yml`, con vincolo **esatto 34** e
confronto di ciascun ruolo/ambito con `READY_ROLES` in
`scripts/ci/rc-preflight-r217.py`.

## Funzionalità implementate e prove

| Area | Evidenza | Stato onesto |
|---|---|---|
| Chunk cifrati con zstd / xz-LZMA2 / gzip / none | `rust/src/chunk_store.rs` e [CI cloud #44298666614](https://github.com/desalvo/multi-backup/actions/runs/44298666614), AGE reale, restore nuovo target per tutti e 4 | **PASS locale CI** |
| Restore da ZIP64 cloud | `rust/src/cloud_recovery.rs`, indice AGE, SHA256 volumi, verifica chunk/parity, restore selettivo nuovo target | **PASS con rclone sintetico** |
| Cambio concorrente dell'indice remoto | Re-download finale dell'indice e confronto SHA256; test di sostituzione a metà download in `scripts/ci/test-cloud-explorer-r218.sh` | **PASS**, non version fencing atomico |
| Server/agent/PG/GUI/OCI/Kubernetes | Workflow `software-readiness-r168.yml` su SHA del gate r218 | **Gate software passato** nel primo run; ricontrollare quello finale |
| Desktop nativo/installer | Ricette `native-installers-r217.yml`, compilate su sorgente r218 | Test incrociati; non equivalgono a firma ufficiale |
| Screenshot Rust, manuali e brochure IT/EN | [CI #44295235864](https://github.com/desalvo/multi-backup/actions/runs/44295235864), fonte immutabile e PDF pubblicati | **PASS**, documenti correnti versionati |
| Client iOS/iPadOS/Android | [CI #44256037105](https://github.com/desalvo/multi-backup/actions/runs/44256037105) | Build/test unsigned, no installazione reale certificata |

## Gate unico sullo SHA

`.github/workflows/rc-preflight-r218.yml` è **non pubblicante**.
Richiede allineamento dell'identico `GITHUB_SHA` su:
contratti READY, suite software Rust/server/agent/Kubernetes,
cloud ZIP64/age reali con trasporto sintetico, pacchetti nativi
Ubuntu/macOS ARM+Intel/Windows installati e security policy.
Rifiuta *failure*, *cancelled* e *skipped* e produce un JSON esplicito
`r218-exact-source-gate.json` **solo se** ogni famiglia è verde.

- Primo gate [#44297591424](https://github.com/desalvo/multi-backup/actions/runs/44297591424):
  rilevati problemi di sola infrastruttura/policy: dipendenza
  `defusedxml` non installata, controllo r217 di catalogo 33 invece dei
  34 provider r218. Nessuna promozione da quel run.
- Correzione `c8e05cdf`: dipendenza Python esplicita,
  workflow READY r218 separato con vincolo 34, collaudo aggiuntivo
  su tutti i codec; secondo gate
  [#44298666402](https://github.com/desalvo/multi-backup/actions/runs/44298666402).
  **Non dichiarare verde** senza controllare effettivamente la
  conclusione di quest'ultimo run.

## Attività bloccate da laboratorio/credenziali esterni

1. OAuth, token refresh, version ID/ETag condizionale e restore remoto
   su veri account Dropbox/Google Drive/iCloud/OneDrive/ownCloud/Nextcloud/Box.
   Il catalogo cloud viene ricontrollato, ma non esiste il fencing atomico
   del fornitore: `remote_version_atomic=false`.
2. HA/fencing con **tre host fisici distinti**, caduta di alimentazione,
   separazione di rete e collaudo di ripartenza.
3. Restore e avvio di VM reali VMware, oVirt, PVE e PBS; guest consistency,
   snapshot/change-tracking e validazione applicativa.
4. Firma Authenticode, Developer ID/Apple notarization, firme RPM/DEB e
   installazione mobile tramite canali ufficiali o hardware fisico.
5. Certificazione universale POSIX ACL/xattr, Oracle RAC/ASM/TDE,
   MySQL GTID/PITR e SAN multipath/CHAP.

**Regola release:** non effettuare merge su `main`, tag stable, né
presentare r218 come RC pubblica firmata prima delle autorizzazioni
e dei gate pertinenti. La RC0 r217 resta indipendente e con CI
storicamente verde [#44246558178](https://github.com/desalvo/multi-backup/actions/runs/44246558178).

## Ripresa del lavoro

Aprire la PR #55 e il **più recente** run r218 sull'esatto SHA;
controllare tutti gli `needs`, identificare job failed o skipped e
prelevare i log. Se il gate è verde, verificare JSON artifact,
`SOURCE_SHA` e SHA256 delle build. Se non verde, correggere il codice
o laboratorio che ha fallito senza abbassare criteri e ripetere
la CI. Aggiornare `docs/DEVELOPMENT_STATUS.md`, `docs/HANDOFF.md`
e questa matrice ad ogni cambiamento sostanziale.
