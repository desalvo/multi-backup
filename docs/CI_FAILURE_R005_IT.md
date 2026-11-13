# Fix CI nativa Multi Backup r005

## Esecuzioni analizzate

- Full CI verde: https://github.com/desalvo/multi-backup/actions/runs/43494324235
- Native CI: https://github.com/desalvo/multi-backup/actions/runs/43494324431
- Windows job: https://github.com/desalvo/multi-backup/actions/runs/43494324431/job/132123583905
- Rocky EL10 job: https://github.com/desalvo/multi-backup/actions/runs/43494324431/job/132123584088

## Cause e correzioni

1. Windows: `choco install age.portable` installa correttamente age, ma `Get-Command age.exe` restituisce lo shim nella directory Chocolatey `bin`. Copiato altrove, lo shim perde il target e fallisce. Copiare il binario reale in `lib/age.portable/tools` e verificarne l'esecuzione.
2. Rocky EL10: l'immagine DockerHub `rockylinux:10` della Official Library non è disponibile. Usare il repository gestito dal progetto Rocky `rockylinux/rockylinux:10` e uniformare EL8/9 per avere basi aggiornate.

Altri job già verdi: macOS arm64/x86_64, Alma EL8/9/10, Rocky EL8/9.

Stato RC: nuova full native CI e pubblicazione early-release da verificare prima di dichiarare completamento.
