//! Standalone Rust GUI preview. This is NOT a Python/Tk wrapper.
//! Backend functions are the same compiled Rust operations as the native CLI.
use eframe::egui::{self, Align, Color32, Context, RichText, TextureHandle, Vec2};
use multi_backup_core::{archive, archive_management, dropbox_client, github_client, progress_rgb, remote_restore, updater, vault};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc::{self, Receiver}, Arc},
    process::{Command, Stdio},
    io::Write,
    thread,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

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
    UpdateInfo(updater::ReleaseInfo),
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
    remote_repo: String,
    remote_file: String,
    delete_confirm: String,
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
/// Match the legacy Python settings_path()/secure_vault.vault_path()
/// on each supported platform. A vault is not stored in the backup output.
fn default_vault_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    #[cfg(target_os = "macos")]
    let base = Path::new(&home).join("Library/Application Support");
    #[cfg(target_os = "windows")]
    let base = PathBuf::from(std::env::var("APPDATA")
        .unwrap_or_else(|_|Path::new(&home).join("AppData/Roaming")
            .display().to_string()));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = PathBuf::from(std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_|Path::new(&home).join(".config")
            .display().to_string()));
    base.join("multi-backup/keys/default.agevault")
}
/// Read-only migration of the old app's non-secret user preferences.
/// No settings file is modified by merely opening the Rust preview.
fn read_legacy_settings() -> Result<serde_json::Value, String> {
    let path = default_vault_path().parent()
        .and_then(|dir| dir.parent())
        .ok_or("Invalid protected settings location")?
        .join("settings.json");
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound =>
            return Ok(serde_json::Value::Null),
        Err(error) => return Err(format!("Cannot read legacy settings: {error}")),
    };
    if !meta.file_type().is_file() || meta.len() > 128 * 1024 {
        return Err("Unsafe or oversized legacy settings file".into());
    }
    let input = std::fs::File::open(path).map_err(|e|e.to_string())?;
    let data: serde_json::Value =
        serde_json::from_reader(input).map_err(|e|e.to_string())?;
    if !data.is_object() { return Err("Invalid legacy settings JSON object".into()); }
    Ok(data)
}
fn imported_provider(preferences: &serde_json::Value) -> String {
    if let Some(list) = preferences["enabled_backends"].as_array() {
        for item in list {
            if item == "github" { return "github".into(); }
            if item == "dropbox" { return "dropbox".into(); }
        }
    }
    "github".into()
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
    must_unlock_on_start: bool,
    session_identity: Option<Arc<Zeroizing<Vec<u8>>>>,
    smoke_frames: Option<usize>,
    remote_confirm: Option<String>,
    update_channel: String,
    update_info: Option<updater::ReleaseInfo>,
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
        let vault_path = default_vault_path();
        let must_unlock_on_start = std::fs::symlink_metadata(&vault_path).is_ok();
        let migrated = read_legacy_settings();
        let empty_prefs = serde_json::Value::Null;
        let prefs = migrated.as_ref().unwrap_or(&empty_prefs);
        let output = prefs["output_directory"].as_str()
            .filter(|s| !s.is_empty() && s.len() < 4096)
            .unwrap_or_else(|| "").to_owned();
        let default_output_path = if output.is_empty() { default_output() } else { output };
        let age_recipient = prefs["default_recipient"].as_str()
            .filter(|v|v.starts_with("age1") && v.len() >= 24 && v.len() < 256)
            .unwrap_or("").to_owned();
        let max_runs = prefs["backup_ci_runs"].as_u64()
            .filter(|v| (1..=1000).contains(v)).unwrap_or(5);
        let update_channel = match prefs["update_channel"].as_str() {
            Some("stable") => "stable", Some("early-release") => "early-release",
            _ => "stable",
        };
        let provider = imported_provider(prefs);
        let fields = Fields {
            output: default_output_path,
            recipient: age_recipient,
            vault_path: vault_path.display().to_string(),
            max_runs: max_runs.to_string(),
            ..Fields::default()
        };
        let entries = archive_catalog(Path::new(&fields.output));
        Self {
            fields, page: Page::Dashboard, sidebar: true, about: false,
            provider,
            status: match migrated {
                Ok(serde_json::Value::Null) => "Nessun backup attivo".into(),
                Ok(_) => "Impostazioni r018 importate in sola lettura".into(),
                Err(error) => format!("Impostazioni legacy non importate: {error}"),
            },
            active: false, started: Instant::now(),
            last_activity: Instant::now(), progress: None, receiver: None,
            hero: None,
            logo: load_photo(ctx, "multi-backup-logo", include_bytes!("../../assets/icon-256.png")),
            count: entries.len(), entries, vault_verified: false,
            must_unlock_on_start,
            session_identity: None,
            smoke_frames: if smoke { Some(0) } else { None },
            remote_confirm: None,
            update_channel: update_channel.into(),
            update_info: None,
        }
    }
    fn rescan(&mut self) {
        self.entries = archive_catalog(Path::new(&self.fields.output));
        self.count = self.entries.len();
    }
    fn run_backup(&mut self, resume: bool) {
        if self.active { return; }
        if !self.vault_verified || self.session_identity.is_none() {
            self.page = Page::Vault;
            self.status = "Crea o sblocca il vault cifrato prima di avviare un backup".into();
            return;
        }
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
                github_client::backup_with_progress(
                    &output, &recipient, resume, max_runs,
                    |done, total, message| {
                        let fraction = if total == 0 {0.0}
                            else {(done as f32 / total as f32) * 0.96};
                        let _ = tx.send(Event::Progress(fraction, message.into()));
                    }).map_err(|e|e.to_string())
            } else {
                dropbox_client::backup_with_progress(
                    &output, &remote, &recipient, resume,
                    |done, total, message| {
                        let fraction = if total == 0 {0.0}
                            else {(done as f32 / total as f32) * 0.96};
                        let _ = tx.send(Event::Progress(fraction, message.into()));
                    }).map_err(|e|e.to_string())
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
    fn run_remote_restore(&mut self, provider: String) {
        if self.active { return; }
        let source = self.fields.selected_archive.clone();
        let identity = self.fields.identity.clone();
        let secret = self.session_identity.clone();
        if source.trim().is_empty() || (identity.trim().is_empty() && secret.is_none()) {
            self.status = "Per il ripristino remoto seleziona l'archivio e sblocca il vault".into();
            return;
        }
        let repo = self.fields.remote_repo.clone();
        let path = self.fields.remote_file.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.active = true;
        self.progress = None;
        self.started = Instant::now();
        self.last_activity = Instant::now();
        thread::spawn(move || {
            let _ = tx.send(Event::Status(format!("Ripristino remoto {} richiesto…", provider)));
            let done = if provider == "github" {
                (if let Some(secret) = &secret {
                    remote_restore::github_new_repository_from_memory(
                        Path::new(&source), secret, &repo, true)
                } else {
                    remote_restore::github_new_repository(Path::new(&source),
                        Path::new(&identity), &repo, true)
                }).map_err(|e|e.to_string())
            } else {
                (if let Some(secret) = &secret {
                    remote_restore::dropbox_new_file_from_memory(
                        Path::new(&source), secret, &path)
                } else {
                    remote_restore::dropbox_new_file(Path::new(&source),
                        Path::new(&identity), &path)
                }).map(|r|r["path_display"].as_str().unwrap_or(&path).to_owned())
                    .map_err(|e|e.to_string())
            };
            match done {
                Ok(s) => { let _ = tx.send(Event::Success(format!("Ripristino remoto: {s}"))); },
                Err(e) => { let _ = tx.send(Event::Failure(e)); },
            }
        });
    }
    fn run_restore(&mut self, verify_only: bool) {
        if self.active { return; }
        let path = self.fields.selected_archive.clone();
        let identity = self.fields.identity.clone();
        let secret = self.session_identity.clone();
        if path.trim().is_empty() || (identity.trim().is_empty() && secret.is_none()) {
            self.status = "Seleziona un archivio e sblocca il vault oppure scegli la chiave age".into();
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
                (if let Some(secret) = &secret {
                    archive::verify_from_memory(Path::new(&path), secret)
                } else {
                    archive::verify(Path::new(&path), Path::new(&identity))
                }).map(|size| format!("Archivio verificato: {size} elementi"))
            } else {
                (if let Some(secret) = &secret {
                    archive::extract_from_memory(Path::new(&path), secret, Path::new(&dest))
                } else {
                    archive::extract(Path::new(&path), Path::new(&identity), Path::new(&dest))
                }).map(|members| format!(
                    "Ripristinati {} elementi in {}", members.len(), dest
                ))
            };
            match outcome {
                Ok(message) => {let _ = tx.send(Event::Success(message));}
                Err(error) => {let _ = tx.send(Event::Failure(error.to_string()));}
            }
        });
    }
    fn check_native_updates(&mut self) {
        if self.active { return; }
        let channel = self.update_channel.clone();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.active = true;
        self.progress = None;
        self.last_activity = Instant::now();
        thread::spawn(move || {
            let _ = tx.send(Event::Status("Controllo release Rust autenticata…".into()));
            match updater::get_release(&channel) {
                Ok(info) => { let _ = tx.send(Event::UpdateInfo(info)); },
                Err(error) => { let _ = tx.send(Event::Failure(
                    format!("Aggiornamento non disponibile: {error}"))); }
            }
        });
    }
    fn stage_native_update(&mut self) {
        if self.active { return; }
        let Some(info) = self.update_info.clone() else { return; };
        let folder = Path::new(&self.fields.output).join(".updates");
        let (tx,rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.active = true;
        self.progress = None;
        self.last_activity = Instant::now();
        thread::spawn(move || {
            let _ = tx.send(Event::Status("Download HTTPS e verifica SHA-256…".into()));
            match updater::stage(&info, &folder) {
                Ok(path) => { let _ = tx.send(Event::Success(format!(
                    "Pacchetto verificato e salvato (non installato): {}",
                    path.display()))); },
                Err(error) => { let _ = tx.send(Event::Failure(
                    format!("Verifica aggiornamento fallita: {error}"))); }
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
                    Event::UpdateInfo(info) => {
                        self.status = if let Some(asset) = &info.asset {
                            format!("Disponibile asset nativo verificabile: {}", asset.name)
                        } else {
                            "Nessun installer Rust disponibile per questa piattaforma/canale".into()
                        };
                        self.update_info = Some(info);
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
    /// Original vector hero, independent of stock imagery, URLs and watermarks.
    fn draw_original_hero(ui: &mut egui::Ui) {
        let width = ui.available_width();
        let height = (width / 3.4).clamp(130.0, 270.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
        let pen = ui.painter_at(rect);
        pen.rect_filled(rect, 17.0, Color32::from_rgb(16, 32, 76));
        let step = (width / 16.0).max(8.0);
        for i in 0..5 {
            let x = rect.left() + i as f32 * step;
            let right = rect.right() - i as f32 * step;
            for dx in [x, right - step * 0.75] {
                let rack = egui::Rect::from_min_size(
                    egui::pos2(dx, rect.top() + 6.0), Vec2::new(step * 0.74, height - 12.0));
                pen.rect_filled(rack, 5.0, Color32::from_rgb(12, 26, 60));
                pen.rect_stroke(rack, 5.0, egui::Stroke::new(1.0, Color32::from_rgb(57, 96, 168)));
                for j in 0..9 {
                    let led = egui::pos2(rack.left() + 8.0,
                        rack.top() + 10.0 + j as f32 * (height - 30.0) / 10.0);
                    pen.circle_filled(led, 1.7, if j%3==0 {VIOLET}
                        else {Color32::from_rgb(66, 205, 241)});
                }
            }
        }
        let card = rect.shrink2(Vec2::new(width * 0.16, height * 0.23));
        pen.rect_filled(card, 18.0, Color32::from_rgb(23, 38, 97));
        pen.rect_stroke(card, 18.0, egui::Stroke::new(2.0, Color32::from_rgb(85, 205, 247)));
        pen.text(card.center() - Vec2::new(0.0, height * 0.06),
            egui::Align2::CENTER_CENTER, "Multi Backup",
            egui::FontId::proportional((width * 0.044).clamp(18.0, 39.0)),INK);
        let bar = egui::Rect::from_min_size(
            egui::pos2(card.left()+card.width()*0.18,card.bottom()-height*0.15),
            Vec2::new(card.width()*0.64,8.0));
        pen.rect_filled(bar, 4.0, Color32::from_rgb(31, 52, 110));
        pen.rect_filled(egui::Rect::from_min_size(bar.min,
           Vec2::new(bar.width()*0.67,bar.height())),4.0,Color32::from_rgb(58, 207, 231));
    }

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        if let Some(hero) = &self.hero {
            let size = hero.size_vec2();
            let width = ui.available_width();
            let height = (width * size.y / size.x).min(290.0);
            ui.image((hero.id(), Vec2::new((height * size.x/size.y).min(width), height)));
        } else { Self::draw_original_hero(ui); }
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
                    ui.label(format!("{:0.1} MiB", bytes as f64 / (1024.0*1024.0)));
                });
            }
        });
        if ui.button("Verifica / Ripristina archivio selezionato").clicked() {
            self.page = Page::Restore;
        }
        ui.separator();
        ui.label("Per cancellare un archivio locale digita il filename esatto:");
        ui.text_edit_singleline(&mut self.fields.delete_confirm);
        if ui.add_enabled(!self.active, egui::Button::new("Cancella backup selezionato"))
            .clicked() {
            let target = PathBuf::from(&self.fields.selected_archive);
            match archive_management::delete(
                Path::new(&self.fields.output), &target,
                &self.fields.delete_confirm, self.active) {
                Ok(removed) => {
                    self.status = format!("Eliminati {} file locali; audit registrato", removed.len());
                    self.fields.delete_confirm.clear();
                    self.fields.selected_archive.clear();
                    self.rescan();
                }
                Err(error) => self.status = format!("Cancellazione rifiutata: {error}"),
            }
        }
        ui.label(RichText::new(&self.status).color(INK));
    }
    fn restore(&mut self, ui: &mut egui::Ui) {
        ui.heading("Ripristino cifrato");
        ui.label("Il motore Rust richiede una directory NUOVA e autentica tutto lo stream age.");
        ui.label("Archivio .tar.gz.age");
        ui.text_edit_singleline(&mut self.fields.selected_archive);
        ui.label("Il vault sbloccato è usato in memoria. Chiave age esterna opzionale:");
        ui.text_edit_singleline(&mut self.fields.identity);
        ui.label("Nuova directory di destinazione (non può esistere)");
        ui.text_edit_singleline(&mut self.fields.destination);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.active, egui::Button::new("Verifica integrità"))
                .clicked() { self.run_restore(true); }
            if ui.add_enabled(!self.active, egui::Button::new("Ripristina")
                .fill(VIOLET)).clicked() { self.run_restore(false); }
        });
        ui.separator();
        ui.label("Creazione REMOTA esplicita (non sovrascrive repository o file esistenti)");
        ui.label("Nuovo repository GitHub owner/name (privato)");
        ui.text_edit_singleline(&mut self.fields.remote_repo);
        if ui.add_enabled(!self.active, egui::Button::new("Crea repository da archivio"))
            .clicked() { self.remote_confirm = Some("github".into()); }
        ui.label("Nuovo file Dropbox /cartella/file.ext");
        ui.text_edit_singleline(&mut self.fields.remote_file);
        if ui.add_enabled(!self.active, egui::Button::new("Carica nuovo file su Dropbox"))
            .clicked() { self.remote_confirm = Some("dropbox".into()); }
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
        if ui.text_edit_singleline(&mut self.fields.vault_path).changed() {
            self.session_identity = None;
            self.vault_verified = false;
        }
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
        #[cfg(target_os = "macos")]
        {
            ui.separator();
            ui.label("Touch ID — disponibile sui Mac con impronta registrata");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Sblocca con Touch ID").clicked() {
                    self.macos_biometric("unlock");
                }
                if ui.add_enabled(self.vault_verified,
                    egui::Button::new("Abilita Touch ID (reinserisci password)")).clicked() {
                    self.macos_biometric("enroll");
                }
                if ui.button("Disabilita Touch ID").clicked() {
                    self.macos_biometric("disable");
                }
            });
        }
        ui.label(RichText::new(if self.vault_verified {
            "Vault sbloccato in RAM fino alla chiusura dell'app" }
            else { "Vault non sbloccato" }).color(MUTED));
        ui.label(RichText::new(&self.status).color(INK));
    }
    /// Existing native Swift LocalAuthentication + device-bound Keychain
    /// helper. Only the vault password crosses a local pipe, never argv/logs.
    /// Touch ID cannot bypass the Rust vault's AES-GCM authentication.
    #[cfg(target_os = "macos")]
    fn macos_biometric(&mut self, action: &str) {
        let response = (|| -> Result<Option<Zeroizing<String>>, String> {
            let exe = std::env::current_exe().map_err(|e|e.to_string())?;
            let bin = exe.parent().and_then(|p|p.parent())
                .ok_or_else(||"Native Mac app path not found".to_string())?
                .join("Resources/bin/multi-backup-biometric");
            if !std::fs::symlink_metadata(&bin)
                .is_ok_and(|m|m.file_type().is_file()) {
                return Err("Verified native Touch ID helper is missing".into());
            }
            match action {
                "enroll" => {
                    if !self.vault_verified {
                        return Err("First unlock the protected vault with its password".into());
                    }
                    let password = &self.fields.password;
                    vault::verify(Path::new(&self.fields.vault_path), password)
                        .map_err(|_|"Incorrect vault password for enrollment".to_owned())?;
                    let mut child = Command::new(&bin).arg("enroll")
                        .stdin(Stdio::piped()).stdout(Stdio::null())
                        .stderr(Stdio::null()).spawn().map_err(|e|e.to_string())?;
                    {
                        let mut stdin = child.stdin.take().ok_or("No biometric input pipe")?;
                        stdin.write_all(password.as_bytes()).map_err(|e|e.to_string())?;
                    }
                    if !child.wait().map_err(|e|e.to_string())?.success() {
                        return Err("Touch ID enrollment refused (may already be enabled)".into());
                    }
                    Ok(None)
                }
                "unlock" => {
                    let result = Command::new(&bin).arg("unlock")
                        .stderr(Stdio::null()).output().map_err(|e|e.to_string())?;
                    if !result.status.success() {
                        return Err("Touch ID unavailable, not enrolled or denied".into());
                    }
                    let secret = Zeroizing::new(result.stdout);
                    if secret.len() < 12 || secret.len() > 1024 {
                        return Err("Biometric credential size rejected".into());
                    }
                    let password = Zeroizing::new(
                        String::from_utf8(secret.to_vec()).map_err(|e|e.to_string())?);
                    Ok(Some(password))
                }
                "disable" => {
                    let status = Command::new(&bin).arg("disable")
                        .stdout(Stdio::null()).stderr(Stdio::null())
                        .status().map_err(|e|e.to_string())?;
                    if !status.success() {
                        return Err("Could not disable Touch ID credential".into());
                    }
                    Ok(None)
                }
                _ => Err("Unsupported biometric action".into()),
            }
        })();
        match response {
            Ok(password) => {
                if let Some(password) = password {
                    match vault::unlock_identity(
                        Path::new(&self.fields.vault_path), &password) {
                        Ok(secret) => {
                            self.session_identity = Some(Arc::new(secret));
                            self.vault_verified = true;
                            if let Ok(recipient) = vault::verify(
                                Path::new(&self.fields.vault_path), &password) {
                                self.fields.recipient = recipient;
                            }
                            self.status = "Vault sbloccato con Touch ID per questa sessione".into();
                        }
                        Err(_) => {
                            self.status = "Touch ID non ha sbloccato il vault selezionato".into();
                        }
                    }
                } else {
                    self.status = format!("Touch ID: {action} eseguito");
                }
            }
            Err(error) => self.status = error,
        }
        self.fields.password.zeroize();
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
                if action == "verify" || action == "create" {
                    match vault::unlock_identity(original, first) {
                        Ok(secret) => {
                            self.session_identity = Some(Arc::new(secret));
                            self.vault_verified = true;
                        },
                        Err(error) => {
                            self.session_identity = None;
                            self.vault_verified = false;
                            self.status = format!("Sblocco chiave non riuscito: {error}");
                            self.fields.password.zeroize();
                            self.fields.password_confirm.zeroize();
                            return;
                        }
                    }
                }
                self.status = format!("Operazione {action} riuscita per {public}");
                self.fields.recipient = public;
            }
            Err(error) => {
                self.status = format!("Operazione rifiutata: {error}");
            }
        }
        self.fields.password.zeroize();
        self.fields.password_confirm.zeroize();
    }
    fn updates(&mut self, ui: &mut egui::Ui) {
        ui.heading("Aggiornamenti nativi Rust");
        ui.label(format!("Build installata: {VERSION}"));
        ui.label("Origine GitHub ufficiale con SHA-256 verificato.");
        ui.horizontal(|ui| {
            if ui.selectable_value(&mut self.update_channel,
                "early-release".to_string(), "Early release").changed() {
                self.update_info = None;
            }
            if ui.selectable_value(&mut self.update_channel,
                "stable".to_string(), "Stable").changed() {
                self.update_info = None;
            }
        });
        if ui.add_enabled(!self.active, egui::Button::new("Controlla aggiornamenti"))
            .clicked() { self.check_native_updates(); }
        if let Some(info) = &self.update_info {
            ui.label(format!("Release: {} — {}", info.channel, info.tag));
            if let Some(asset) = &info.asset {
                ui.label(format!("Asset Rust: {} ({} bytes)", asset.name, asset.bytes));
                if ui.add_enabled(!self.active,
                    egui::Button::new("Scarica e verifica SHA-256 (NON installa)"))
                    .clicked() { self.stage_native_update(); }
            } else {
                ui.label("Nessun pacchetto Rust con SHA-256 disponibile");
            }
        }
        ui.hyperlink_to("Release ufficiali", format!("{REPO}/releases"));
        ui.label(RichText::new(
            "Nessuna installazione automatica: verificare sempre la firma del sistema operativo."
        ).color(MUTED));
        ui.label(RichText::new(&self.status).color(INK));
        self.draw_progress(ui);
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
    fn unlock_screen(&mut self, ctx: &Context) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(NAVY))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(55.0);
                    if let Some(logo) = &self.logo {
                        ui.image((logo.id(), Vec2::new(100.0,100.0)));
                    }
                    ui.heading(RichText::new("Multi Backup — Vault protetto")
                        .size(27.0).color(INK));
                    ui.label("Per proteggere i backup esistenti, sblocca la chiave prima di usare l'app.");
                    ui.add_space(14.0);
                    ui.label("Vault cifrato esistente (percorso legacy compatibile):");
                    ui.add(egui::TextEdit::singleline(&mut self.fields.vault_path)
                        .desired_width(460.0));
                    ui.add_space(6.0);
                    ui.label("Password del vault");
                    let response = ui.add(egui::TextEdit::singleline(&mut self.fields.password)
                        .password(true).desired_width(280.0));
                    let proceed = ui.add(egui::Button::new("Sblocca vault")
                        .fill(VIOLET)).clicked()
                        || (response.lost_focus() && ui.input(|input|
                            input.key_pressed(egui::Key::Enter)));
                    if proceed {
                        match vault::unlock_identity(
                            Path::new(&self.fields.vault_path),
                            &self.fields.password) {
                            Ok(secret) => {
                                self.session_identity = Some(Arc::new(secret));
                                self.vault_verified = true;
                                if let Ok(public) = vault::verify(
                                    Path::new(&self.fields.vault_path), &self.fields.password) {
                                    self.fields.recipient = public;
                                }
                                self.status = "Vault autenticato per questa sessione".into();
                            },
                            Err(_) => {
                                self.status = "Password errata o vault non valido; accesso negato".into();
                            }
                        }
                        self.fields.password.zeroize();
                    }
                    #[cfg(target_os = "macos")]
                    if ui.button("Sblocca con Touch ID, se già configurato").clicked() {
                        self.macos_biometric("unlock");
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new(&self.status).color(MUTED));
                    ui.label(RichText::new(
                        "La chiave privata non viene mai salvata in un file in chiaro.")
                        .color(MUTED).small());
                });
            });
    }

    fn about(&mut self, ctx: &Context) {
        let mut open = self.about;
        egui::Window::new("About Multi Backup")
            .collapsible(false).resizable(true)
            .open(&mut open).default_width(430.)
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
        self.about = open;
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
        if self.must_unlock_on_start && !self.vault_verified {
            self.unlock_screen(ctx);
            if self.about { self.about(ctx); }
            return;
        }
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
        if let Some(provider) = self.remote_confirm.clone() {
            let mut open = true;
            egui::Window::new("Conferma ripristino remoto")
                .collapsible(false).resizable(false)
                .open(&mut open).show(ctx, |ui| {
                    ui.label(RichText::new(
                        "Verrà creato un NUOVO repository/file remoto.                          L'operazione non sovrascrive dati esistenti."
                    ).color(INK));
                    ui.horizontal(|ui| {
                        if ui.button("Annulla").clicked() { self.remote_confirm = None; }
                        if ui.button("Conferma CREATE-NEW").clicked() {
                            self.remote_confirm = None;
                            self.run_remote_restore(provider.clone());
                        }
                    });
                });
            if !open { self.remote_confirm = None; }
        }
    }
}

/// Bundled native helpers must be discoverable even when the app is launched
/// from Finder or Windows Explorer, where the usual shell PATH is absent.
/// Only our installed application directories are prepended.
fn configure_bundled_helpers() {
    let Ok(executable) = std::env::current_exe() else { return };
    let Some(parent) = executable.parent() else { return };
    #[cfg(target_os = "macos")]
    let candidate = parent.parent().map(|contents| contents.join("Resources/bin"));
    #[cfg(target_os = "windows")]
    let candidate = Some(parent.join("bin"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let candidate = Some(parent.join("bin"));
    let Some(candidate) = candidate else { return };
    if !candidate.is_dir() { return; }
    // A signed application owns this folder; reject a symlink placed over it.
    if std::fs::symlink_metadata(&candidate)
        .is_ok_and(|meta| meta.file_type().is_symlink()) { return; }
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![candidate];
    paths.extend(std::env::split_paths(&current));
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }
}

fn main() -> eframe::Result {
    configure_bundled_helpers();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("{VERSION}"); return Ok(());
    }
    if args.len() == 1 && args[0] == "--self-test" {
        // Artwork is built from in-process vector geometry; no external photo asset.
        assert!(std::mem::size_of::<NativeApp>() > 0);
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
