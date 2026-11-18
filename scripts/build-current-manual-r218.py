#!/usr/bin/env python3
"""Authentic current-Rust (r218) EN/IT manuals.

Every screenshot MUST be captured from current Rust binaries/web assets in
the same CI SHA; legacy r018 Tk screenshots are forbidden. Web/mobile API
records are named fixtures, never misrepresented as live production.
"""
from __future__ import annotations
import argparse
import os
from html import escape
from pathlib import Path
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    Image,PageBreak,Paragraph,SimpleDocTemplate,Spacer,Table,TableStyle,
)
from PIL import Image as PIL
PAGES=A4
NAVY=colors.HexColor("#122549")
BLUE=colors.HexColor("#245aa2")
GRAY=colors.HexColor("#5b6881")
WHITE=colors.white
ROOT=Path(__file__).resolve().parents[1]
FONT_PATH=Path("/usr/share/fonts/truetype/dejavu")
if FONT_PATH.is_dir():
    pdfmetrics.registerFont(TTFont("MBA",str(FONT_PATH/"DejaVuSans.ttf")))
    pdfmetrics.registerFont(TTFont("MBB",str(FONT_PATH/"DejaVuSans-Bold.ttf")))
    REG,BOLD="MBA","MBB"
else:REG,BOLD="Helvetica","Helvetica-Bold"
ST={
    "title":ParagraphStyle("m_title",fontName=BOLD,fontSize=25,leading=33,textColor=NAVY,spaceAfter=14),
    "h":ParagraphStyle("m_h",fontName=BOLD,fontSize=16,leading=23,textColor=NAVY,spaceBefore=16,spaceAfter=11),
    "p":ParagraphStyle("m_p",fontName=REG,fontSize=9.4,leading=15.4,spaceAfter=11),
    "label":ParagraphStyle("m_label",fontName=BOLD,fontSize=9.4,leading=14,textColor=BLUE,spaceBefore=8,spaceAfter=7),
    "small":ParagraphStyle("m_small",fontName=REG,fontSize=8.2,leading=12.5,textColor=GRAY,spaceAfter=9),
    "cli":ParagraphStyle("m_cli",fontName=REG,fontSize=8.2,leading=14,textColor=NAVY,
        backColor=colors.HexColor("#eaf1fa"),borderPadding=9,spaceAfter=13),
}
def par(text,style="p"):
    return Paragraph(escape(text),ST[style])
def banner(c,doc):
    w,h=PAGES
    c.saveState()
    c.setFillColor(NAVY)
    c.rect(0,h-11*mm,w,11*mm,stroke=0,fill=1)
    c.setFillColor(WHITE);c.setFont(BOLD,8)
    c.drawString(18*mm,h-7.3*mm,"MULTI BACKUP · CURRENT RUST INTERFACE · r218 PREVIEW")
    c.setFillColor(GRAY);c.setFont(REG,7.2)
    c.drawString(18*mm,12*mm,"EUPL-1.2 · GitHub desalvo/multi-backup")
    c.drawRightString(w-18*mm,12*mm,str(doc.page))
    c.restoreState()
def screen(root,name,caption):
    path=root/name
    if not path.is_file():
        raise FileNotFoundError(f"Missing real UI capture {path}; refusing legacy replacement")
    with PIL.open(path) as im:
        width,height=im.size
    if width<360 or height<300:raise RuntimeError(f"Invalid UI capture {path}")
    scale=min((171*mm)/width,(112*mm)/height)
    return [Image(str(path),width=width*scale,height=height*scale),
            par(caption,"small")]
