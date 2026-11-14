//! Transactional Rust-native schema-1 journal compatible with checkpoint.py.
//! A locked directory journal preserves immutable archives on any failure.
use chrono::Utc;
use fs2::FileExt;
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}};
use tempfile::NamedTempFile;

pub type JournalResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
fn no_symlink(path: &Path) -> JournalResult<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() { return Err(err("Journal symlinks are forbidden")); }
    }
    Ok(())
}
fn private_dir(path: &Path) -> JournalResult<()> {
    no_symlink(path)?;
    fs::create_dir_all(path)?;
    no_symlink(path)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(err("Journal directory must be private (0700)"));
        }
    }
    Ok(())
}
fn unique_hex() -> String {
    let mut buf = [0u8; 4];
    OsRng.fill_bytes(&mut buf);
    buf.iter().map(|v| format!("{v:02x}")).collect()
}

// Python json.dumps(config, sort_keys=True) defaults to ": " and ", " with
// ensure_ascii=True. We preserve those bytes for exact resume hash parity.
pub fn python_json(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(v) => if *v { "true" } else { "false" }.into(),
        Value::Number(v) => v.to_string(),
        Value::String(s) => {
            let mut escaped = String::from("\"");
            for ch in s.chars() {
                match ch {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    '\u{8}' => escaped.push_str("\\b"),
                    '\u{c}' => escaped.push_str("\\f"),
                    '\n' => escaped.push_str("\\n"),
                    '\r' => escaped.push_str("\\r"),
                    '\t' => escaped.push_str("\\t"),
                    c if c < '\u{20}' || c > '\u{7e}' => {
                        for unit in c.encode_utf16(&mut [0u16; 2]).iter() {
                            escaped.push_str(&format!("\\u{unit:04x}"));
                        }
                    },
                    c => escaped.push(c),
                }
            }
            escaped.push('"');
            escaped
        },
        Value::Array(values) => {
            format!("[{}]", values.iter().map(python_json).collect::<Vec<_>>().join(", "))
        },
        Value::Object(entries) => {
            let mut keys = entries.keys().collect::<Vec<_>>();
            keys.sort();
            format!("{{{}}}", keys.iter().map(|k| {
                format!("{}: {}", python_json(&Value::String((*k).clone())),
                    python_json(entries.get(*k).unwrap()))
            }).collect::<Vec<_>>().join(", "))
        },
    }
}
pub fn config_hash(config: &Value) -> String {
    format!("{:x}", Sha256::digest(python_json(config).as_bytes()))
}
fn atomic_json(path: &Path, data: &Value) -> JournalResult<()> {
    let parent = path.parent().ok_or_else(|| err("Journal path lacks parent"))?;
    private_dir(parent)?;
    no_symlink(path)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut encoded = serde_json::to_vec_pretty(data)?;
    encoded.push(b'\n');
    temporary.write_all(&encoded)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn read_state(path: &Path) -> JournalResult<Value> {
    no_symlink(path)?;
    let meta = fs::metadata(path)?;
    if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
        return Err(err("Invalid or oversized checkpoint"));
    }
    let value: Value = serde_json::from_reader(File::open(path)?)?;
    if value["schema"] != 1 || !value["steps"].is_object() ||
       value["cycle"].as_str().is_none() {
        return Err(err("Unsupported checkpoint schema"));
    }
    Ok(value)
}
fn valid_cycle(cycle: &str) -> bool {
    (8..=80).contains(&cycle.len()) &&
        cycle.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn digest_file(path: &Path) -> JournalResult<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(err("Nonregular archive checkpoint file"));
    }
    let mut hasher = Sha256::new();
    let mut stream = File::open(path)?;
    let mut buf = [0u8; 65536];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub struct Journal {
    directory: PathBuf,
    active: PathBuf,
    pub data: Value,
    pub resumed: bool,
    _lock: File,
}
impl Journal {
    pub fn start(directory: &Path, config: &Value,
                 restart: bool, require_active: bool) -> JournalResult<Self> {
        if restart && require_active {
            return Err(err("Cannot select both Start and Resume"));
        }
        private_dir(directory)?;
        let lockpath = directory.join("rust-journal.lock");
        no_symlink(&lockpath)?;
        let lock = OpenOptions::new().create(true).read(true).write(true).open(&lockpath)?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            lock.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        lock.try_lock_exclusive().map_err(|_| err("Another backup is using this checkpoint"))?;
        let active = directory.join("active.json");
        no_symlink(&active)?;
        let existing = active.is_file();
        if require_active && !existing {
            return Err(err("No incomplete backup to resume"));
        }
        let resumed = existing && !restart;
        let digest = config_hash(config);
        let data = if resumed {
            let state = read_state(&active)?;
            if state["config_hash"] != digest {
                return Err(err("Resume configuration changed"));
            }
            if state["finished"] == true {
                return Err(err("Last cycle finished; use Start for a new backup"));
            }
            state
        } else {
            if existing {
                let state = read_state(&active)?;
                let name = state["cycle"].as_str().ok_or_else(|| err("Cycle name missing"))?;
                if !valid_cycle(name) { return Err(err("Malformed previous cycle name")); }
                let parked = directory.join(format!("abandoned-{name}.json"));
                if fs::symlink_metadata(&parked).is_ok() {
                    return Err(err("Previous incomplete checkpoint already parked"));
                }
                // Preserve old checkpoint BEFORE modifying active state.
                let saved = OpenOptions::new().write(true).create_new(true).open(&parked)?;
                drop(saved);
                atomic_json(&parked, &state)?;
            }
            let cycle = format!("{}-{}", Utc::now().format("%Y%m%dT%H%M%S%6fZ"), unique_hex());
            let value = json!({
                "schema": 1, "cycle": cycle, "config_hash": digest,
                "steps": {}, "finished": false,
                "started_utc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
            });
            atomic_json(&active, &value)?;
            value
        };
        Ok(Self { directory: directory.to_path_buf(), active, data,
                  resumed, _lock: lock })
    }
    pub fn cycle(&self) -> JournalResult<&str> {
        self.data["cycle"].as_str().ok_or_else(|| err("Cycle name missing"))
    }
    pub fn done(&self, step: &str) -> bool {
        let Some(record) = self.data["steps"].get(step) else { return false };
        let (Some(path), Some(size), Some(hash)) =
            (record["path"].as_str(), record["size"].as_u64(),
             record["sha256"].as_str()) else { return false };
        let candidate = Path::new(path);
        if !candidate.is_absolute() { return false; }
        let Ok(meta) = fs::symlink_metadata(candidate) else { return false };
        if !meta.is_file() || meta.len() != size { return false; }
        digest_file(candidate).is_ok_and(|computed| computed == hash)
    }
    pub fn record(&mut self, step: &str, file: &Path) -> JournalResult<()> {
        if step.is_empty() || step.len() > 1024 || step.chars().any(|c| c.is_control()) {
            return Err(err("Invalid checkpoint step name"));
        }
        let path = file.canonicalize()?;
        if !fs::symlink_metadata(file)?.file_type().is_file() {
            return Err(err("Checkpoint path must be a regular file"));
        }
        let size = fs::metadata(&path)?.len();
        let digest = digest_file(&path)?;
        self.data["steps"][step] = json!({
            "path": path.to_string_lossy(), "size": size, "sha256": digest
        });
        atomic_json(&self.active, &self.data)
    }
    pub fn finish(&mut self) -> JournalResult<()> {
        if self.data["finished"] == true {
            return Err(err("Cycle was already finalized"));
        }
        let names = self.data["steps"].as_object()
            .ok_or_else(|| err("Checkpoint steps missing"))?;
        if names.keys().any(|name| !self.done(name)) {
            return Err(err("Cannot finalize: a recorded archive is missing or damaged"));
        }
        self.data["finished"] = json!(true);
        self.data["finished_utc"] = json!(Utc::now().to_rfc3339());
        let record = self.directory.join(format!("cycle-{}.json", self.cycle()?));
        if fs::symlink_metadata(&record).is_ok() {
            return Err(err("Final cycle record already exists"));
        }
        atomic_json(&record, &self.data)?;
        fs::remove_file(&self.active)?;
        Ok(())
    }
    pub fn status(&self) -> &Value { &self.data }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn python_dump_hash_matches_reference() {
        let c = json!({"recipient":"age1ABC", "max_runs": 5, "active": true,
                       "nested": {"français": "été"}, "other": null});
        let py = "{\"active\": true, \"max_runs\": 5, \"nested\": {\"fran\\u00e7ais\": \"\\u00e9t\\u00e9\"}, \"other\": null, \"recipient\": \"age1ABC\"}";
        assert_eq!(python_json(&c), py);
        assert_eq!(config_hash(&c), format!("{:x}", Sha256::digest(py.as_bytes())));
    }
    #[test] fn durable_start_resume_and_guard() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join(".state"); fs::create_dir(&state).unwrap();
        let cfg = json!({"recipient": "age1example", "max_runs_per_repo": 5});
        let file = tmp.path().join("sample.tar.gz.age");
        fs::write(&file, b"ciphertext fixture").unwrap();
        let mut journal = Journal::start(&state, &cfg, false, false).unwrap();
        let name = journal.cycle().unwrap().to_string();
        journal.record("archive", &file).unwrap();
        assert!(journal.done("archive"));
        assert!(Journal::start(&state, &cfg, false, true).is_err());
        drop(journal);
        let mut resumed = Journal::start(&state, &cfg, false, true).unwrap();
        assert_eq!(resumed.cycle().unwrap(), name);
        assert!(resumed.done("archive"));
        assert!(Journal::start(&state, &json!({"other":3}), false, true).is_err());
        resumed.finish().unwrap();
        assert!(!state.join("active.json").exists());
        assert!(state.join(format!("cycle-{name}.json")).exists());
    }
}
