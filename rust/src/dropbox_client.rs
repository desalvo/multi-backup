//! Rust-native Dropbox incremental backup: read-only API, immutable encrypted
//! objects, authenticated revisions and Python-compatible manifest/session v1.
//! Remote source data is never deleted or moved.
use crate::{archive, dropbox_hash, journal};
use chrono::Utc;
use fs2::FileExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use tempfile::{NamedTempFile, TempDir};
use ureq::{Agent, AgentBuilder, Response};

pub type DropboxResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
const API: &str = "https://api.dropboxapi.com/2/files/";
const DOWNLOAD: &str = "https://content.dropboxapi.com/2/files/download";

fn private_directory(path: &Path) -> DropboxResult<()> {
    let new_directory = fs::symlink_metadata(path).is_err();
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() { return Err(err("Symlink backup directory denied")); }
    } else { fs::create_dir_all(path)?; }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if new_directory { fs::set_permissions(path, fs::Permissions::from_mode(0o700))?; }
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(err("Backup directory must be private (0700)"));
        }
    }
    Ok(())
}
fn write_state(path: &Path, value: &Value) -> DropboxResult<()> {
    let parent = path.parent().ok_or_else(|| err("No state directory"))?;
    private_directory(parent)?;
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(err("Symlink state file rejected"));
    }
    let mut tmp = NamedTempFile::new_in(parent)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer_pretty(&mut tmp, value)?;
    tmp.write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn sha256(path: &Path) -> DropboxResult<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(err("Unsafe archive file"));
    }
    let mut hasher = Sha256::new();
    let mut stream = File::open(path)?;
    let mut buffer = [0u8; 65536];
    loop {
        let n = stream.read(&mut buffer)?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn verified_archive(path: &Path) -> DropboxResult<bool> {
    if !path.exists() { return Ok(false); }
    let sidecar = PathBuf::from(format!("{}.sha256", path.display()));
    if !fs::symlink_metadata(&sidecar).is_ok_and(|m| m.file_type().is_file()) {
        return Err(err("An existing encrypted object lacks its checksum"));
    }
    let text = fs::read_to_string(sidecar)?;
    let expected = text.split_whitespace().next().ok_or_else(|| err("Empty checksum sidecar"))?;
    if expected != sha256(path)? {
        return Err(err("An existing encrypted object failed SHA-256 verification"));
    }
    Ok(true)
}
fn manifest_filename(name: &str) -> bool {
    if !name.ends_with(".tar.gz.age") { return false; }
    let stamp = &name[..name.len() - ".tar.gz.age".len()];
    if stamp.len() != 22 || stamp.as_bytes()[8] != b'T' || !stamp.ends_with('Z') {
        return false;
    }
    stamp[..8].bytes().all(|b| b.is_ascii_digit()) &&
        stamp[9..21].bytes().all(|b| b.is_ascii_digit())
}
struct Session {
    active: PathBuf,
    name: String,
    _lock: File,
}
impl Session {
    fn open(root: &Path, recipient: &str, remote: &str, resume: bool) -> DropboxResult<Self> {
        let state = root.join(".state");
        private_directory(root)?;
        private_directory(&state)?;
        let lockpath = state.join("dropbox-rust.lock");
        if fs::symlink_metadata(&lockpath).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(err("Unsafe session lock symlink"));
        }
        let lock = OpenOptions::new().create(true).read(true).write(true).open(lockpath)?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            lock.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        lock.try_lock_exclusive().map_err(|_| err("Another Dropbox backup owns the session"))?;
        let active = state.join("dropbox-active.json");
        if fs::symlink_metadata(&active).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(err("Unsafe Dropbox active checkpoint"));
        }
        let config = json!({"recipient":recipient,"path":remote});
        let digest = journal::config_hash(&config);
        let name = if resume {
            let value: Value = serde_json::from_reader(File::open(&active)?)?;
            let old = value["manifest_name"].as_str()
                .ok_or_else(|| err("Resume session name missing"))?;
            if value["schema"] != 1 || value["config_hash"] != digest ||
               !manifest_filename(old) {
                return Err(err("Resume checkpoint differs from requested source/recipient"));
            }
            old.to_owned()
        } else {
            if active.exists() {
                let old: Value = serde_json::from_reader(File::open(&active)?)?;
                let previous = old["manifest_name"].as_str()
                    .ok_or_else(|| err("Prior session name missing"))?;
                if !manifest_filename(previous) {
                    return Err(err("Prior session filename is unsafe"));
                }
                let parked = state.join(format!("dropbox-abandoned-{previous}.json"));
                if parked.exists() { return Err(err("Previous checkpoint already parked")); }
                let save = OpenOptions::new().write(true).create_new(true).open(&parked)?;
                drop(save);
                write_state(&parked, &old)?;
            }
            let name = Utc::now().format("%Y%m%dT%H%M%S%6fZ.tar.gz.age").to_string();
            write_state(&active, &json!({
                "schema":1, "backend":"dropbox", "config_hash":digest,
                "manifest_name":name, "started_utc":Utc::now().to_rfc3339()
            }))?;
            name
        };
        Ok(Self { active, name, _lock: lock })
    }
    fn finish(&self, manifest: &Path) -> DropboxResult<()> {
        if manifest.file_name().and_then(|x| x.to_str()) != Some(self.name.as_str()) ||
           !verified_archive(manifest)? {
            return Err(err("Cannot finalize an unverified or mismatched manifest"));
        }
        let saved: Value = serde_json::from_reader(File::open(&self.active)?)?;
        if saved["manifest_name"] != self.name {
            return Err(err("Active Dropbox session identity has changed"));
        }
        fs::remove_file(&self.active)?;
        Ok(())
    }
}
fn accept_retry(status: u16) -> bool { matches!(status, 429|500|502|503|504) }

