#!/usr/bin/env python3
"""Reproducible bilingual Multi Backup RC brochure and detailed user manuals.

The screenshots in this publication come from the actual Tk interface (Xvfb).
The landscape hero and icon are original graphic assets, not representations
of executed backup runs or external storage availability.
"""
from __future__ import annotations
import os
import sys
import io
import re
import tempfile
from pathlib import Path
from reportlab.lib import colors
from reportlab.lib.enums import TA_LEFT,TA_CENTER
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet,ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (BaseDocTemplate,PageTemplate,Frame,Paragraph,PageBreak,
                                 Spacer,Image,KeepTogether,Table,TableStyle,HRFlowable)
from reportlab.pdfgen import canvas

ROOT=Path(__file__).resolve().parents[1]
VERSION=re.search(r"VERSION = '([^']+)'",(ROOT/'version.py').read_text('utf-8')).group(1)
MEDIA=ROOT/'assets'
SCREENS=ROOT/'docs/screenshots'
OUTMAN=ROOT/'docs/manuals';OUTBR=ROOT/'docs/brochures'
OUTMAN.mkdir(parents=True,exist_ok=True);OUTBR.mkdir(parents=True,exist_ok=True)
W,H=A4
NAVY=colors.HexColor('#14253f');BLUE=colors.HexColor('#116ccf');TEAL=colors.HexColor('#1ea9b5')
GOLD=colors.HexColor('#ffaf54');PALE=colors.HexColor('#eef5fc');INK=colors.HexColor('#1e334e')

pdfmetrics.registerFont(TTFont('DejaVu','/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'))
pdfmetrics.registerFont(TTFont('DejaVuBold','/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'))
S={
 'H1':ParagraphStyle('H1',fontName='DejaVuBold',fontSize=20,leading=27,textColor=NAVY,spaceAfter=14),
 'H2':ParagraphStyle('H2',fontName='DejaVuBold',fontSize=12,leading=17,textColor=BLUE,spaceBefore=13,spaceAfter=8),
 'P':ParagraphStyle('P',fontName='DejaVu',fontSize=9.25,leading=15,textColor=INK,spaceAfter=9),
 'Small':ParagraphStyle('Small',fontName='DejaVu',fontSize=8.3,leading=12.5,textColor=colors.HexColor('#536983'),spaceAfter=7),
 'Bullet':ParagraphStyle('Bullet',fontName='DejaVu',fontSize=9.1,leading=14,textColor=INK,leftIndent=18,firstLineIndent=-12,spaceAfter=7),
 'Mono':ParagraphStyle('Mono',fontName='DejaVu',fontSize=7.2,leading=11,textColor=INK,spaceAfter=7,backColor=PALE,borderPadding=6),
 'White':ParagraphStyle('White',fontName='DejaVuBold',fontSize=11,leading=17,textColor=colors.white),
 'Table':ParagraphStyle('Table',fontName='DejaVu',fontSize=8.5,leading=12,textColor=INK),
}


def esc(text):
    return (str(text).replace('&','&amp;').replace('<','&lt;').replace('>','&gt;'))

def p(text,style='P'):
    return Paragraph(text,S[style])

def bullets(items):
    return [p('• '+esc(x),'Bullet') for x in items]


def screenshot(name,width=164*mm,caption=''):
    path=SCREENS/name
    from PIL import Image as PIL
    im=PIL.open(path)
    height=width*im.height/im.width
    jpg=ROOT/'docs/.publication-cache'/f'{path.stem}.jpg'
    jpg.parent.mkdir(parents=True,exist_ok=True)
    if not jpg.exists() or jpg.stat().st_mtime < path.stat().st_mtime:
        im.convert('RGB').save(jpg,quality=83,progressive=True,optimize=True)
    result=[Image(str(jpg),width,height,kind='proportional')]
    if caption:result.append(p(esc(caption),'Small'))
    return result


def panel(title,text):
    cell=[p(esc(title),'H2'),p(text,'P')]
    t=Table([[cell]],colWidths=[168*mm])
    t.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,-1),PALE),('BOX',(0,0),(-1,-1),.5,colors.HexColor('#dae9f5')),
                           ('LEFTPADDING',(0,0),(-1,-1),12),('RIGHTPADDING',(0,0),(-1,-1),12),
                           ('TOPPADDING',(0,0),(-1,-1),10),('BOTTOMPADDING',(0,0),(-1,-1),10)]))
    return t