DOC={
"IT":{
"title":"Manuale utente · interfacce Rust",
"notice":"Versione di sviluppo r218, non distribuzione stabile. Immagini desktop acquisite dall'applicazione Rust in esecuzione sotto Xvfb; immagini server/mobile acquisite dal frontend Rust corrente usando dati sintetici di test, non account reali.",
"overview":"Panoramica e stato",
"overview_p":["Multi Backup integra motore Rust, console server HTTPS, agenti e plugin per backup cifrati e ripristini su destinazioni nuove. La release r217 ha superato il gate CI della RC0 non firmata; cloud e mobile r218 restano Preview.","I pulsanti nella console operano secondo autorizzazioni, disponibilità dei provider e verifiche effettive. Un provider elencato non implica certificazione completa delle operazioni."],
"dashboard":"Desktop Rust · Dashboard",
"dashboard_p":["Questa è la dashboard egui nativa corrente, non una schermata Python/Tk. Le sezioni includono Dashboard, Backup, Archivi, Ripristina, Storage chunk, Plugin, Server e agenti, Vault e chiavi, Aggiornamenti e Impostazioni.","Le schermate esemplificative vengono acquisite da un profilo temporaneo senza credenziali. L'assenza di job o account è intenzionale."],
"storage":"Compressione e preparazione del backup",
"storage_p":["Un nuovo storage a chunk usa zstd livello 3 come compressore predefinito. Sono selezionabili xz (contenitore XZ con algoritmo LZMA2, livello 6), gzip e none. La scelta è salvata nella configurazione cifrata del singolo storage e vale per tutti i suoi chunk.","Un vecchio storage privo del campo compressore viene letto come zstd: non riscrivere o ricomprimere in-place i chunk immutabili. Per cambiare algoritmo creare un nuovo storage e verificare backup, esportazione ZIP64 e restore.", "La cifratura age protegge i chunk compressi; i manifest e gli identificativi verificano la ricostruzione. Le modalità none/gzip/xz non modificano le regole di overwrite o di protezione."],
"storage_cli":"multi-backup-native storage init --root /private/new-store --recipient age1... --chunk-mib 4 --compressor xz",
"backup":"Desktop Rust · Esecuzione del backup",
"backup_p":["Selezionare sorgente, destinazione e identità richiesta dal job. Monitorare progressi, eventuali warning e completamento reale; un job interrotto non è un successo. Le schermate contengono solo operazioni disponibili per il relativo plugin.","Per GitHub e Dropbox la procedura completa dipende dai permessi autorizzati e dalla disponibilità dei servizi. Conservare la chiave privata age lontano dal disco di backup."],
"plugins":"Desktop Rust · Catalogo plugin",
"plugins_p":["Il catalogo comprende 34 plugin; READY identifica soltanto alcuni perimetri operativi esplicitamente collaudati, non una compatibilità universale. Verificare sempre il ruolo B-In/B-Out, il tipo di restore e lo stato del test sul provider reale.","I sette Cloud Drive (Dropbox, Google Drive, iCloud, OneDrive, ownCloud, Nextcloud, Box) usano una configurazione rclone privata e restano Preview fino a collaudo con account veri."],
"archive":"Archivi e recupero selettivo",
"archive_p":["Il server /api/backups elenca manifest logici autenticati; l'API /api/export ricostruisce file/cartelle selezionati su una directory nuova del server. Il restore non sovrascrive per impostazione predefinita.","La console cloud può elencare file di una directory remota, ma non deve confondere tali nomi con il contenuto verificato dei backup ZIP64. Per questi si utilizza la specifica funzione cloud-backup-inspect/restore."],
"cloud":"Server Rust · Cloud Drive",
"cloud_p":["L'immagine mostra il vero HTML, CSS e JavaScript del frontend server Rust corrente, eseguito con dati di test. Navigazione e download sono in sola lettura per default. Upload e delete sono opt-in amministrativi; la cancellazione non garantisce controllo atomico di versione su tutti i provider.","Il server non rivela mai i token rclone al browser. Per la ricerca di backup remoti, selezionare una cartella con bundle.index.age e part-NNNNNN.zip; verificare l'intero bundle prima dell'estrazione."],
"cloud_cli":"multi-backup-native plugins cloud-backup-inspect --config private-rclone.conf --provider nextcloud --directory Backups/snap1 --identity private-key.txt --work-dir /private/work --json",
"restore_cli":"multi-backup-native plugins cloud-backup-restore --config private-rclone.conf --provider nextcloud --directory Backups/snap1 --identity private-key.txt --work-dir /private/work --destination /private/new-restore --selection documents/example.txt --confirm RESTORE:nextcloud:Backups/snap1 --json",
"security":"Sicurezza, policy e limitazioni",
"security_p":["L'import remoto scarica soltanto indice cifrato e volumi attesi; verifica SHA-256 per ogni ZIP, autenticazione age e integrità dei chunk/manifest prima di pubblicare un restore selettivo create-only. Richiede area di lavoro privata e disponibilità di spazio, fino al limite di trasferimento configurato.","Le API server /api/cloud/backup/inspect e /api/cloud/backup/restore richiedono MULTI_BACKUP_CLOUD_RECOVERY_ENABLED=1, HTTPS hardened, amministratore, CSRF e autorizzazione offline. Si tratta ancora di un flusso Preview: le versioni remote non sono bloccate atomicamente e il restore remoto diretto ad altro provider non è implementato.","Le schermate mobile mostrano il frontend responsive effettivo. I client iOS/iPadOS e Android sono contenitori remoti unsigned, non motori per il backup dei dati del dispositivo."],
"final":"Ambito delle verifiche",
"final_p":["I PDF rigenerati non costituiscono una certificazione di firma, notarizzazione o disponibilità pubblica. La CI deve acquisire le immagini dallo stesso commit del codice e rifiutare qualsiasi schermata legacy come rappresentazione attuale.","La release r217 è il riferimento della RC0 testata; r218 cloud e mobile restano separati, in revisione nella PR #55."]
},
"EN":{
"title":"User manual · current Rust UI",
"notice":"r218 development preview, not a stable distribution. Native desktop screenshots are captured from the real running Rust egui app under Xvfb. Web/mobile screenshots use current Rust-server frontend assets and explicit synthetic API fixtures, not real customer data.",
"overview":"Overview and readiness",
"overview_p":["Multi Backup includes a Rust engine, HTTPS web server, agents and plugins for encrypted backup and create-only recovery. r217 passed the unsigned RC0 CI gate; r218 cloud and mobile remain Preview.","Visible providers and buttons never imply universal field certification; check the precise role, test scope and permissions."],
"dashboard":"Rust desktop · Dashboard",
"dashboard_p":["This is the actual current native egui dashboard, not historical Python/Tk. The sidebar has Dashboard, Backup, Archives, Restore, Chunk Storage, Plugins, Server and Agents, Keys and Vault, Updates and Settings.","Documentation screenshots are taken with a disposable, credential-free user profile. Empty jobs and unconfigured accounts are intentional."],
"storage":"Compression and storage initialization",
"storage_p":["New encrypted chunk stores default to zstd level 3. Additional codecs: xz container using LZMA2 level 6, gzip and none. The selection is stored in encrypted per-store configuration and applies to its immutable chunks.","An existing store without the codec attribute is interpreted as zstd, retaining backward compatibility. Do not rewrite or recompress immutable chunks in place; initialize a new store and test ZIP64 export/import and restore.","All compression choices are followed by age encryption and the same authenticated manifest, chunk hashes and create-only recovery policy."],
"storage_cli":"multi-backup-native storage init --root /private/new-store --recipient age1... --chunk-mib 4 --compressor xz",
"backup":"Rust desktop · Backup execution",
"backup_p":["Choose an authorized input plugin and a new output workspace and provide the required secret material separately. Monitor actual job progress, warnings and final validation. An interrupted job is not a success.","Keep the age private identity separate from the backup media. GitHub and Dropbox integration depend on their actual account permissions."],
"plugins":"Rust desktop · Plugin catalog",
"plugins_p":["There are 34 listed plugins; READY is limited to explicitly certified narrow scopes, never the whole backend in all configurations. Review B-In/B-Out operations, restore target type and live tests.","Seven supported Cloud Drive browser backends (Dropbox, Google Drive, iCloud, OneDrive, ownCloud, Nextcloud and Box) use a private external rclone configuration and are still Preview."],
"archive":"Logical archives and selective recovery",
"archive_p":["The server /api/backups route lists authenticated logical manifests; /api/export restores selected files and folders into a new server-side directory. Never assume existing data will be overwritten.","A cloud-directory listing is not the same as inspecting the authenticated contents of an encrypted backup. Portable ZIP64 bundles require cloud-backup-inspect and cloud-backup-restore."],
"cloud":"Rust server · Cloud Drive",
"cloud_p":["The screenshot is made from the real HTML/CSS/JS served by the current Rust server, with test fixture data. Browsing and downloads are read-only by default. Upload/delete require administrative opt-in; atomic remote version compare/delete is not universally available.","rclone tokens remain on the server. A remote portable backup directory consists of bundle.index.age and exact part-NNNNNN.zip volumes; they are verified before restoration."],
"cloud_cli":"multi-backup-native plugins cloud-backup-inspect --config private-rclone.conf --provider nextcloud --directory Backups/snap1 --identity private-key.txt --work-dir /private/work --json",
"restore_cli":"multi-backup-native plugins cloud-backup-restore --config private-rclone.conf --provider nextcloud --directory Backups/snap1 --identity private-key.txt --work-dir /private/work --destination /private/new-restore --selection documents/example.txt --confirm RESTORE:nextcloud:Backups/snap1 --json",
"security":"Security and open boundaries",
"security_p":["Cloud recovery downloads only encrypted index and declared volumes, checks each ZIP SHA-256, age authenticity, chunk integrity and manifest before create-only selective restore. Staging uses a private local workspace and a configured byte cap.","The server's /api/cloud/backup/inspect and /api/cloud/backup/restore endpoints require MULTI_BACKUP_CLOUD_RECOVERY_ENABLED=1, hardened HTTPS, admin, CSRF and offline recovery authorization. Cloud version fencing and direct restore into a new cloud destination are still missing.","The mobile screenshot is of the responsive real server frontend. Unsigned iOS/iPadOS/Android clients are remote containers, not local device-backup engines."],
"final":"Scope of evidence",
"final_p":["The regenerated PDF does not certify installer signing, notarization or public release. All screenshots must be taken from the same Rust source commit and historical Python/Tk screenshots must never be labeled current.","r217 is the tested RC0 baseline; r218 cloud and mobile are under review in draft PR #55."]
}}
def chapter(story,root,title,paragraphs,photo=None,caption=None,command=None):
    story.append(par(title,"h"))
    for text in paragraphs:story.append(par(text))
    if command:story.append(par(command,"cli"))
    if photo:story.extend(screen(root,photo,caption))
    story.append(PageBreak())
