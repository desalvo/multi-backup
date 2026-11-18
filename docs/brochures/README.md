# Brochure bilingui — r218 (2026-11-18)

**Fonte corrente:** `scripts/build-brochure-r218.py`. Genera PDF A4 in italiano e inglese con tutte le novità successive alla brochure storica r038, piattaforme server/desktop/browser/mobile e perimetri dei 16/34 plugin READY.

Eseguire `python3 scripts/build-brochure-r218.py --out generated-brochures` dopo aver installato reportlab, oppure scaricare gli artefatti del workflow separato `Multi Backup r218 - optional bilingual current brochures` sulla stessa SHA.

**Attenzione:** i file PDF con nome `Multi-Backup-Brochure-IT.pdf` e `Multi-Backup-Brochure-EN.pdf` già versionati in questa cartella appartengono alle generazioni precedenti e non vanno interpretati come brochure correnti finché non vengono sostituiti con i PDF rigenerati e verificati. La brochure r218 è una preview tecnica; non dichiara RC/stable certificata e non promette APK/IPA installabili.


## r218 — screenshot delle interfacce correnti

Il generatore scripts/build-brochure-r218.py ora richiede --screenshots DIR, con catture del vero desktop Rust egui e del reale frontend server Rust (quest'ultimo con dati sintetici marcati). Non può più rigenerare nuove brochure usando screenshot Python/Tk: senza le immagini della CI fallisce esplicitamente. Workflow brochures-r218.yml: pubblicazioni IT/EN ed eventuale sostituzione dei PDF tracciati solo dopo verifica dell'interfaccia e del PDF.
