#!/usr/bin/env python3
"""Fresh bilingual Multi Backup brochure: real project status, not stale r018 imagery."""
from __future__ import annotations
import argparse
from html import escape
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (Image, KeepTogether, PageBreak, Paragraph,
                               SimpleDocTemplate, Spacer, Table, TableStyle)

ROOT = Path(__file__).resolve().parents[1]
NAVY = colors.HexColor("#122549")
BLUE = colors.HexColor("#216dd3")
TEAL = colors.HexColor("#1c8e98")
INK = colors.HexColor("#27374c")
GRAY = colors.HexColor("#586780")
PALE = colors.HexColor("#eaf1fa")

FONTS = Path("/usr/share/fonts/truetype/dejavu")
if FONTS.is_dir():
    pdfmetrics.registerFont(TTFont("MB", str(FONTS / "DejaVuSans.ttf")))
    pdfmetrics.registerFont(TTFont("MBB", str(FONTS / "DejaVuSans-Bold.ttf")))
    BASE, BOLD = "MB", "MBB"
else:
    BASE, BOLD = "Helvetica", "Helvetica-Bold"
STYLES = {
    "title": ParagraphStyle("mb_title", fontName=BOLD, fontSize=28, leading=35, textColor=NAVY, spaceAfter=12),
    "h1": ParagraphStyle("mb_h1", fontName=BOLD, fontSize=16, leading=22, textColor=NAVY, spaceBefore=14, spaceAfter=8),
    "h2": ParagraphStyle("mb_h2", fontName=BOLD, fontSize=11, leading=15, textColor=BLUE, spaceBefore=9, spaceAfter=5),
    "body": ParagraphStyle("mb_body", fontName=BASE, fontSize=9.1, leading=14.7, textColor=INK, spaceAfter=7),
    "small": ParagraphStyle("mb_small", fontName=BASE, fontSize=7.7, leading=12, textColor=GRAY, spaceAfter=6),
    "cell": ParagraphStyle("mb_cell", fontName=BASE, fontSize=7.5, leading=11.8, textColor=INK),
    "head": ParagraphStyle("mb_head", fontName=BOLD, fontSize=8, leading=12, textColor=colors.white),
}
def p(value, style="body"):
    return Paragraph(escape(value), STYLES[style])
def add_text(story, title, *paras):
    story.append(p(title, "h1"))
    story.extend(p(x) for x in paras)
def table(headings, rows, widths):
    matrix = [[p(x, "head") for x in headings]] + [
        [p(str(x), "cell") for x in row] for row in rows
    ]
    obj = Table(matrix, colWidths=widths, hAlign="LEFT", repeatRows=1)
    obj.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, 0), NAVY),
        ("ROWBACKGROUNDS", (0, 1), (-1, -1), [colors.white, PALE]),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("TOPPADDING", (0, 0), (-1, -1), 9),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 9),
        ("LEFTPADDING", (0, 0), (-1, -1), 8),
        ("RIGHTPADDING", (0, 0), (-1, -1), 8),
        ("LINEBELOW", (0, 0), (-1, 0), 0.5, BLUE)
    ]))
    return obj
def mark(page, doc):
    w, h = A4
    page.saveState()
    page.setFillColor(NAVY)
    page.rect(0, h - 10 * mm, w, 10 * mm, stroke=0, fill=1)
    page.setFillColor(colors.white)
    page.setFont(BOLD, 8)
    page.drawString(18 * mm, h - 6.7 * mm, "MULTI BACKUP  |  r218 DEVELOPMENT PREVIEW")
    page.setFillColor(GRAY)
    page.setFont(BASE, 7.3)
    page.drawString(18 * mm, 12 * mm, "EUPL-1.2  |  github.com/desalvo/multi-backup")
    page.drawRightString(w - 18 * mm, 12 * mm, str(doc.page))
    page.restoreState()