class Book(BaseDocTemplate):
    def __init__(self,path,lang,kind='manual'):
        self.kind=kind
        super().__init__(str(path),pagesize=A4,leftMargin=20*mm,rightMargin=20*mm,
                         topMargin=22*mm,bottomMargin=21*mm,title='Multi Backup '+kind,
                         author='Multi Backup Project',subject='Multi Backup RC1')
        self.lang=lang
        frame=Frame(self.leftMargin,self.bottomMargin,self.width,self.height,id='body',leftPadding=0,
                    rightPadding=0,topPadding=0,bottomPadding=0)
        self.addPageTemplates([PageTemplate(id='default',frames=frame,onPage=self.decor)])
    def decor(self,c:canvas.Canvas,doc):
        c.saveState()
        c.setFillColor(NAVY);c.rect(0,H-11*mm,W,11*mm,fill=1,stroke=0)
        c.setFont('DejaVuBold',8);c.setFillColor(colors.white)
        c.drawString(20*mm,H-7.2*mm,'MULTI BACKUP  /  '+VERSION)
        c.setStrokeColor(colors.HexColor('#b8d9ed'));c.line(20*mm,15*mm,W-20*mm,15*mm)
        c.setFillColor(colors.HexColor('#4e6885'));c.setFont('DejaVu',7.5)
        c.drawString(20*mm,10.5*mm,'EUPL-1.2  |  '+VERSION+'  |  '+('USER MANUAL' if self.kind=='manual' else 'BROCHURE'))
        c.drawRightString(W-20*mm,10.5*mm,f'{doc.page:02d}')
        c.restoreState()
    def cover(self,c:canvas.Canvas,doc):
        c.saveState()
        # elegant full-bleed photographic hero, tightly cropped from original artistic asset
        c.drawImage(str(MEDIA/'hero.jpg'),0,H*.27,width=W,height=H*.73,mask='auto')
        c.setFillColor(NAVY);c.rect(0,0,W,H*.33,fill=1,stroke=0)
        c.drawImage(str(MEDIA/'icon-256.png'),W-56*mm,16*mm,width=37*mm,height=37*mm,mask='auto')
        c.setFillColor(colors.white)
        c.setFont('DejaVuBold',29)
        c.drawString(20*mm,66*mm,'Multi Backup')
        c.setFont('DejaVuBold',14);c.drawString(20*mm,54*mm,('Manuale utente' if self.lang=='IT' else 'User Manual')
                                             if self.kind=='manual' else ('Brochure di prodotto' if self.lang=='IT' else 'Product Brochure'))
        c.setFont('DejaVu',10);c.drawString(20*mm,42*mm,('Sicuro  •  Incrementale  •  Multipiattaforma' if self.lang=='IT'
                                                       else 'Secure  •  Incremental  •  Cross-platform'))
        c.setFont('DejaVu',8);c.setFillColor(colors.HexColor('#b9dbf6'))
        c.drawString(20*mm,17*mm,f'RELEASE CANDIDATE 1  |  {self.lang}  |  NOVEMBER 2026')
        c.restoreState()


def chapter(title,paragraphs,items=None,screen=None,caption='',example=None,extra=None):
    elements=[p(esc(title),'H1'),HRFlowable(width='100%',thickness=1,color=colors.HexColor('#bfd7eb')),
              Spacer(1,11)]
    for txt in paragraphs:elements.append(p(txt))
    if items:elements+=bullets(items)
    if example:
        elements.append(p('CLI example / Esempio CLI','H2'))
        elements.append(p(esc(example).replace('\n','<br/>'),'Mono'))
    if screen:
        elements.append(Spacer(1,6))
        elements+=screenshot(screen,caption=caption)
    if extra:
        elements.append(Spacer(1,7));elements.append(panel(extra[0],extra[1]))
    elements.append(PageBreak())
    return elements