def build(lang,root,destination):
    d=DOC[lang]
    story=[Spacer(1,12*mm),par("Multi Backup","title"),par(d["title"],"h"),
           par(d["notice"]),Spacer(1,8*mm)]
    chapter(story,root,d["overview"],d["overview_p"])
    chapter(story,root,d["dashboard"],d["dashboard_p"],"desktop-dashboard.png",
           "Real native Rust desktop application / Applicazione Rust reale.")
    chapter(story,root,d["storage"],d["storage_p"],command=d["storage_cli"])
    chapter(story,root,d["backup"],d["backup_p"],"desktop-backup.png",
           "Real Rust desktop backup page / Pagina backup Rust reale.")
    chapter(story,root,d["plugins"],d["plugins_p"],"desktop-plugins.png",
           "Actual Rust plugin registry / Registro plugin nell'applicazione Rust.")
    chapter(story,root,d["archive"],d["archive_p"],"web-archives-fixture.png",
           "Real Rust-server frontend with SYNTHETIC test data / Interfaccia server Rust con dati SIMULATI.")
    chapter(story,root,d["cloud"],d["cloud_p"],"web-cloud-drives-fixture.png",
           "Actual Rust-server Cloud UI, synthetic API fixture / UI Cloud Rust reale, dati di test.",
           d["cloud_cli"])
    chapter(story,root,d["security"],d["security_p"],"mobile-cloud-drive-fixture.png",
           "Real responsive server UI, synthetic fixture; NOT an iOS/Android device capture.",
           d["restore_cli"])
    story.append(par(d["final"],"h"))
    for text in d["final_p"]:story.append(par(text))
    destination.parent.mkdir(parents=True,exist_ok=True)
    doc=SimpleDocTemplate(str(destination),pagesize=A4,leftMargin=19*mm,rightMargin=19*mm,
        topMargin=21*mm,bottomMargin=21*mm,title="Multi Backup r218 - "+d["title"],
        author="Multi Backup Project",subject="Current Rust interface user manual")
    doc.build(story,onFirstPage=banner,onLaterPages=banner)
def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--screenshots",type=Path,required=True)
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    for lang in ("IT","EN"):
        name="Multi-Backup-Manuale-Utente-IT.pdf" if lang=="IT" else "Multi-Backup-User-Manual-EN.pdf"
        path=args.out/name
        build(lang,args.screenshots,path)
        print(lang,path,path.stat().st_size)
if __name__=="__main__":main()
