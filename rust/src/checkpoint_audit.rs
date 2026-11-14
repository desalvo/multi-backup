//! Read-only audit of r018 Python checkpoint and archived step records.
//! The native audit does not rewrite or finalize user checkpoint state.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs::{self, File}, io::Read, path::Path};

pub type AuditResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Serialize)]
pub struct Audit {
    pub schema: u64,
    pub cycle: String,
    pub valid: Vec<String>,
    pub invalid: Vec<String>,
    pub finished: bool,
}

fn digest_file(path: &Path) -> AuditResult<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err("Referenced checkpoint file must not be a symlink".into());
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn audit(file: &Path) -> AuditResult<Audit> {
    let meta = fs::symlink_metadata(file)?;
    if !meta.file_type().is_file() || meta.len() > 16 * 1024 * 1024 {
        return Err("Refusing unsafe or oversized checkpoint".into());
    }
    let data: Value = serde_json::from_reader(File::open(file)?)?;
    let schema = data.get("schema").and_then(Value::as_u64)
        .ok_or("Checkpoint schema missing")?;
    if schema != 1 { return Err("Unsupported checkpoint schema".into()); }
    let cycle = data.get("cycle").and_then(Value::as_str)
        .ok_or("Cycle name missing")?.to_string();
    if !(8..=80).contains(&cycle.len()) ||
       !cycle.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err("Invalid checkpoint cycle name".into());
    }
    let finished = data.get("finished").and_then(Value::as_bool)
        .ok_or("Checkpoint completion flag missing")?;
    let digest = data.get("config_hash").and_then(Value::as_str)
        .ok_or("Checkpoint config hash missing")?;
    if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid checkpoint config hash".into());
    }
    let steps = data.get("steps").and_then(Value::as_object)
        .ok_or("Checkpoint step records missing")?;
    if steps.len() > 500_000 { return Err("Excessive checkpoint records".into()); }
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for (name, record) in steps {
        let test = (|| -> AuditResult<bool> {
            if name.is_empty() || name.len() > 1024 ||
               name.chars().any(|c| c.is_control()) {
                return Err("Unsafe step name".into());
            }
            let pathname = record.get("path").and_then(Value::as_str)
                .ok_or("Step path missing")?;
            let expected_size = record.get("size").and_then(Value::as_u64)
                .ok_or("Step size missing")?;
            let sha = record.get("sha256").and_then(Value::as_str)
                .ok_or("Step SHA-256 missing")?;
            if sha.len() != 64 || !sha.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Ok(false);
            }
            let path = Path::new(pathname);
            if !path.is_absolute() || !path.is_file() {
                return Ok(false);
            }
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.file_type().is_file() || metadata.len() != expected_size {
                return Ok(false);
            }
            Ok(digest_file(path)? == sha.to_ascii_lowercase())
        })();
        if matches!(test, Ok(true)) {
            valid.push(name.clone());
        } else {
            invalid.push(name.clone());
        }
    }
    Ok(Audit { schema, cycle, valid, invalid, finished })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_checkpoint_format() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("active.json");
        fs::write(&f, br#"{"schema":2,"cycle":"abcdefgh","steps":{}}"#).unwrap();
        assert!(audit(&f).is_err());
    }
    #[test]
    fn identifies_tampering_without_changing_data() {
        let d = tempfile::tempdir().unwrap();
        let payload = d.path().join("archive.age");
        fs::write(&payload, b"encrypted").unwrap();
        let f = d.path().join("active.json");
        let good = format!("{:x}", Sha256::digest(b"encrypted"));
        let config = "0".repeat(64);
        let data = serde_json::json!({
            "schema": 1, "cycle": "20261114T123456789Z-abc123ef",
            "config_hash": config, "steps": {
                "archive": {"path": payload.to_str().unwrap(), "size": 9, "sha256": good}},
            "finished": false
        });
        fs::write(&f, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(audit(&f).unwrap().valid, vec!["archive"]);
        fs::write(&payload, b"modified!").unwrap();
        assert_eq!(audit(&f).unwrap().invalid, vec!["archive"]);
        assert!(!audit(&f).unwrap().finished);
    }
}