EN=[
('The workspace and first launch',[
 'Multi Backup combines independent source plugins under one encrypted backup workspace. The first release candidate preserves the GitHub disaster recovery engine and adds a Dropbox read-only incremental backup plugin.',
 'The navigation sidebar provides Dashboard, Backup, Archives, Plugins, Encryption, Updates and Settings. Each screen works with the same application configuration and underlying CLI.'],
 ['Review the enabled plugins before starting a job.','Set a private output directory on an encrypted volume.','Keep your age private key separate from backup media.'],'dashboard.png','Actual running dashboard captured on Linux. Other operating systems may render controls differently.'),
('Installation and platform requirements',[
 'Install from the release page matching your platform: DMG on macOS 13+ (Apple Silicon or Intel), EXE/ZIP on Windows 11 x64, or RPM for Rocky/AlmaLinux 8, 9 and 10. Until signatures are configured, these are RC evaluation packages.',
 'For source use, install Python 3.11+, dependencies from requirements.txt and the external age, git, Git LFS and GitHub CLI programs. The Dropbox engine requires age and a token with minimum read permissions.'],
 ['Confirm the installer signature and SHA-256 digest before proceeding.','Allow the OS to install the CLI helper tools if they were not bundled.','Use gh auth login for GitHub; keep Dropbox tokens out of shell history.'],None,''),
('Dashboard tour',[
 'Dashboard is designed for quick orientation. Three cards show the available providers, the encryption model and the update channel. Use Create backup to open the job configuration or Browse archives to inspect finished snapshots.',
 'The application is deliberately not a cloud synchronization service: source data is never overwritten by restore actions without a user-selected destination.'],None,'dashboard.png','Dashboard landing page with photographic header and shortcuts.'),
('Choosing a source plugin',[
 'Open Plugins to enable the backend required for a job. GitHub is enabled by default; Dropbox is disabled until the user explicitly switches it on. The backend selector in Backup determines which source engine runs.',
 'External plugins are not imported automatically from arbitrary local paths. The current plugin registry exposes only the packaged, reviewed backends.'],
 ['GitHub supports repository metadata, release assets, CI logs and optional artifact cleanup.','Dropbox supports read-only file revisions and incremental encrypted storage.','Dropbox never calls remote delete or purge APIs.'],'plugins.png','Plugin settings page. Change the enabled state before switching backend.'),
('Preparing GitHub credentials',[
 'Authenticate the GitHub CLI with gh auth login and grant access to the repositories you want to preserve. GitHub returns metadata, but NOT the original values of existing Secrets.',
 'Use an external secure directory if you intentionally want to include known original secret values in an encrypted disaster recovery archive. Exports from CI and release registries may have independent retention rules.'],
 ['Never paste tokens or credentials into README files or issue trackers.','Dry-run CI artifact cleanup before performing any deletion.','Verify that release publishing artifacts remain protected.'],None,''),
('Preparing Dropbox credentials',[
 'Create a Dropbox API access token with read-only permissions. Enable Dropbox in Plugins and supply the token using MULTI_BACKUP_DROPBOX_TOKEN or an OS credential manager entry under service multi-backup and user dropbox.',
 'The Dropbox engine reads folder listings and file downloads, calculates a stable key from file ID and revision, and reuses existing encrypted files when their SHA-256 checksum matches. It writes a separate encrypted manifest for each cycle.'],
 ['No remote delete or modify calls are used.','Local old revisions are never purged.','Sharing ACLs and team admin settings are not yet exported in this RC.'],None,''),
('Backup workflow and progress',[
 'In Backup, select the backend, output and staging locations, and an age recipient. The output directory must be private; stage files on an encrypted volume. Press Run or resume to start a child process.',
 'The progress bar reflects the job state emitted by the CLI, while the log lists operations, warnings, and cleanup summaries. Interrupt safely leaves previous atomic archives and checkpoint journals intact.'],
 ['Do not use a shared or unencrypted staging volume.','For GitHub, repeat the exact command to resume from the last verified shard.','Select Force fresh cycle when you intentionally need a completely new run.'],'backup.png','Backup page with source selection, encryption keys, cleanup simulation and progress bar.'),
('CI artifact cleanup and safety',[
 'The optional GitHub artifact cleanup runs BEFORE the GitHub backup. It keeps the configured number of main-branch CI executions (default five), respects tag-associated runs, and dynamically protects artifacts needed for distribution, or whose dependencies are uncertain.',
 'Use Cleanup dry-run first. Real deletion is irreversible. As work proceeds, the CLI shows per-artifact updates, per-repository summaries, and an overall report. Cleanup does not delete workflow histories, releases or Git repository references.'],
 ['Explicitly opt in to cleanup; it is off by default.','Review dynamic protection decisions; unknown dependencies stay preserved.','Do not assume an external software repository is independent from an old artifact without checking.'],None,'',
 'multi-backup backup --backend github --cleanup-ci-artifacts --cleanup-ci-dry-run --keep-ci-runs 5'),
('Resuming, restarting and migration',[
 'The GitHub engine saves an integrity-checked checkpoint for each completed metadata and repository archive. The current cycle is automatically resumed when the same configuration is used after interruption.',
 'The --restart option produces a distinct backup cycle without overwriting the earlier completed archive. Older monolithic encrypted metadata archives are recognized and can be repartitioned with the correct age identity and recipient; originals remain immutable until new shards are verified.'],
 ['Do not remove .state until a full backup and restore validation have completed.','An interrupted download may need to restart its current file; prior completed shards are retained.'],None,'',
 'multi-backup backup --backend github --recipient age1EXAMPLE --restart --progress'),
('Understanding encrypted archive layout',[
 'The GitHub layout separates account/common metadata, per-repository settings, issues, releases and individual CI run archives; Git mirrors are kept in distinct encrypted archives, so unchanged snapshots may be deduplicated.',
 'The Dropbox layout contains encrypted objects and versioned encrypted manifests. Each file revision can be reused across backup cycles; there is no remote or local purge. Sidecar SHA-256 digests detect accidental corruption.'],None,'archives.png','Archive explorer showing available encrypted metadata and repository archives.'),
('Browsing and previewing content',[
 'Select an archive in the Archives page and provide the corresponding age private key. List contents to inspect member names. Show file displays a text or JSON member without extracting the entire archive into a user directory.',
 'The CLI offers list, browse, view, show and extract with equivalent functions. Large archives are streamed through the decryption layer, and paths are validated before any extraction.'],
 ['Do not extract archives onto shared drives.','Choose a new destination directory and review permissions.','Git mirrors must be checked out to a fresh path.'],'archives.png','Archive navigation and file inspection actions.'),
('Checkout and read-only mount',[
 'Checkout Git converts a saved bare mirror into a working tree at a new destination. Mount read-only uses bindfs on Linux/macOS, or subst on Windows after preparation of a temporary decrypted copy.',
 'The temporary mount workspace contains readable files; keep it on encrypted storage and unmount after use. The tool does not provide an always-encrypted virtual filesystem without disk staging.'],
 ['Install and configure bindfs where supported.','Use unmount after inspection to remove the private temporary extraction.'],None,''),
('Create and protect encryption keys',[
 'Open Encryption and generate a new age X25519 identity. The CLI keys generate subcommand creates an identity in a private application configuration folder with owner-only permissions and prints only the public recipient.',
 'The recipient string may be stored in application settings. The identity file must not be bundled with your ciphertext backups: keep separate encrypted/offline recovery copies and test the decryption process regularly.'],
 ['A lost age private key means the encrypted data is not recoverable.','Never share private key contents in screenshots or support logs.','Some platform key management requires installation of age-keygen.'],'encryption.png','Real key-management screen. Private key values are never displayed.'),
('Application settings',[
 'Settings stores non-secret preferences such as backup location, enabled plugins, retention and update channel. On macOS settings are stored under the user Application Support folder; on Windows under APPDATA; on Linux under XDG_CONFIG_HOME or ~/.config.',
 'The settings file is kept private where supported. Dropbox access tokens are excluded from JSON settings and must live in the environment or protected OS keyring.'],None,None,''),
('Update channels and verification',[
 'Stable is the default channel. Early release is selectable in Settings or the Updates screen. Early builds are uploaded after a [full-ci] commit on main, while stable releases require a version tag and a passing native build matrix.',
 'The RC checks for updates at launch and periodically (default six hours). It can stage an update only when a GitHub-provided SHA-256 digest is present; it never silently launches unverified installers. OS signing is required for a production-grade unattended updater.'],
 ['Use stable unless you are testing RC builds.','Validate publisher signatures before installing.','Rolling early release packages may change without backward-compatibility guarantees.'],'updates.png','Updates page with release channel selection.'),
('CLI recipes',[
 'The same actions are available by CLI. Select backend on each backup invocation; plugins can be enabled or disabled from the CLI. Use --progress for a progress bar and --json-progress to integrate with scripts.',
 'Run --help for each subcommand. Example commands below cover creation, key management, retention simulation and channel selection.'],None,None,'',
 'multi-backup --version\nmulti-backup plugins\nmulti-backup plugin enable dropbox\nmulti-backup keys generate --name primary\nmulti-backup backup --backend github --progress\nmulti-backup settings channel early-release\nmulti-backup updates check'),
('Recovery drills and operational checklist',[
 'A backup is valuable only after recovery can be demonstrated. Regularly list the archive catalog, verify the SHA-256 sidecars, decrypt a small metadata file, checkout one Git mirror and confirm its Git history.',
 'For Dropbox, restore a selected revision into a safe local directory (never over live source data), and compare its content to the expected revision. Document the identities and the incident-response procedure outside the archived storage.'],
 ['Keep encryption identities offline and separate.','Test restore on a clean host before relying on the backup.','Review incomplete counts and CI cleanup reports.'],None,''),
('Security limits and troubleshooting',[
 'If a backup fails, inspect the CLI status and checkpoint before rerunning. Missing gh credentials, unavailable age, insufficient private staging permissions or expired API tokens are frequent causes. The resume mechanism avoids overwriting valid completed archives.',
 'Remote backup metadata does not include original GitHub Secrets values or every external system setting. Native packages are release-candidate artifacts until full platform integration, signing and security review complete.'],
 ['Error 401/403: check credentials and scope.','Invalid archive: verify checksum, identity and archive path.','Old metadata: supply original age identity plus new recipient for migration.'],None,''),
('Release channel and native support matrix',[
 'Multi Backup targets macOS Ventura 13 or later on Intel and Apple Silicon, Windows 11 x64, and Linux RPM distributions RockyLinux/AlmaLinux 8, 9 and 10. Build jobs use target OS runners or containers. All jobs must pass for release publication.',
 'Early release assets are published under the rolling early-release tag on GitHub. Stable uses a version tag such as v0.1.0. RPM repository definitions remain disabled until signing and trust distribution are configured.'],None,None,''),
('Roadmap and reporting issues',[
 'The first release candidate is a test milestone, not a final production certification. The next milestones are native installer launch tests, encrypted legacy migration test cases, live Dropbox integration and end-to-end install/update/restore validation.',
 'Report defects to the issue tracker of desalvo/multi-backup with version, platform, exact CLI parameters (never secrets), sanitized logs and a brief reproduction. The handoff in docs/HANDOFF.md supports continuation of development.'],None,None,'')]