TEXT = {
 "IT": {
 "tag": "Backup cifrati, restore verificato e gestione remota multi-piattaforma.",
 "notice": "Edizione di sviluppo 18 novembre 2026. La r218 aggiunge sorgenti di client mobili non firmati; la RC r217 non è ancora certificata.",
 "intro": "Una piattaforma, più ambienti",
 "intro_text": "Motore Rust, server web autenticato, agenti e catalogo plugin per acquisire dati e ripristinarli in destinazioni nuove senza sovrascrivere gli originali. Archivi cifrati age, verifica SHA-256, checkpoint, audit e regole di sicurezza a privilegi minimi.",
 "news": "Principali novità dalla brochure precedente (r038)",
 "news_items": [
  "Client nativi REMOTI iOS/iPadOS (SwiftUI e WKWebView) e Android (Java e WebView), stessa console server e layout responsive per smartphone/tablet.",
  "App Rust desktop con installer reali: Ubuntu 24 .deb; macOS Apple Silicon e Intel .dmg; Windows x64 Inno con rendering software su host senza GPU.",
  "16 percorsi di provider READY soltanto per operazioni circoscritte, testati con laboratori veri; catalogo completo di 33 plugin. Ogni altro scenario resta preview.",
  "Server Rust con cluster PostgreSQL, agenti, Kubernetes, gestione job, audit, REST monitoraggio, SNMP, SMTP e policy di accesso.",
  "Vault cifrato con esperimenti MinIO Object Lock COMPLIANCE e IAM separato; l'immunità contro root/guasti fisici non è certificata.",
  "Laboratori reali MySQL 8.4, PostgreSQL/MariaDB, Oracle Free RMAN, CephFS/RBD, iSCSI, S3, NFS, SMB, GlusterFS, SSH/SSHFS, WebDAV e Lustre."
 ],
 "operations": "Plugin e perimetri verificati",
 "scope": "READY descrive almeno un percorso operativo precisamente delimitato, non la certificazione universale di un intero prodotto/provider.",
 "platform": "Piattaforme supportate e canali",
 "plthead": ["Componente", "Piattaforme", "Stato r218"],
 "platforms": [
  ("Desktop nativo Rust", "Windows x64; macOS Apple Silicon/Intel; Ubuntu 24 x86_64", "Build/installazione reali collaudate in r217; artefatti preview non firmati"),
  ("Server e agenti", "Linux; container OCI; Kubernetes; PostgreSQL condiviso", "Test software/cluster verdi; HA fisica a tre host non certificata"),
  ("RPM / altri Linux", "Rocky/Alma 8/9/10; Debian 12/13", "Ricette e target presenti, matrice firmata end-to-end da completare"),
  ("Mobile universale", "iOS 16+, iPadOS 16+; iOS Simulator", "Solo codice e build unsigned opzionali; nessun App Store"),
  ("Mobile Android", "Android 8+ / API 26+ su smartphone/tablet", "Solo codice e APK unsigned opzionale; nessun Play Store"),
  ("Browser", "Console responsive via HTTPS sul server", "Stesse API e sessioni del server; login locale")
 ],
 "status": "Stato della release candidate",
 "status_text": "La full CI r217 ha collaudato separatamente quattro installer nativi e molte famiglie di provider. Il gate complessivo richiede anche CephFS/RBD: un run fallito non genera la RC. Le build mobili sono volutamente indipendenti e non bloccano la RC.",
 "limits": "Limiti di certificazione ancora aperti",
 "limits_items": [
  "Firma Apple, notarizzazione, Windows Authenticode e package signing RPM/DEB.",
  "Tre host fisici HA con fencing e fault reali; restore boot di VM su VMware, oVirt, PVE/PBS.",
  "SAN CHAP/multipath, metadata POSIX ACL/xattr completi, Oracle RAC/ASM/TDE e MySQL GTID/PITR end-to-end.",
  "App mobile: IPA/APK non firmati non installabili tramite i normali canali; SSO IdP esterno non ancora integrato, nessun backup su device."
 ],
 "footer": "La fonte autorevole dello stato è la documentazione tecnica e la CI relativa all'esatto commit. Licenza EUPL-1.2."
 },
 "EN": {
 "tag": "Encrypted backups, verified recovery and cross-platform remote operations.",
 "notice": "Development edition, 18 November 2026. r218 adds unsigned mobile client sources; r217 RC is not yet certified.",
 "intro": "One platform, many environments",
 "intro_text": "Rust engine, authenticated web server, agents and a pluggable provider catalog for acquisition and new-target restore without overwriting original data. Age encryption, SHA-256 verification, checkpoints, audit logs and least-privilege security controls.",
 "news": "What's new since the previous r038 brochure",
 "news_items": [
  "Native REMOTE iOS/iPadOS (SwiftUI/WKWebView) and Android (Java/WebView) clients, sharing the server console with responsive mobile/tablet layouts.",
  "Rust desktop native installers: Ubuntu 24 .deb; macOS Apple Silicon and Intel .dmg; Windows x64 Inno with software rendering on GPU-less hosts.",
  "16 provider paths READY only for tightly bounded operations, field-tested in genuine labs; 33 total catalog entries. Other scopes remain preview.",
  "Rust server with PostgreSQL shared cluster, agents, Kubernetes, job orchestration, audit, monitoring REST, SNMP, SMTP and access policy.",
  "Encrypted vault and real MinIO Object Lock COMPLIANCE with separate IAM; root/physical immunity is not certified.",
  "Real labs for MySQL 8.4, PostgreSQL/MariaDB, Oracle Free RMAN, CephFS/RBD, iSCSI, S3, NFS, SMB, GlusterFS, SSH/SSHFS, WebDAV and Lustre."
 ],
 "operations": "Verified provider operation boundaries",
 "scope": "READY describes at least one precisely delimited operation, not universal certification of the entire provider.",
 "platform": "Platforms and distribution channels",
 "plthead": ["Component", "Platforms", "r218 status"],
 "platforms": [
  ("Native Rust desktop", "Windows x64; macOS Apple Silicon/Intel; Ubuntu 24 x86_64", "Actual installed previews tested in r217, unsigned"),
  ("Server and agents", "Linux; OCI containers; Kubernetes; shared PostgreSQL", "Software/cluster tests passing; physical three-host HA not certified"),
  ("RPM / other Linux", "Rocky/Alma 8/9/10; Debian 12/13", "Recipes/targets exist; signed full OS matrix remains open"),
  ("Universal mobile", "iOS 16+, iPadOS 16+; iOS Simulator", "Optional unsigned source/build, no App Store release"),
  ("Android mobile", "Android 8+ / API 26+, phones and tablets", "Optional unsigned APK source/build, no Play Store release"),
  ("Browser", "Responsive HTTPS web console", "Existing server APIs and sessions; local login")
 ],
 "status": "Release candidate status",
 "status_text": "r217 CI separately passed four real native installer tests and many provider field families. The complete gate also requires CephFS/RBD: failed runs cannot produce an RC. Mobile builds are intentionally independent and cannot block the RC.",
 "limits": "Certification gaps",
 "limits_items": [
  "Apple signing/notarization, Windows Authenticode, signed RPM/DEB packages.",
  "Real physical three-host HA/fencing and VM boot restore on VMware, oVirt, PVE/PBS.",
  "SAN CHAP/multipath, full POSIX ACL/xattr, Oracle RAC/ASM/TDE, integrated MySQL GTID/PITR.",
  "Unsigned mobile IPA/APK are not normally installable; external IdP SSO handoff, mobile backup engine and real-device acceptance are not implemented."
 ],
 "footer": "Exact-commit CI and technical documentation remain authoritative for readiness. EUPL-1.2."
 }
}
READY = [
 ("POSIX / PostgreSQL 16 / MariaDB-MySQL 8.4", "Quiesced/file or single database full/new-target restore only"),
 ("S3-MinIO / S3 WORM COMPLIANCE", "Pinned S3 object and immutable-policy tests, scoped IAM"),
 ("CephFS / Ceph RBD / Linux iSCSI", "Mounted file bytes or pinned snapshot to a NEW image/LUN"),
 ("Oracle RMAN / WebDAV", "Encrypted RMAN backup media, verified HTTPS DAV data transport"),
 ("SSH / SSHFS / Lustre", "Pinned file, mounted target and real kernel MDT/OST byte contents"),
 ("NFSv3 / SMB3 / GlusterFS", "Mounted file FULL/INC and new-path restore on genuine server labs")
]
def build(lang, destination):
    content = TEXT[lang]
    destination.parent.mkdir(parents=True, exist_ok=True)
    doc = SimpleDocTemplate(str(destination), pagesize=A4,
        leftMargin=18*mm, rightMargin=18*mm,
        topMargin=20*mm, bottomMargin=20*mm,
        title=f"Multi Backup r218 - {lang}",
        author="Multi Backup Project", subject="Cross-platform development brochure")
    story = [Spacer(1,12*mm)]
    logo = ROOT/"assets/icon-256.png"
    if logo.is_file():
        story.extend([Image(str(logo),width=30*mm,height=30*mm), Spacer(1,7*mm)])
    story.extend([p("Multi Backup", "title"),p(content["tag"],"h1"),
                  p(content["notice"],"small"),Spacer(1,9*mm)])
    add_text(story,content["intro"],content["intro_text"])
    story.append(Spacer(1,4*mm))
    story.append(p(content["news"],"h1"))
    for item in content["news_items"]:
        story.append(p("•  "+item))
    story.append(PageBreak())
    add_text(story,content["operations"],content["scope"])
    story.append(table(["Provider group","Exact qualified role"],READY,[71*mm,102*mm]))
    story.append(Spacer(1,7*mm))
    story.append(p(content["platform"],"h1"))
    story.append(table(content["plthead"],content["platforms"],[41*mm,62*mm,70*mm]))
    story.append(PageBreak())
    add_text(story,content["status"],content["status_text"])
    story.append(p(content["limits"],"h1"))
    for value in content["limits_items"]:
        story.append(p("•  "+value))
    story.extend([Spacer(1,10*mm),p(content["footer"],"small"),
                  p("github.com/desalvo/multi-backup", "h2")])
    doc.build(story,onFirstPage=mark,onLaterPages=mark)

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--out",type=Path,default=ROOT/"docs/brochures")
    args=parser.parse_args()
    for language in ("IT","EN"):
        output=args.out/f"Multi-Backup-Brochure-{language}.pdf"
        build(language,output)
        print(output,output.stat().st_size)