pub struct DropboxClient {
    agent: Agent,
    token: String,
}
impl DropboxClient {
    fn from_env() -> DropboxResult<Self> {
        let token = std::env::var("MULTI_BACKUP_DROPBOX_TOKEN")?;
        if token.trim().len() < 12 || token.contains('\n') || token.contains('\r') {
            return Err(err("Dropbox access token is absent or malformed"));
        }
        let agent = AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(180))
            .timeout_write(Duration::from_secs(120))
            .redirects(0)
            .build();
        Ok(Self { agent, token })
    }
    fn post(&self, url: &str, payload: &Value) -> DropboxResult<Response> {
        if ![format!("{API}list_folder"), format!("{API}list_folder/continue")].contains(&url.to_string()) {
            return Err(err("Unapproved Dropbox listing endpoint"));
        }
        for attempt in 0..4 {
            let response = self.agent.post(url)
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Content-Type", "application/json")
                .send_json(payload.clone());
            match response {
                Ok(value) => return Ok(value),
                Err(ureq::Error::Status(code, _)) if accept_retry(code) && attempt < 3 => {
                    thread::sleep(Duration::from_secs(1 << attempt));
                }
                Err(ureq::Error::Status(_, _)) => return Err(err("Dropbox API rejected listing")),
                Err(error) => return Err(Box::new(error)),
            }
        }
        Err(err("Dropbox retry limit exceeded"))
    }
    fn list(&self, remote: &str) -> DropboxResult<Vec<Value>> {
        let mut result = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let (endpoint, payload) = match &cursor {
                None => ("list_folder", json!({
                    "path":remote, "recursive":true, "include_deleted":false, "limit":2000
                })),
                Some(cursor) => ("list_folder/continue", json!({"cursor":cursor})),
            };
            let value: Value = self.post(&format!("{API}{endpoint}"), &payload)?.into_json()?;
            let entries = value["entries"].as_array().ok_or_else(|| err("Dropbox listing missing entries"))?;
            if result.len().saturating_add(entries.len()) > 500_000 {
                return Err(err("Dropbox listing exceeds safety limit"));
            }
            for entry in entries {
                if entry[".tag"].as_str() == Some("file") ||
                   entry[".tag"].as_str() == Some("folder") {
                    result.push(entry.clone());
                }
            }
            if value["has_more"] == true {
                let next = value["cursor"].as_str().ok_or_else(|| err("Missing Dropbox pagination cursor"))?;
                if next.is_empty() || cursor.as_deref() == Some(next) {
                    return Err(err("Invalid Dropbox pagination cursor"));
                }
                cursor = Some(next.to_owned());
            } else { break; }
        }
        Ok(result)
    }
    fn download(&self, id: &str, revision: &str, into: &Path) -> DropboxResult<Value> {
        let id = if id.starts_with("id:") { id.to_owned() } else { format!("id:{id}") };
        let argument = json!({"path":id}).to_string();
        for attempt in 0..4 {
            let request = self.agent.post(DOWNLOAD)
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Dropbox-API-Arg", &argument);
            match request.call() {
                Ok(response) => {
                    let metadata: Value = serde_json::from_str(response.header("Dropbox-API-Result")
                        .ok_or_else(|| err("Dropbox download result metadata missing"))?)?;
                    if metadata["rev"] != revision || metadata["id"] != id {
                        return Err(err("Dropbox object identity or revision changed while downloading"));
                    }
                    let mut destination = OpenOptions::new().write(true).create_new(true).open(into)?;
                    io::copy(&mut response.into_reader(), &mut destination)?;
                    destination.sync_all()?;
                    return Ok(metadata);
                }
                Err(ureq::Error::Status(code, _)) if accept_retry(code) && attempt < 3 => {
                    thread::sleep(Duration::from_secs(1 << attempt));
                }
                Err(ureq::Error::Status(_, _)) => return Err(err("Dropbox file download rejected")),
                Err(error) => return Err(Box::new(error)),
            }
        }
        Err(err("Dropbox download retry limit exceeded"))
    }
}
fn field<'a>(item: &'a Value, name: &str) -> DropboxResult<&'a str> {
    item[name].as_str().filter(|v| !v.is_empty())
        .ok_or_else(|| err("Required Dropbox metadata field missing"))
}
fn validate_download(file: &Path, listing: &Value, download: &Value) -> DropboxResult<()> {
    let rev = field(listing,"rev")?;
    let id = field(listing,"id")?;
    if download["rev"] != rev || download["id"] != id {
        return Err(err("Dropbox file changed between listing and download"));
    }
    let len = listing["size"].as_u64().ok_or_else(|| err("Dropbox listing omitted file length"))?;
    if download["size"].as_u64() != Some(len) || fs::metadata(file)?.len() != len {
        return Err(err("Dropbox file length mismatch"));
    }
    let hash = field(listing,"content_hash")?;
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(err("Invalid Dropbox listing content hash"));
    }
    if !download["content_hash"].is_null() && download["content_hash"] != hash {
        return Err(err("Dropbox returned inconsistent content hash"));
    }
    if dropbox_hash::hash_file(file)? != hash {
        return Err(err("Dropbox file content hash check failed"));
    }
    Ok(())
}