IT=[
('Panoramica e primo avvio',[
 'Multi Backup riunisce motori di backup indipendenti in uno spazio di lavoro cifrato. La prima release candidate mantiene il motore GitHub di disaster recovery e aggiunge il plugin Dropbox incrementale e di sola lettura.',
 'La navigazione comprende Dashboard, Backup, Archivi, Plugin, Cifratura, Aggiornamenti e Impostazioni. Interfaccia grafica e CLI condividono la configurazione dell’applicazione.'],
 ['Prima di avviare un lavoro controllare i plugin attivi.','Impostare un percorso di output riservato su volume cifrato.','Conservare la chiave privata age separata dal backup.'],'dashboard.png','Dashboard reale catturata su Linux; sui diversi sistemi i controlli possono variare.'),
('Installazione e requisiti',[
 'Scaricare il pacchetto del proprio sistema operativo: DMG per macOS 13+ (Apple Silicon o Intel), EXE/ZIP per Windows 11 x64, RPM per Rocky/AlmaLinux 8, 9 e 10. I pacchetti RC devono essere verificati e non sono ancora certificati come produzione.',
 'Per eseguire il sorgente servono Python 3.11+, i moduli in requirements.txt e i programmi esterni age, git, Git LFS e GitHub CLI. Il plugin Dropbox richiede age e un token con permessi minimi di lettura.'],
 ['Controllare le firme dell’installer e lo SHA-256.','Verificare che le dipendenze native siano state incluse o installate.','Usare gh auth login per GitHub; non mostrare i token nelle schermate.'],None,''),
('Dashboard',[
 'La Dashboard fornisce una vista immediata delle sorgenti, del modello di cifratura e del canale di aggiornamento. I pulsanti permettono di creare un backup, consultare gli archivi o aprire la configurazione dei plugin.',
 'Il software non è un servizio di sincronizzazione cloud: il ripristino avviene verso una destinazione selezionata, senza sovrascrivere implicitamente i dati online.'],None,'dashboard.png','Schermata iniziale con immagine fotografica e scorciatoie.'),
('Selezione del plugin',[
 'Aprire Plugin per abilitare il motore necessario. GitHub è abilitato per default, mentre Dropbox resta disattivato finché non viene abilitato esplicitamente. Il selettore in Backup determina la sorgente da elaborare.',
 'Il registro interno non carica automaticamente codice Python sconosciuto da cartelle arbitrarie: nella RC sono disponibili soltanto i motori distribuiti con l’applicazione.'],
 ['GitHub: repository, release, log CI e pulizia facoltativa degli artifact.','Dropbox: versioni dei file, backup incrementale e cifratura.','Dropbox non esegue API remote di delete o purge.'],'plugins.png','Pagina di gestione dei plugin e del loro stato.'),
('Autenticazione GitHub',[
 'Autenticarsi con gh auth login e assicurarsi che il token abbia accesso ai repository da conservare. GitHub non permette di leggere i valori originali dei Secrets già registrati.',
 'I segreti originali, se disponibili, possono essere inclusi solo fornendo intenzionalmente una cartella di credenziali su disco cifrato. I registry esterni e i repository di distribuzione possono richiedere politiche di backup proprie.'],
 ['Non inserire credenziali in ticket o file README.','Provare il cleanup soltanto in dry-run, prima della cancellazione.','Controllare le dipendenze tra CI e repository di distribuzione.'],None,''),
('Autenticazione Dropbox',[
 'Creare un token Dropbox con autorizzazioni di sola lettura. Abilitare Dropbox dai Plugin e usare MULTI_BACKUP_DROPBOX_TOKEN oppure il portachiavi del sistema (servizio multi-backup, utente dropbox).',
 'Il motore legge l’elenco dei file e le revisioni, identifica ogni versione tramite ID e revisione, riutilizza gli archivi già verificati via SHA-256 e produce un manifest cifrato a ogni ciclo.'],
 ['Non modifica e non cancella file su Dropbox.','Non effettua purge dei vecchi backup locali.','Condivisioni, permessi di team e impostazioni amministrative sono fuori scope nella RC.'],None,''),
('Esecuzione e avanzamento',[
 'Nella pagina Backup scegliere backend, cartelle di output/staging e destinatario age. La cartella di staging deve essere privata e su disco cifrato. Il pulsante di avvio lancia un processo separato.',
 'La barra avanza in base agli eventi forniti dalla CLI e l’area dei log mostra attività, warning e sommari. L’interruzione sicura mantiene gli archivi atomici completati e i checkpoint validi.'],
 ['Non usare uno staging condiviso o non cifrato.','Con GitHub ripetere il comando identico per riprendere.','Usare Force fresh cycle per iniziare un nuovo ciclo indipendente.'],'backup.png','Pannello Backup con motore selezionabile, chiavi, simulazione cleanup e barra.'),
('Pulizia degli artifact CI',[
 'Il cleanup GitHub è facoltativo e precede il backup. Mantiene le ultime CI del branch principale secondo il numero configurato (default cinque), tutela le CI associate ai tag e preserva dinamicamente gli artifact necessari o potenzialmente necessari per la distribuzione.',
 'Eseguire prima Cleanup dry-run: la cancellazione reale è irreversibile. Durante l’operazione vengono mostrati avanzamento per artifact, sommario per repository e riepilogo complessivo. Non vengono eliminati commit, tag o release.'],
 ['Attivare espressamente il cleanup.','Controllare le decisioni conservative prima di applicare.','Verificare eventuali dipendenze esterne non dichiarate nei workflow.'],None,'',
 'multi-backup backup --backend github --cleanup-ci-artifacts --cleanup-ci-dry-run --keep-ci-runs 5'),
('Ripresa, riavvio e migrazione',[
 'Il motore GitHub salva per ogni archivio completato un checkpoint con verifica SHA-256. Dopo una interruzione, lo stesso comando riprende dall’ultima unità verificata.',
 'Con --restart si crea un nuovo ciclo senza sovrascrivere gli archivi precedenti. I vecchi metadata monolitici cifrati possono essere ripartiti usando identità age originale e destinatario per i nuovi archivi; la sorgente originale resta immutata fino alla verifica.'],
 ['Non cancellare .state prima di una verifica di ripristino.','Il file in download al momento dell’interruzione potrebbe essere riscaricato.'],None,'',
 'multi-backup backup --backend github --recipient age1ESEMPIO --restart --progress'),
('Struttura degli archivi cifrati',[
 'GitHub conserva account e dati comuni, impostazioni per repository, issue, release e singole esecuzioni CI; i mirror Git usano archivi cifrati separati e possono essere deduplicati.',
 'Dropbox memorizza oggetti cifrati e manifest cifrati per ciclo. Le revisioni non cambiate vengono riutilizzate, senza purge remoto o locale. I file sidecar SHA-256 permettono di verificare la corruzione accidentale.'],None,'archives.png','Catalogo dei backup: metadata cifrati e archivi repository.'),
('Esplorazione e anteprima',[
 'Selezionare un archivio e la chiave privata age associata, quindi scegliere List contents per vedere i file interni. Show file mostra il testo di un membro JSON o di log senza estrarre il contenuto in una cartella utente.',
 'La CLI mette a disposizione list, browse, view, show, extract. L’estrazione verifica i percorsi ed esclude membri pericolosi come link simbolici o attraversamenti di cartelle.'],
 ['Non estrarre in cartelle condivise.','Usare una destinazione nuova con permessi privati.','I mirror Git richiedono un checkout dedicato.'],'archives.png','Comandi della finestra Archivi.'),
('Checkout e mount di sola lettura',[
 'Checkout Git converte il mirror bare salvato in una working copy presso una destinazione nuova. Mount read-only usa bindfs su Linux/macOS oppure subst su Windows, dopo preparazione di una copia temporanea decifrata.',
 'Lo spazio di lavoro del mount contiene dati in chiaro; usare volume cifrato e smontare al termine. Non è previsto un filesystem cifrato virtuale senza file temporanei.'],
 ['Installare bindfs dove necessario.','Concludere con unmount e controllare la pulizia dello staging.'],None,''),
('Gestione delle chiavi age',[
 'Nella pagina Encryption si può generare un’identità X25519. La CLI keys generate crea la chiave nella cartella riservata dell’applicazione, imposta permessi 0600 e rende disponibile la sola chiave pubblica.',
 'Il destinatario pubblico può comparire nelle impostazioni. La chiave privata deve essere conservata fuori dal deposito cifrato, con copie di recupero offline e prove periodiche di decifratura.'],
 ['La perdita della chiave privata impedisce di recuperare il backup.','Non mostrare valori privati negli screenshot.','Per generare le chiavi può servire il programma age-keygen.'],'encryption.png','Schermata reale per la gestione delle chiavi.'),
('Preferenze dell’applicazione',[
 'La pagina Settings mantiene preferenze non segrete: destinazione dei backup, plugin attivi, limiti di conservazione e canale di aggiornamento. Su macOS usa Application Support, su Windows APPDATA e su Linux XDG_CONFIG_HOME o ~/.config.',
 'Il file delle impostazioni viene mantenuto privato, dove previsto dal sistema. I token Dropbox non sono salvati nel JSON delle preferenze.'],None,None,''),
('Aggiornamenti e canali',[
 'Stable è il canale predefinito; Early release è selezionabile nelle impostazioni o nella schermata Updates. Le versioni anticipate vengono prodotte dai commit [full-ci] su main, quelle stabili dai tag di versione dopo il superamento dei test nativi.',
 'La RC controlla i nuovi pacchetti all’avvio e periodicamente (sei ore per default) e scarica soltanto file con hash SHA-256 pubblicato da GitHub. L’installazione senza intervento è bloccata finché mancano firme e notarizzazione.'],
 ['Preferire stable per uso quotidiano.','Verificare sempre la firma del produttore.','La early release può variare più rapidamente.'],'updates.png','Schermata Updates e scelta del canale.'),
('Esempi CLI',[
 'Tutte le operazioni principali sono disponibili da terminale. Il backend deve essere selezionato per ogni backup; i plugin si possono abilitare dalla CLI. --progress abilita la barra e --json-progress produce eventi integrabili.',
 'L’help dei comandi è esclusivamente in inglese. Gli esempi seguenti coprono versione, plugin, cifratura, cleanup e aggiornamenti.'],None,None,'',
 'multi-backup --version\nmulti-backup plugins\nmulti-backup plugin enable dropbox\nmulti-backup keys generate --name primary\nmulti-backup backup --backend github --progress\nmulti-backup settings channel early-release\nmulti-backup updates check'),
('Controlli di ripristino',[
 'Un backup è utile soltanto se è stato verificato un ripristino. Periodicamente elencare gli archivi, verificare i checksum SHA-256, decifrare un piccolo metadata, effettuare checkout di un mirror Git e controllare la cronologia.',
 'Per Dropbox selezionare una revisione e ripristinarla in una directory locale sicura, senza sovrascrivere i dati sorgente. Conservare procedure e identità di emergenza fuori dai backup.'],
 ['Conservare la chiave age offline e separata.','Provare ripristini su un host pulito.','Controllare i sommari di backup incompleti e cleanup CI.'],None,''),
('Sicurezza e risoluzione dei problemi',[
 'Se un backup non riesce, esaminare il log e i checkpoint prima di ripetere. Credenziali gh mancanti, age non disponibile, permessi dello staging e token scaduti sono cause comuni. I checkpoint impediscono di sovrascrivere gli archivi completati validi.',
 'I metadata GitHub non comprendono i valori originali dei Secrets né tutti i sistemi esterni. I pacchetti RC richiedono test d’installazione, firma e revisione di sicurezza prima della produzione.'],
 ['Errore 401/403: controllare credenziali e permessi.','Archivio non leggibile: verificare checksum e identità.','Metadata storici: fornire identità age originale e destinatario nuovo.'],None,''),
('Sistemi supportati e release',[
 'Il progetto punta a macOS Ventura 13+ su Apple Silicon/Intel, Windows 11 x64, RockyLinux/AlmaLinux 8/9/10 x86_64. La CI usa runner o container corrispondenti. Tutti i job richiesti devono concludersi correttamente prima della pubblicazione.',
 'Il tag rolling early-release raccoglie i pacchetti anticipati, mentre i tag v0.1.0 e successivi identificano le stable. Le definizioni repository RPM restano disabilitate in attesa della firma GPG.'],None,None,''),
('Sviluppi successivi e segnalazioni',[
 'La prima release candidate è un traguardo di verifica, non una certificazione production. I passaggi successivi sono test nativi degli installer, migrazione reale di archivi cifrati, prove Dropbox e validazione end-to-end di installazione/aggiornamento/ripristino.',
 'Per segnalare problemi usare le issue del repository desalvo/multi-backup, indicando versione, sistema operativo, parametri CLI senza credenziali, log anonimizzati e passi di riproduzione. L’handoff si trova in docs/HANDOFF.md.'],None,None,'')]


