//! Standalone Rust GUI preview. This is NOT a Python/Tk wrapper.
//! Backend functions are the same compiled Rust operations as the native CLI.
use eframe::egui::{self, Align, Color32, Context, RichText, TextureHandle, Vec2};
use multi_backup_core::{archive, dropbox_client, github_client, progress_rgb, vault};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroize;

const VERSION: &str = "0.1.0-r020-native-preview";
const AUTHOR: &str = "Alessandro De Salvo";
const REPO: &str = "https://github.com/desalvo/multi-backup";
const NAVY: Color32 = Color32::from_rgb(10, 26, 60);
const PANEL: Color32 = Color32::from_rgb(34, 54, 105);
const BORDER: Color32 = Color32::from_rgb(97, 119, 190);
const INK: Color32 = Color32::from_rgb(235, 243, 255);
const MUTED: Color32 = Color32::from_rgb(179, 199, 231);
const VIOLET: Color32 = Color32::from_rgb(113, 87, 231);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Page {
    Dashboard, Backup, Archives, Restore, Plugins, Vault, Updates, Settings,
}
impl Page {
    const ALL: [Self; 8] = [
        Self::Dashboard, Self::Backup, Self::Archives, Self::Restore,
        Self::Plugins, Self::Vault, Self::Updates, Self::Settings,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard", Self::Backup => "Backup",
            Self::Archives => "Archivi", Self::Restore => "Ripristina",
            Self::Plugins => "Plugin", Self::Vault => "Vault e chiavi",
            Self::Updates => "Aggiornamenti", Self::Settings => "Impostazioni",
        }
    }
}
enum Event {
    Status(String),
    Progress(f32, String),
    Success(String),
    Failure(String),
}
#[derive(Default)]
struct Fields {
    output: String,
    recipient: String,
    identity: String,
    selected_archive: String,
    destination: String,
    vault_path: String,
    password: String,
    password_confirm: String,
    recovery_path: String,
    dropbox_path: String,
    max_runs: String,
}
fn load_photo(ctx: &Context, key: &str, raw: &[u8]) -> Option<TextureHandle> {
    let input = image::load_from_memory(raw).ok()?.to_rgba8();
    let (w, h) = input.dimensions();
    let picture = egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize], input.as_raw()
    );
    Some(ctx.load_texture(key, picture, egui::TextureOptions::LINEAR))
}
fn default_output() -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    Path::new(&home).join("Multi-Backups").display().to_string()
}
fn archive_catalog(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() { return Vec::new(); }
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false)
        .max_depth(9).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() { continue; }
        if entry.path().to_string_lossy().ends_with(".tar.gz.age") {
            result.push(entry.into_path());
        }
    }
    result.sort_by(|a, b| b.cmp(a));
    result
}
struct NativeApp {
    fields: Fields,
    page: Page,
    sidebar: bool,
    about: bool,
    provider: String,
    status: String,
    active: bool,
    started: Instant,
    last_activity: Instant,
    progress: Option<f32>,
    receiver: Option<Receiver<Event>>,
    hero: Option<TextureHandle>,
    logo: Option<TextureHandle>,
    entries: Vec<PathBuf>,
    count: usize,
    vault_verified: bool,
    smoke_frames: Option<usize>,
}
impl NativeApp {
    fn new(ctx: &egui::Context, smoke: bool) -> Self {
        ctx.set_visuals(egui::Visuals::dark());
        ctx.style_mut(|style| {
            style.visuals.panel_fill = NAVY;
            style.visuals.window_fill = PANEL;
            style.visuals.widgets.inactive.bg_fill = PANEL;
            style.visuals.widgets.hovered.bg_fill = VIOLET;
            style.visuals.widgets.active.bg_fill = VIOLET;
            style.visuals.widgets.noninteractive.fg_stroke.color = INK;
            style.spacing.item_spacing = Vec2::new(10.0, 10.0);
            style.spacing.button_padding = Vec2::new(15.0, 9.0);
        });
        let fields = Fields {
            output: default_output(),
            vault_path: Path::new(&default_output()).join("keys")
                .join("default.agevault").display().to_string(),
            max_runs: "5".into(),
            ..Fields::default()
        };
        let entries = archive_catalog(Path::new(&fields.output));
        Self {
            fields, page: Page::Dashboard, sidebar: true, about: false,
            provider: "github".into(),
            status: "Nessun backup attivo".into(),
            active: false, started: Instant::now(),
            last_activity: Instant::now(), progress: None, receiver: None,
            hero: load_photo(ctx, "backup-hero", include_bytes!("../../assets/hero.jpg")),
            logo: load_photo(ctx, "multi-backup-logo", include_bytes!("../../assets/icon-256.png")),
            count: entries.len(), entries, vault_verified: false,
            smoke_frames: if smoke { Some(0) } else { None },
        }
    }
    fn rescan(&mut self) {
        self.entries = archive_catalog(Path::new(&self.fields.output));
        self.count = self.entries.len();
    }
    fn run_backup(&mut self, resume: bool) {
        if self.active { return; }
        if self.fields.recipient.trim().is_empty() {
            self.status = "Serve la chiave pubblica age del vault".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let output = PathBuf::from(self.fields.output.trim());
        let recipient = self.fields.recipient.trim().to_owned();
        let provider = self.provider.clone();
        let remote = self.fields.dropbox_path.clone();
        let max_runs = self.fields.max_runs.parse::<usize>().unwrap_or(5).min(100_000);
        self.active = true;
        self.started = Instant::now();
        self.last_activity = Instant::now();
        self.progress = None;
        self.status = format!("{}: {} in corso", provider, if resume {"Resume"} else {"Start"});
        self.receiver = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(Event::Status("Preparazione e verifica dei checkpoint…".into()));
            let result = if provider == "github" {
                github_client::backup(&output, &recipient, resume, max_runs)
                    .map_err(|e| e.to_string())
            } else {
                dropbox_client::backup(&output, &remote, &recipient, resume)
                    .map_err(|e| e.to_string())
            };
            match result {
                Ok(info) => {
                    let _ = tx.send(Event::Progress(1.0, "Backup verificato".into()));
                    let _ = tx.send(Event::Success(format!(
                        "Completato: {}",
                        info.get("backend").and_then(|v|v.as_str()).unwrap_or("backup")
                    )));
                }
                Err(reason) => { let _ = tx.send(Event::Failure(reason)); }
            }
        });
    }
    fn run_restore(&mut self, verify_only: bool) {
        if self.active { return; }
        let path = self.fields.selected_archive.clone();
        let identity = self.fields.identity.clone();
        if path.trim().is_empty() || identity.trim().is_empty() {
            self.status = "Seleziona archivio cifrato e file chiave age".into();
            return;
        }
        let dest = self.fields.destination.clone();
        if !verify_only && dest.trim().is_empty() {
            self.status = "Serve una destinazione di ripristino NUOVA".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.active = true;
        self.progress = None;
        self.started = Instant::now();
        self.last_activity = Instant::now();
        thread::spawn(move || {
            let _ = tx.send(Event::Status("Autenticazione age e verifica archivio…".into()));
            let outcome = if verify_only {
                archive::verify(Path::new(&path), Path::new(&identity))
                    .map(|size| format!("Archivio verificato: {size} elementi"))
            } else {
                archive::extract(Path::new(&path), Path::new(&identity), Path::new(&dest))
                    .map(|members| format!(
                        "Ripristinati {} elementi in {}", members.len(), dest
                    ))
            };
            match outcome {
                Ok(message) => {let _ = tx.send(Event::Success(message));}
                Err(error) => {let _ = tx.send(Event::Failure(error.to_string()));}
            }
        });
    }
    fn poll(&mut self) {
        let mut completed = false;
        if let Some(receiver) = &self.receiver {
            while let Ok(event) = receiver.try_recv() {
                self.last_activity = Instant::now();
                match event {
                    Event::Status(s) => self.status = s,
                    Event::Progress(p, s) => {
                        self.progress = Some(p.clamp(0.0, 1.0)); self.status = s;
                    }
                    Event::Success(s) => {
                        self.status = s; self.progress = Some(1.0);
                        self.active = false; completed = true;
                    }
                    Event::Failure(s) => {
                        self.status = format!("Errore: {}", s.chars().take(320).collect::<String>());
                        self.active = false; completed = true;
                    }
                }
            }
        }
        if completed { self.receiver = None; self.rescan(); }
    }
    fn draw_progress(&self, ui: &mut egui::Ui) {
        let width = ui.available_width().max(180.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 11.0, Color32::from_rgb(16, 36, 77));
        let fraction = self.progress.unwrap_or(0.0).clamp(0.0, 1.0);
        let pixels = (width * fraction).ceil().max(0.0) as usize;
        for x in 0..pixels {
            let t = x as f64 / width as f64 * 100.0;
            let (r,g,b) = progress_rgb(t);
            let left = rect.left() + x as f32;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(egui::pos2(left, rect.top() + 3.0),
                    Vec2::new(1.15, 18.0)), 0.0, Color32::from_rgb(r,g,b));
        }
        // An independent small pulse moves only when the native job is active
        // and has emitted an actual recent event. No guessed percentage.
        if self.active && self.last_activity.elapsed() < Duration::from_secs(8) {
            let phase = (self.started.elapsed().as_secs_f32() / 2.4).fract();
            let bounce = 1.0 - (2.0 * phase - 1.0).abs();
            let x = rect.left() + 15.0 + bounce * (width - 30.0);
            ui.painter().circle_filled(egui::pos2(x, rect.center().y), 6.0,
                Color32::from_rgb(218, 245, 255));
        }
        if let Some(p) = self.progress {
            ui.label(RichText::new(format!("{}%", (p * 100.0).round() as i32))
                .color(INK).strong());
        } else {
            ui.label(RichText::new("Avanzamento non quantificato: nessuna percentuale simulata")
                .color(MUTED).small());
        }
    }
    fn card(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::group(ui.style()).fill(PANEL).stroke(egui::Stroke::new(1.0, BORDER))
            .show(ui, |ui| {
                ui.set_min_width(130.0);
                ui.label(RichText::new(title).color(MUTED).strong());
                content(ui);
            });
    }
    fn dashboard(&mut self, ui: &mut egui::Ui) {
        if let Some(hero) = &self.hero {
            let size = hero.size_vec2();
            let width = ui.available_width();
            let height = (width * size.y / size.x).min(290.0);
            ui.image((hero.id(), Vec2::new((height * size.x/size.y).min(width), height)));
        }
        ui.heading(RichText::new("Multi Backup").size(33.0).color(INK));
        ui.label(RichText::new("I tuoi dati, sempre al sicuro — backup cifrati e ripristino")
            .color(MUTED));
        ui.horizontal_wrapped(|ui| {
            Self::card(ui, "ARCHIVI LOCALI", |ui| {
                ui.label(RichText::new(self.count.to_string()).size(25.0).color(INK));
            });
            Self::card(ui, "CIFRATURA", |ui| {
                ui.label(RichText::new("age / AES-256-GCM").size(18.0).color(INK));
            });
            Self::card(ui, "PROVIDER", |ui| {
                ui.label(RichText::new(self.provider.as_str()).size(21.0).color(INK));
            });
            Self::card(ui, "BUILD", |ui| {
                ui.label(RichText::new(VERSION).color(INK));
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.active,
                egui::Button::new(RichText::new("▶ Start backup").color(INK))
                    .fill(VIOLET)).clicked() { self.run_backup(false); }
            if ui.add_enabled(!self.active,
                egui::Button::new(RichText::new("↻ Resume backup").color(INK))
                    .fill(Color32::from_rgb(28, 125, 83))).clicked() {
                self.run_backup(true);
            }
            if ui.button("Configura…").clicked() { self.page = Page::Settings; }
        });
        ui.separator();
        Self::card(ui, "ATTIVITÀ BACKUP", |ui| {
            ui.label(RichText::new(&self.status).color(INK));
            self.draw_progress(ui);
        });
        ui.add_space(8.0);
        Self::card(ui, "ULTIMI ARCHIVI CIFRATI", |ui| {
            for entry in self.entries.iter().take(6) {
                let name = entry.file_name().unwrap_or_default().to_string_lossy();
                if ui.selectable_label(self.fields.selected_archive == entry.to_string_lossy(),
                    name).clicked() {
                    self.fields.selected_archive = entry.display().to_string();
                    self.page = Page::Restore;
                }
            }
        });
    }
    fn backup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Backup nativo");
        ui.label("I dati vengono compressi e cifrati direttamente dal motore Rust.");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.provider, "github".to_owned(), "GitHub");
            ui.selectable_value(&mut self.provider, "dropbox".to_owned(), "Dropbox");
        });
        ui.label("Directory privata di destinazione");
        ui.text_edit_singleline(&mut self.fields.output);
        ui.label("Destinatario pubblico age (age1…) ");
        ui.text_edit_singleline(&mut self.fields.recipient);
        if self.provider == "dropbox" {
            ui.label("Percorso Dropbox, vuoto per l'intero account");
            ui.text_edit_singleline(&mut self.fields.dropbox_path);
            ui.label("Token letto solo da MULTI_BACKUP_DROPBOX_TOKEN, mai salvato");
        } else {
            ui.horizontal(|ui| {
                ui.label("Numero di run CI completi / repo (0=tutti)");
                ui.text_edit_singleline(&mut self.fields.max_runs);
            });
            ui.label("Autenticazione tramite gh auth; nessuna credenziale salvata nella GUI.");
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.active, egui::Button::new("Start nuovo backup")
                .fill(VIOLET)).clicked() { self.run_backup(false); }
            if ui.add_enabled(!self.active, egui::Button::new("Resume senza rinominare")
                .fill(Color32::from_rgb(23, 117, 83))).clicked() {
                self.run_backup(true);
            }
        });
        ui.label(RichText::new(&self.status).color(INK));
        self.draw_progress(ui);
    }
    fn archives(&mut self, ui: &mut egui::Ui) {
        ui.heading("I tuoi backup protetti");
        ui.horizontal(|ui| {
            if ui.button("Aggiorna elenco").clicked() { self.rescan(); }
            ui.label(format!("{} archivi cifrati", self.count));
        });
        egui::ScrollArea::vertical().max_height(520.).show(ui, |ui| {
            for item in &self.entries {
                let path = item.display().to_string();
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.fields.selected_archive == path,
                        item.file_name().unwrap_or_default().to_string_lossy()).clicked() {
                        self.fields.selected_archive = path.clone();
                    }
                    let bytes = item.metadata().map(|v| v.len()).unwrap_or(0);
                    ui.label(format!("{:.1} MiB", bytes as f64 / (1024.0*1024.0)));
                });
            }
        });
        if ui.button("Verifica / Ripristina archivio selezionato").clicked() {
            self.page = Page::Restore;
        }
    }
    fn restore(&mut self, ui: &mut egui::Ui) {
        ui.heading("Ripristino cifrato");
        ui.label("Il motore Rust richiede una directory NUOVA e autentica tutto lo stream age.");
        ui.label("Archivio .tar.gz.age");
        ui.text_edit_singleline(&mut self.fields.selected_archive);
        ui.label("File di identità privata age (non salvato nelle preferenze)");
        ui.text_edit_singleline(&mut self.fields.identity);
        ui.label("Nuova directory di destinazione (non può esistere)");
        ui.text_edit_singleline(&mut self.fields.destination);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.active, egui::Button::new("Verifica integrità"))
                .clicked() { self.run_restore(true); }
            if ui.add_enabled(!self.active, egui::Button::new("Ripristina")
                .fill(VIOLET)).clicked() { self.run_restore(false); }
        });
        ui.label(RichText::new(&self.status).color(INK));
        self.draw_progress(ui);
    }
    fn plugins(&mut self, ui: &mut egui::Ui) {
        ui.heading("Provider nativi");
        Self::card(ui, "GitHub", |ui| {
            ui.label("Git mirror, metadata e Actions CI tramite gh/git con Rust.");
            if ui.button("Seleziona GitHub").clicked() {
                self.provider = "github".into(); self.page = Page::Backup;
            }
        });
        Self::card(ui, "Dropbox", |ui| {
            ui.label("Download TLS read-only, revisioni e archivi cifrati.");
            if ui.button("Seleziona Dropbox").clicked() {
                self.provider = "dropbox".into(); self.page = Page::Backup;
            }
        });
    }
    fn vault(&mut self, ui: &mut egui::Ui) {
        ui.heading("Vault protetto");
        ui.label("Vault AES-256-GCM / scrypt, compatibile con le identità Python v1.");
        ui.label("Percorso vault .agevault");
        ui.text_edit_singleline(&mut self.fields.vault_path);
        ui.label("Password");
        ui.add(egui::TextEdit::singleline(&mut self.fields.password).password(true));
        ui.horizontal(|ui| {
            if ui.button("Sblocca").clicked() {
                self.vault_action("verify");
            }
            if ui.button("Crea nuovo vault").clicked() {
                self.vault_action("create");
            }
        });
        ui.label("Percorso nuovo vault o copia di recupero");
        ui.text_edit_singleline(&mut self.fields.recovery_path);
        ui.label("Seconda password (backup oppure nuova)");
        ui.add(egui::TextEdit::singleline(&mut self.fields.password_confirm).password(true));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Esporta recupero").clicked() { self.vault_action("export"); }
            if ui.button("Importa recupero").clicked() { self.vault_action("import"); }
            if ui.button("Cambia password").clicked() { self.vault_action("change"); }
        });
        ui.label(RichText::new(if self.vault_verified {
            "Vault autenticato" } else { "Vault non sbloccato" }).color(MUTED));
        ui.label(RichText::new(&self.status).color(INK));
    }
    fn vault_action(&mut self, action: &str) {
        let original = Path::new(&self.fields.vault_path);
        let extra = Path::new(&self.fields.recovery_path);
        let first = &self.fields.password;
        let second = &self.fields.password_confirm;
        let outcome = match action {
            "verify" => vault::verify(original, first),
            "create" => vault::create(original, first),
            "export" => vault::export_recovery(original, extra, first, second),
            "import" => vault::import_recovery(original, extra, first, second),
            "change" => vault::change_password(original, first, second),
            _ => return,
        };
        match outcome {
            Ok(public) => {
                self.vault_verified = true;
                self.status = format!("Operazione {action} riuscita per {public}");
                self.fields.recipient = public;
            }
            Err(error) => {
                self.vault_verified = false;
                self.status = format!("Operazione rifiutata: {error}");
            }
        }
        self.fields.password.zeroize();
        self.fields.password_confirm.zeroize();
    }
    fn updates(&self, ui: &mut egui::Ui) {
        ui.heading("Aggiornamenti");
        ui.label("Canali disponibili: early-release e stable.");
        ui.label(format!("Build nativa: {VERSION}"));
        ui.hyperlink_to("Apri le release ufficiali verificate", 
            format!("{REPO}/releases"));
        ui.label(RichText::new(
            "Gli aggiornamenti non vengono applicati automaticamente senza verifica della firma."
        ).color(MUTED));
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Impostazioni");
        ui.label("Directory privata per gli archivi");
        ui.text_edit_singleline(&mut self.fields.output);
        ui.label("Destinatario pubblico age");
        ui.text_edit_singleline(&mut self.fields.recipient);
        ui.label("Numero di CI GitHub conservate per repository");
        ui.text_edit_singleline(&mut self.fields.max_runs);
        ui.label("Cartella Dropbox di origine");
        ui.text_edit_singleline(&mut self.fields.dropbox_path);
        ui.label("Le password private non vengono mai salvate nel file di configurazione.");
        if ui.button("Aggiorna archivi").clicked() {
            self.rescan();
            self.status = "Inventario aggiornato".into();
        }
    }
    fn about(&mut self, ctx: &Context) {
        egui::Window::new("About Multi Backup")
            .collapsible(false).resizable(true)
            .open(&mut self.about).default_width(430.)
            .show(ctx, |ui| {
                if let Some(logo) = &self.logo {
                    ui.image((logo.id(), Vec2::new(72.,72.)));
                }
                ui.heading(RichText::new("Multi Backup").size(26.).color(INK));
                ui.label("Native Rust desktop");
                ui.separator();
                ui.label(format!("Versione: {VERSION}"));
                ui.label(format!("Autore: {AUTHOR}"));
                ui.label("Licenza: EUPL-1.2");
                ui.label("Sicurezza: age / scrypt / AES-256-GCM");
                ui.hyperlink_to("Repository del progetto", REPO);
                ui.label(RichText::new(
                    "Anteprima nativa: compatibilità con gli archivi r018, \
                     provider e restore ancora in qualificazione."
                ).color(MUTED));
            });
    }
}
impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        if let Some(frames) = &mut self.smoke_frames {
            *frames += 1;
            if *frames >= 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(90));
        }
        self.poll();
        if self.active { ctx.request_repaint_after(Duration::from_millis(100)); }
        egui::TopBottomPanel::top("app-toolbar").frame(egui::Frame::none().fill(NAVY))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("☰").clicked() {self.sidebar = !self.sidebar;}
                    ui.label(RichText::new("Multi Backup").color(INK).size(22.).strong());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("ⓘ About").clicked() { self.about = true; }
                        ui.label(RichText::new("SPAZIO BACKUP PROTETTO").color(MUTED));
                    });
                });
            });
        if self.sidebar {
            egui::SidePanel::left("sidebar").resizable(false)
                .default_width(242.).min_width(242.)
                .frame(egui::Frame::none().fill(Color32::from_rgb(8,29,67)))
                .show(ctx, |ui| {
                    ui.add_space(12.0);
                    if let Some(logo) = &self.logo {
                        ui.vertical_centered(|ui| {
                            ui.image((logo.id(), Vec2::new(88.,88.)));
                            ui.label(RichText::new("Multi Backup").size(23.)
                                .strong().color(INK));
                            ui.label(RichText::new("Backup sicuri e affidabili").color(MUTED));
                        });
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new("IL TUO SPAZIO").color(MUTED).strong());
                    for page in Page::ALL {
                        if page == Page::Vault {
                            ui.separator();
                            ui.label(RichText::new("PREFERENZE").color(MUTED).strong());
                        }
                        let selected = self.page == page;
                        if ui.add(egui::Button::new(RichText::new(page.label()).size(16.)
                                .color(INK))
                                .fill(if selected {VIOLET} else {PANEL})
                                .min_size(Vec2::new(212., 35.))).clicked() {
                            self.page = page;
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Licenza: EUPL-1.2").small().color(MUTED));
                    ui.label(RichText::new(format!("Autore: {AUTHOR}")).small().color(MUTED));
                    ui.label(RichText::new(VERSION).small().color(MUTED));
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::none().fill(NAVY))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(420.0);
                    ui.add_space(18.);
                    match self.page {
                        Page::Dashboard => self.dashboard(ui),
                        Page::Backup => self.backup(ui),
                        Page::Archives => self.archives(ui),
                        Page::Restore => self.restore(ui),
                        Page::Plugins => self.plugins(ui),
                        Page::Vault => self.vault(ui),
                        Page::Updates => self.updates(ui),
                        Page::Settings => self.settings(ui),
                    }
                });
            });
        if self.about { self.about(ctx); }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("{VERSION}"); return Ok(());
    }
    if args.len() == 1 && args[0] == "--self-test" {
        assert!(image::load_from_memory(include_bytes!("../../assets/hero.jpg")).is_ok());
        assert!(image::load_from_memory(include_bytes!("../../assets/icon-256.png")).is_ok());
        assert_eq!(progress_rgb(0.), (232,32,32));
        assert_eq!(progress_rgb(100.), (32,232,32));
        println!("Native Rust GUI self-test OK"); return Ok(());
    }
    let smoke = args.len() == 1 && args[0] == "--smoke";
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Multi Backup · {VERSION}"))
            .with_inner_size([1320.0, 890.0])
            .with_min_inner_size([860.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native("Multi Backup", options,
        Box::new(move |cc| Ok(Box::new(NativeApp::new(&cc.egui_ctx, smoke)))))
}