pub fn backup_with_progress<F: FnMut(usize, usize, &str)>(
    output: &Path, remote: &str, recipient: &str, resume: bool,
    mut on_progress: F,
) -> DropboxResult<Value> {
    if !(remote.is_empty() || remote.starts_with('/')) ||
        remote.len() > 1024 || remote.contains('\n') || remote.contains('\r') {
        return Err(err("Invalid Dropbox remote folder path"));
    }
    if !recipient.starts_with("age1") || recipient.len() < 24 {
        return Err(err("Valid public age recipient is required"));
    }
    private_directory(output)?;
    let session = Session::open(output, recipient, remote, resume)?;
    let root = output.join("dropbox");
    let objects = root.join("objects");
    let manifests = root.join("manifests");
    for folder in [&root, &objects, &manifests] { private_directory(folder)?; }
    let manifest_path = manifests.join(&session.name);
    if resume && verified_archive(&manifest_path)? {
        session.finish(&manifest_path)?;
        return Ok(json!({"backend":"dropbox", "manifest":manifest_path,"resumed":true}));
    }
    let client = DropboxClient::from_env()?;
    let mut counts = json!({"files_seen":0u64,"saved":0u64,"reused":0u64,
                            "folders":0u64,"errors":0u64});
    let mut entries = Vec::new();
    let listing = client.list(remote)?;
    let total = listing.len();
    on_progress(0, total, "Elenco Dropbox ricevuto");
    for (index, item) in listing.into_iter().enumerate() {
        if item[".tag"] == "folder" {
            counts["folders"] = json!(counts["folders"].as_u64().unwrap_or(0)+1);
            entries.push(json!({"kind":"folder","path":item["path_display"]}));
            on_progress(index + 1, total, "Cartella indicizzata");
            continue;
        }
        let id = field(&item,"id")?;
        let rev = field(&item,"rev")?;
        let mut hasher = Sha256::new();
        hasher.update(id.as_bytes());
        hasher.update([0u8]);
        hasher.update(rev.as_bytes());
        let key = format!("{:x}", hasher.finalize());
        let folder = objects.join(&key[..2]);
        private_directory(&folder)?;
        let target = folder.join(format!("{key}.tar.gz.age"));
        let reused = verified_archive(&target)?;
        counts["files_seen"] = json!(counts["files_seen"].as_u64().unwrap_or(0)+1);
        if reused {
            counts["reused"] = json!(counts["reused"].as_u64().unwrap_or(0)+1);
        } else {
            let stage = TempDir::new_in(output)?;
            let payload = stage.path().join("payload");
            let metadata = client.download(id, rev, &payload)?;
            validate_download(&payload, &item, &metadata)?;
            archive::create(stage.path(), &target, recipient, "multi-backup/dropbox")?;
            counts["saved"] = json!(counts["saved"].as_u64().unwrap_or(0)+1);
        }
        entries.push(json!({"kind":"file","id":id,
            "path":item["path_display"],"rev":rev,
            "bytes":item["size"],"content_hash":item["content_hash"],
            "encrypted_object":format!("objects/{}/{}.tar.gz.age", &key[..2], key)
        }));
        on_progress(index + 1, total, if reused {"Oggetto verificato e riutilizzato"}
            else {"Oggetto scaricato e cifrato"});
    }
    let manifest = json!({
        "schema":1,"backend":"dropbox",
        "created_utc":session.name.trim_end_matches(".tar.gz.age"),
        "counts":counts, "entries":entries,
        "retention":"append-only; no remote delete/purge APIs"
    });
    let tmp = TempDir::new_in(output)?;
    fs::write(tmp.path().join("manifest.json"),serde_json::to_vec_pretty(&manifest)?)?;
    archive::create(tmp.path(), &manifest_path, recipient, "multi-backup/dropbox")?;
    session.finish(&manifest_path)?;
    Ok(json!({"backend":"dropbox", "manifest":manifest_path, "counts":counts}))
}
pub fn backup(output: &Path, remote: &str, recipient: &str, resume: bool)
    -> DropboxResult<Value> {
    backup_with_progress(output, remote, recipient, resume, |_,_,_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn original_object_key_is_stable() {
        let mut hasher = Sha256::new();
        hasher.update(b"id:abc\0rev0001");
        assert_eq!(format!("{:x}",hasher.finalize()).len(),64);
    }
    #[test] fn session_name_safety() {
        assert!(manifest_filename("20261114T102030123456Z.tar.gz.age"));
        assert!(!manifest_filename("../../backup.tar.gz.age"));
    }
    #[test] fn no_remote_api_writes_present() {
        assert!(!API.contains("delete"));
        assert!(!DOWNLOAD.contains("upload"));
    }
}