def prepend_cover(document: Book, output: Path) -> None:
    """Place the artistic cover first, avoiding the blank title-page flow bug."""
    from pypdf import PdfReader, PdfWriter
    buffer=io.BytesIO()
    c=canvas.Canvas(buffer,pagesize=A4)
    document.cover(c,None)
    c.showPage();c.save();buffer.seek(0)
    writer=PdfWriter()
    writer.append(PdfReader(buffer))
    writer.append(PdfReader(str(output)))
    writer.add_metadata({'/Title':f'Multi Backup {document.kind} {document.lang}',
                         '/Author':'Multi Backup Project'})
    temp=output.with_name(output.stem+'.partial.pdf')
    try:
        with temp.open('wb') as file:
            writer.write(file)
        os.replace(temp,output)
    finally:
        temp.unlink(missing_ok=True)



def refresh_chapters_for_current_interface():
    """Correct current operational instructions without removing older detail."""
    replacements={
        'EN': {
            'The workspace and first launch': [
                'The approved photographic navy, violet and cyan dashboard opens with eight routes: Dashboard, Backup, Archives, Restore, Plugins, Vault and keys, Updates and Settings. The visual cards reflect actual local archive, vault and plugin state; sample scheduled plans are not implemented.',
                'At first launch, unlock your existing age vault using its password or the previously enrolled Touch ID on supported Macs. The large primary action starts the existing protected backup workflow; archive and restore shortcuts open the corresponding tools.'
            ],
            'Installation and platform requirements': [
                'Download the DMG for your Mac architecture (Apple Silicon or Intel). The mounted disk image contains Multi Backup.app and an Applications shortcut: drag the app onto Applications, then launch it from Applications.',
                'Early-release Mac packages use ad-hoc signing and are not Apple notarized. On the first launch approve through macOS Privacy & Security if necessary; do not disable Gatekeeper globally. Stable signed and notarized packages remain a separate qualification milestone.'
            ],
            'Dashboard tour': [
                'The premium photographic dashboard contains a landscape hero, rounded luminous actions, recent locally detected archives, vault unlock status, enabled GitHub/Dropbox plugins, actual update check status, disk usage and seven-day local archive activity.',
                'Disk usage refers to the destination filesystem and never represents an invented cloud quota. Scheduled plans shown in the approved design concept are not yet implemented; only actual saved archives appear.'
            ],
            'Backup workflow and progress': [
                'On Backup, choose destination and provider, then Start / resume backup. When an existing checkpoint is found, the log explicitly shows the resumed cycle identifier, checkpoint count and a repository progress summary. Completed encrypted archives are reused only after integrity checks.',
                'For GitHub, configure the number of complete CI runs per repository. If an artifact download remains unavailable after retries, Multi Backup attempts older workflow runs to fill the requested number. The optional Ignore CI artifact download errors setting permits omissions, but the log and manifest still identify every missing item.'
            ],
            'Resuming, restarting and migration': [
                'The backup log identifies resumption, verified checkpoints and current completion progress. A failed artifact does not terminate unrelated repositories; the engine retries and uses older CI executions when available. An incomplete cycle retains its checkpoint for another resume.',
                'When Ignore CI artifact download errors is enabled, the archive can contain an explicit omissions report and completion is flagged with warnings; it must not be treated as a lossless disaster-recovery copy. The option is off by default and can be enabled under Backup Advanced options or with --ignore-ci-artifact-errors.'
            ],
        },
        'IT': {
            'Panoramica e primo avvio': [
                'La dashboard fotografica approvata usa card blu, viola e ciano. Nella sidebar sono presenti Dashboard, Backup, Archivi, Ripristina, Plugin, Vault e chiavi, Aggiornamenti e Impostazioni. Gli indicatori mostrano soltanto dati realmente presenti sul disco o nelle impostazioni.',
                'All’avvio sbloccare il vault age con la password oppure, dopo abilitazione, con Touch ID sui Mac compatibili. Il pulsante principale avvia il flusso di backup già esistente e le scorciatoie rimandano alle rispettive schermate.'
            ],
            'Installazione e requisiti': [
                'Aprire il DMG adatto al Mac (Apple Silicon o Intel): contiene Multi Backup.app e il collegamento alla cartella Applicazioni. Trascinare Multi Backup.app sul collegamento Applicazioni e avviare l’app dalla cartella Applicazioni.',
                'I DMG early-release sono firmati ad hoc e non notarizzati da Apple. Se necessario autorizzare il primo avvio in Impostazioni di Sistema → Privacy e Sicurezza, senza disabilitare globalmente Gatekeeper. Le versioni stable necessitano ancora della qualificazione di firma e notarizzazione.'
            ],
            'Dashboard': [
                'La dashboard premium comprende fotografia panoramica, pulsanti arrotondati, archivi recenti realmente trovati, stato del vault, plugin GitHub/Dropbox abilitati, aggiornamenti, uso effettivo del disco e andamento locale degli archivi negli ultimi sette giorni.',
                'La capacità visualizzata riguarda il filesystem di destinazione e non una quota cloud. I piani automatici presenti nel mock concettuale non sono ancora disponibili: non vengono mostrati backup di esempio inventati.'
            ],
            'Esecuzione e avanzamento': [
                'In Backup selezionare il plugin e la destinazione, poi Avvia / riprendi backup. Se esiste un checkpoint compatibile, la finestra di log mostra esplicitamente la ripresa, l’identificativo del ciclo, il numero di checkpoint e l’avanzamento dei repository. I segmenti verificati sono riutilizzati senza sovrascriverli.',
                'Se fallisce il download di un artifact CI, il programma ritenta e prova esecuzioni CI precedenti per raggiungere il numero configurato di CI complete. L’opzione Ignora errori di download degli artifact CI, disattivata per default, consente omissioni dichiarate nel log e nel manifest.'
            ],
            'Ripresa, riavvio e migrazione': [
                'La ripresa segnala nel log il ciclo esistente, i checkpoint verificati e il progresso. Gli artifact non scaricabili non interrompono gli altri repository: vengono ritentati e, se possibile, sostituiti con altre esecuzioni CI.',
                'Se le CI complete non raggiungono il numero richiesto, per default il backup viene segnalato come incompleto e rimane riprendibile. Attivando Ignora errori di download degli artifact CI da Opzioni avanzate o con --ignore-ci-artifact-errors si possono accettare omissioni, sempre documentate come tali.'
            ],
        },
    }
    for language, chapters in (('EN',EN),('IT',IT)):
        for ix, original in enumerate(chapters):
            title=original[0]
            if title not in replacements[language]:
                continue
            new_paragraphs=replacements[language][title]
            chapters[ix]=(title,new_paragraphs,original[2],original[3],original[4],*original[5:])


