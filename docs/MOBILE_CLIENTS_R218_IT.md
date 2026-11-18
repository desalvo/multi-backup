# Multi Backup r218 — client remoti iOS, iPadOS e Android (anteprima)

**Sviluppo separato dalla RC r217.** I client mobili NON costituiscono un backup engine mobile: sono app native leggere, frontend del server Rust tramite la medesima console web `/`, con stessa autenticazione web, sessione e controlli CSRF. Tutti i backup, restore, plugin, agenti e vault rimangono esclusivamente sul server. La GUI server è responsive, con selettore di sezione e comandi touch su smartphone e tablet.

## Piattaforme, build, firma

| Piattaforma | Implementazione | Output CI non firmato | Stato |
|---|---|---|---|
| iOS / iPhone (16+) | SwiftUI + WKWebView | `multi-backup-r218-ios-ipados-unsigned.ipa` | Preview: build/device da verificare su macOS CI |
| iPadOS / iPad (16+) | Stessa app universale (device family 1,2) | stesso IPA / app universale | Preview: build/device da verificare su macOS CI |
| iOS/iPadOS Simulator | SwiftUI + WKWebView | `multi-backup-r218-ios-ipados-simulator.zip` | Preview: simulator build da verificare |
| Android (API 26+, Android 8+) | Java Android Activity + Android System WebView | `multi-backup-r218-android-unsigned.apk` | Preview: build da verificare su Android CI |

Le app sono compilate da `.github/workflows/mobile-unsigned-preview.yml`. Nessun secret Apple/Play Console è richiesto e non vengono chiamati servizi App Store/Google Play. **Un IPA senza firma non è installabile normalmente su iPhone/iPad fisici** e un APK release unsigned deve essere firmato prima dell'installazione ordinaria. I file sono soltanto artefatti di compilazione e valutazione tecnica. L'esecuzione su device reali e la distribuzione sono successive.

## Connessione sicura

1. Aprire l'app; configurare **solo** `https://host[:porta]` del server Multi Backup, con certificato TLS pubblicamente valido/trusted. Non accettare URL con utente/password, query, path o HTTP.
2. Accedere alla console del server con i consueti account. I cookie di sessione sono gestiti da WebKit/WebView; chiavi age e token dei provider rimangono sul server.
3. Usare **Server** per cambiare destinazione; i client cancellano le sessioni WebView memorizzate nel proprio contenitore applicativo.
4. La GUI adatta controlli, griglie, ricerca e scroll a schermi piccoli. Non esegue backup in locale, non accede a filesystem o credenziali nativi del device.

## Restrizioni note della prima preview

- Link esterni espliciti si aprono nel browser di sistema; redirect SSO verso Identity Provider esterni e ritorno automatico nell'app richiedono un flusso OAuth ASWebAuthenticationSession/Custom Tabs ancora da implementare. Il **login locale HTTPS** del server funziona attraverso la WebView. Non aggirare restrizioni di origin per forzare il login.
- Nessuna modalità offline, push notification, biometria mobile, upload foto/file o condivisione di documenti del device.
- Nessuna firma Apple Developer ID/iOS provisioning, TestFlight, App Store o firma Android release: non è una distribuzione disponibile all'utente finale.
- Non è stato ancora certificato il comportamento su dispositivi fisici, certificate pinning o session lifetime in background.
- La CI mobile è completamente **non bloccante** per `rc-preflight-r217.yml`: nessun job mobile nel suo `needs`, nessuna firma richiesta, nessun rilascio stable.
- Il server richiede `MULTI_BACKUP_PUBLIC_ORIGIN=https://...`, TLS vero tramite reverse proxy; non esporre le API senza HTTPS.

## Comandi per riprodurre

Android (Java 17, Android SDK API 35, Gradle 8.10.2):

```sh
gradle -p mobile/android --no-daemon :app:assembleRelease
# mobile/android/app/build/outputs/apk/release/app-release-unsigned.apk
```

iOS / iPadOS (macOS con Xcode e XcodeGen):

```sh
xcodegen generate --spec mobile/ios/project.yml
xcodebuild -project mobile/ios/MultiBackupMobile.xcodeproj -scheme MultiBackupMobile \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  CODE_SIGNING_ALLOWED=NO build
```

Le build sono solo preview: prima di chiamare READY un client richiedere test di login/logout effettivi, attacco TLS, UI su schermi piccoli e restore server controllato. Le brochure aggiornate sono generate in workflow separato e mostrano chiaramente questi limiti.


## r218 Cloud Drive

L'app mobile visualizza la stessa console web Rust del server, ora con sezione **Cloud Drive** per navigare directory, scaricare file e, solo se abilitate dal server, effettuare upload di nuovi oggetti o cancellazioni puntuali autorizzate. I mount FUSE/WinFsp POSIX non sono disponibili nativamente su iPhone/iPad/Android; il client vi accede attraverso il server. Il browser di un cloud remoto non è l'elenco dei file interni a un archivio age: quello richiede import verificato, manifest e selezione di restore. Vedi [guida cloud](CLOUD_DRIVES_MOUNT_RESTORE_R218_IT.md).
