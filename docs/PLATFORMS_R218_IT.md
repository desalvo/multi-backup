# Multi Backup r218 — matrice piattaforme e capacità

Data: 18 novembre 2026. Fonte di qualificazione: CI r217 #44246558178, commit b085abc, COMPLETATA CON SUCCESSO. Il branch r218 contiene aggiunte indipendenti ancora in qualificazione. Nessuna affermazione di release pubblica firmata.

| Area | Piattaforme | Interfaccia | Stato verificato |
|---|---|---|---|
| Server Rust e agenti | Linux, container OCI, Kubernetes | Web HTTPS + REST, agenti Rust | Suite esistenti r217 verde, HA fisica 3 host non certificata |
| GUI desktop Rust | macOS Apple Silicon e Intel, Windows x64, Ubuntu 24 x86_64 | App nativa installabile | Quattro installer reali r217 PASS, non firmati |
| Linux aggiuntivi | Rocky/AlmaLinux 8/9/10, Debian 12/13 | RPM/DEB e servizi | Matrice completa firmata e install/rollback da collaudare |
| Browser | PC, tablet e smartphone | Console web responsive r218 | Codice e unit test r218, E2E device reale da collaudare |
| iOS/iPadOS | iPhone e iPad 16+ | SwiftUI + WKWebView, un'unica app universale | Build unsigned iPhone/iPad e simulator su CI PASS; distribuzione fisica richiede firma |
| Android | Android 8+ (API 26+) | Java + WebView | Sorgenti e CI, APK unsigned in qualificazione |
| Cloud explorer | Dropbox, Google Drive, iCloud Drive, OneDrive, ownCloud, Nextcloud, Box | Browser remoto, upload/download/delete opt-in; desktop rclone FUSE/WinFsp | Rust core CI con backend simulato; provider reali da qualificare |
| iCloud B-In/B-Out | iCloud Drive, remoto rclone con 2FA Apple | Singolo file → age; age → nuovo file cloud | CI r218 con age reale e trasporto simulato PASS; non READY |

Il catalogo di backup contiene 34 provider dopo iCloud, di cui 16 con almeno un ambito operativo READY delimitato. Box è un backend del browser cloud comune, non un plugin backup specifico certificato. Resta obbligatorio distinguere **accesso a directory cloud** e **accesso ai membri di archivi cifrati**, quest'ultimo tramite manifest, checksum, chiave age e restore selettivo. Il collegamento automatico cloud → chunk store → restore rimane non completato.

Vedi: MOBILE_CLIENTS_R218_IT.md, CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md, ICLOUD_B_IN_OUT_R218_IT.md, RC_READINESS_R217_IT.md. La demo locale con dati simulati è distinta dal server Rust reale.