refresh_chapters_for_current_interface()


def manual(lang):
    filename=('Multi-Backup-User-Manual-EN.pdf' if lang=='EN' else 'Multi-Backup-Manuale-Utente-IT.pdf')
    output=OUTMAN/filename
    doc=Book(output,lang,'manual')
    story=[]
    story.extend([p('Contents' if lang=='EN' else 'Indice','H1')])
    data=EN if lang=='EN' else IT
    for i,chapter_spec in enumerate(data,1):
        story.append(p(f'{i:02d}   {esc(chapter_spec[0])}','P'))
    story.extend([p('Note: screens are real captures of the RC Linux GUI, not fictitious live backups.' if lang=='EN'
                    else 'Nota: gli screenshot sono catture reali della GUI RC Linux, non simulazioni di backup effettuati.','Small'),PageBreak()])
    for i,item in enumerate(data,1):
        title,paras,items,screen,cap,*rest=item
        # odd shape optional example / extra
        example=rest[0] if rest else None
        story+=chapter(f'{i:02d}  {title}',paras,items,screen,cap,example)
    # last chapter has explicit PageBreak; harmless trailing blank may be avoided if needed
    doc.build(story)
    prepend_cover(doc,output)
    return output


def feature_box(title,body):
    return [p(esc(title),'H2'),p(body,'Small')]


