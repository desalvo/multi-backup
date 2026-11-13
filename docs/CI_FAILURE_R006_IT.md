# Hotfix Multi Backup r006: Windows Inno Setup

Run: https://github.com/desalvo/multi-backup/actions/runs/43495351110  
Job: Windows 11 x64 installer (`132127057950`).

**Blocco confermato:** compilatore Inno Setup 6.7.1: `Could not read ... packaging\windows\LICENSE` mentre `LICENSE` risiede nella radice del repository.

**Correzione:** impostazione `SourceDir=..\..` nel file `packaging/windows/installer.iss`. Risolve tutti i file di input relativi (LICENSE, README, dist, icon) e colloca `OutputDir=installers` nella directory controllata dal workflow.

Smoke test necessario nel nuovo job Windows, seguito da upload degli installer e pubblicazione early-release.
