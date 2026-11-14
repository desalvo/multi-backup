//! Audited local deletion parity for the existing Python archive management.
//! Never touches provider APIs; only explicitly selected registered ciphertext
//! and its same-directory verification metadata can be deleted.
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};
pub type DeleteResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
pub fn plan(output: &Path, archive: &Path) -> DeleteResult<Vec<PathBuf>> {
    if fs::symlink_metadata(output)?.file_type().is_symlink() {
        return Err(err("Backup root cannot be a symlink"));
    }
    let root = output.canonicalize()?;
    if !fs::metadata(&root)?.is_dir() { return Err(err("Invalid backup root")); }
    let supplied = if archive.is_absolute() {
        archive.to_path_buf()
    } else { std::env::current_dir()?.join(archive) };
    let relative = supplied.strip_prefix(&root)
        .map_err(|_| err("Refusing deletion outside the backup root"))?;
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(err("Unsafe backup archive path component"));
    }
    let mut checked = root.clone();
    for component in relative.components() {
        checked.push(component.as_os_str());
        if fs::symlink_metadata(&checked)?.file_type().is_symlink() {
            return Err(err("Symlink in archive path"));
        }
    }
    if checked.to_string_lossy() != supplied.to_string_lossy() ||
        !checked.to_string_lossy().ends_with(".tar.gz.age") ||
        !fs::symlink_metadata(&checked)?.file_type().is_file() {
        return Err(err("Invalid or unregistered encrypted archive file"));
    }
    let is_registered = {
        let text = relative.to_string_lossy().replace('\\', "/");
        let file = checked.file_name().unwrap_or_default().to_string_lossy();
        text.starts_with("metadata/") ||
        text.starts_with("_metadata/") ||
        text.starts_with("dropbox/objects/") ||
        text.starts_with("dropbox/manifests/") ||
        (text.starts_with("repositories/") &&
          (file.contains("__git-") || text.contains("/native-snapshots/")))
    };
    if !is_registered {
        return Err(err("Archive not in the authorized local catalog"));
    }
    let state = root.join(".state");
    if state.exists() && fs::symlink_metadata(&state)?.file_type().is_symlink() {
        return Err(err("Unsafe checkpoint directory"));
    }
    let active = state.join("active.json");
    if active.exists() {
        if !fs::symlink_metadata(&active)?.file_type().is_file() {
            return Err(err("Invalid active checkpoint"));
        }
        let checkpoint: Value = serde_json::from_reader(File::open(active)?)?;
        let steps = checkpoint["steps"].as_object()
            .ok_or_else(|| err("Unreadable active checkpoint steps"))?;
        for step in steps.values() {
            if step["path"].as_str().is_some_and(|saved| Path::new(saved) == checked) {
                return Err(err("Archive belongs to unfinished backup; resume first"));
            }
        }
    }
    let dropbox = state.join("dropbox-active.json");
    if dropbox.exists() {
        if !fs::symlink_metadata(&dropbox)?.file_type().is_file() {
            return Err(err("Invalid Dropbox resume state"));
        }
        let state: Value = serde_json::from_reader(File::open(dropbox)?)?;
        if checked.to_string_lossy().contains("/dropbox/manifests/") &&
            checked.file_name().and_then(|v| v.to_str()) ==
                state["manifest_name"].as_str() {
            return Err(err("Encrypted manifest is in an incomplete Dropbox cycle"));
        }
    }
    let mut files = vec![checked.clone()];
    for suffix in [".sha256", ".json", ".artifact-omissions.json"] {
        let companion = PathBuf::from(format!("{}{}", checked.display(), suffix));
        if let Ok(meta) = fs::symlink_metadata(&companion) {
            if !meta.file_type().is_file() {
                return Err(err("Unsafe companion archive metadata type"));
            }
            files.push(companion);
        }
    }
    Ok(files)
}
pub fn delete(output: &Path, archive: &Path,
              confirmation: &str, in_use: bool) -> DeleteResult<Vec<String>> {
    if in_use { return Err(err("Backup/restore operation is active")); }
    if archive.file_name().and_then(|x| x.to_str()) != Some(confirmation) {
        return Err(err("Confirmation must exactly match encrypted archive filename"));
    }
    let files = plan(output, archive)?;
    let root = output.canonicalize()?;
    let state = root.join(".state");
    if let Ok(meta) = fs::symlink_metadata(&state) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(err("Unsafe audit directory"));
        }
    } else {
        fs::create_dir(&state)?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
        }
    }
    let log = state.join("archive-deletions.jsonl");
    if fs::symlink_metadata(&log).is_ok_and(|m|m.file_type().is_symlink()) {
        return Err(err("Unsafe deletion audit log"));
    }
    let mut audit = OpenOptions::new().append(true).create(true).open(&log)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        audit.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut removed = Vec::new();
    let outcome: DeleteResult<()> = (|| {
        for item in &files {
            fs::remove_file(item)?;
            removed.push(item.strip_prefix(&root)?.to_string_lossy().replace('\\', "/"));
        }
        Ok(())
    })();
    let record = json!({
        "utc": Utc::now().to_rfc3339(),
        "requested": files[0].strip_prefix(&root)?.to_string_lossy().replace('\\', "/"),
        "removed": removed,
        "complete": outcome.is_ok() && removed.len() == files.len(),
        "operation": "local-delete-only"
    });
    serde_json::to_writer(&mut audit, &record)?;
    audit.write_all(b"\n")?;
    audit.sync_all()?;
    outcome?;
    Ok(removed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn deletes_only_selected_local_archive_with_audit() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let folder = root.join("metadata"); fs::create_dir(&folder).unwrap();
        let file = folder.join("example.tar.gz.age");
        fs::write(&file, b"ciphertext").unwrap();
        fs::write(format!("{}.sha256",file.display()), b"hash").unwrap();
        assert!(delete(root, &file, "wrong", false).is_err());
        assert!(delete(root, &file, "example.tar.gz.age", true).is_err());
        let removed = delete(root, &file, "example.tar.gz.age", false).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!file.exists());
        let log = fs::read_to_string(root.join(".state/archive-deletions.jsonl")).unwrap();
        assert!(log.contains("local-delete-only"));
        assert!(log.contains("\"complete\":true"));
    }
    #[test] fn refuses_outside_root_even_with_matching_filename() {
        let d = tempfile::tempdir().unwrap();
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(plan(d.path(), file.path()).is_err());
    }
}