def brochure(lang):
    filename='Multi-Backup-Brochure-'+lang+'.pdf'
    output=OUTBR/filename
    doc=Book(output,lang,'brochure')
    story=[]
    if lang=='EN':
        story+=[p('One app. Multiple backup sources.','H1'),p('A photographic premium dashboard for encrypted, resilient and resumable backups, with clear vault status, independent plugins and local archive activity.'),
                Spacer(1,7)]
        headings=[('GITHUB','Complete repository and CI recovery','Git mirrors, release assets, metadata, issues/PRs and optional cleanup of historical CI artifacts.'),
                  ('DROPBOX','Incremental file versions','Opt-in read-only plugin: changed revisions are encrypted once and safely reused in later cycles.'),
                  ('ENCRYPTION','Your keys, your control','Age X25519 encryption, private key generation, separated credentials and atomic output verification.')]
    else:
        story+=[p('Una sola app. Più sorgenti di backup.','H1'),p('Dashboard fotografica premium con backup cifrati e riprendibili, stato del vault, plugin indipendenti e attività reale degli archivi.'),Spacer(1,7)]
        headings=[('GITHUB','Disaster recovery dei repository','Mirror Git, release, metadata, issue/PR e cleanup facoltativo degli artifact CI storici.'),
                  ('DROPBOX','Versioni incrementali dei file','Plugin attivabile e in sola lettura: le revisioni non cambiate vengono cifrate una sola volta.'),
                  ('CIFRATURA','Controllo delle chiavi','Age X25519, generazione delle chiavi, credenziali separate e verifica degli archivi.')]
    for code,title,body in headings:story.append(panel(code+'  /  '+title,esc(body)));story.append(Spacer(1,9))
    story+=screenshot('dashboard.png',150*mm,('Actual premium photographic dashboard, '+VERSION if lang=='EN' else 'Dashboard fotografica premium reale, '+VERSION))
    story.append(PageBreak())
    story.extend([p('Engineered for clarity and control' if lang=='EN' else 'Sicurezza e controllo visibili','H1')])
    story.extend([p(('The photographic interface exposes actual local status, secure actions and archive history. Resume keeps verified encrypted shards; failed CI artifact downloads trigger retries and replacement with older runs where possible.'
                    if lang=='EN' else 'La nuova interfaccia fotografica mostra stato reale, plugin e archivi. I checkpoint mantengono gli archivi cifrati già verificati; gli artifact CI non scaricabili vengono ritentati e sostituiti, quando possibile, con esecuzioni precedenti.'))])
    story+=screenshot('backup.png',152*mm,'Backup operation and progress' if lang=='EN' else 'Operazioni di backup e avanzamento')
    story.extend([panel('PROTECT & BROWSE' if lang=='EN' else 'PROTEGGI & CONSULTA',
                        'Explore finished archives, inspect metadata, restore a Git working tree and manage the keys you need for recovery.' if lang=='EN'
                        else 'Esplora gli archivi, consulta i metadata, ricostruisci una working copy Git e gestisci le chiavi necessarie al ripristino.'),PageBreak()])
    story.extend([p('From source to recoverable archive' if lang=='EN' else 'Dalla sorgente all’archivio recuperabile','H1')])
    for i,(title,descr) in enumerate(([('SELECT','Choose a GitHub or Dropbox plugin.'),('ENCRYPT','Encrypt each successful shard with age.'),
                          ('VERIFY','Check SHA-256 and record a checkpoint.'),('EXPLORE','Browse, extract, checkout or mount for recovery.')]
                         if lang=='EN' else [('SELEZIONA','Scegli il motore GitHub o Dropbox.'),('CIFRA','Cifra ogni archivio completo con age.'),
                         ('VERIFICA','Controlla SHA-256 e salva il checkpoint.'),('RIPRISTINA','Esplora, estrai, effettua checkout o mount.')]),1):
        story.append(panel(f'{i:02d}  {title}',esc(descr)));story.append(Spacer(1,7))
    story.extend([p('RELEASE CHANNELS' if lang=='EN' else 'CANALI DI RILASCIO','H2')])
    story.append(p(('Early release: commit on main containing [full-ci]. Stable: push a stable version tag. macOS (ARM/Intel), Windows 11 x64 and Rocky/AlmaLinux 8-10 are part of the native build matrix.'
                     if lang=='EN' else 'Early release: commit su main contenente [full-ci]. Stable: push di un tag di versione stabile. La matrice nativa comprende macOS (ARM/Intel), Windows 11 x64 e Rocky/AlmaLinux 8-10.')))
    story.append(p('Release candidate: platform signing, notarization, installer end-to-end tests and fully unattended updates are pending.'
                   if lang=='EN' else 'Release candidate: firma dei pacchetti, notarizzazione, test completi degli installer e aggiornamenti non assistiti ancora da validare.','Small'))
    story.append(p('github.com/desalvo/multi-backup','H2'))
    doc.build(story)
    prepend_cover(doc,output)
    return output

if __name__=='__main__':
    for language in ('EN','IT'):
        for result in (manual(language),brochure(language)):
            print(result, result.stat().st_size)
