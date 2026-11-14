//! Explicit, non-overwriting remote restoration from authentic age archives.
//! No remote operation occurs during preview/list/verification; all remote
//! writes are only reachable through dedicated user-requested CLI commands.
use crate::archive;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::Command,
    time::Duration,
};
use tempfile::TempDir;
use ureq::{Agent, AgentBuilder};

pub type RestoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
pub fn github_slug(slug: &str) -> RestoreResult<(&str, &str)> {
    let parts: Vec<_> = slug.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| {
        p.is_empty() || p.len() > 100 || *p == "." || *p == ".." ||
        !p.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    }) {
        return Err(err("Target must be a safe GitHub owner/new-repository slug"));
    }
    Ok((parts[0], parts[1]))
}
fn checked_command(command: &str, args: &[&str]) -> RestoreResult<()> {
    if command != "gh" && command != "git" { return Err(err("Unsupported remote helper")); }
    let status = Command::new(command).args(args).status()?;
    if !status.success() { return Err(err("Remote GitHub restore command failed")); }
    Ok(())
}
enum Identity<'a> {
    File(&'a Path),
    Memory(&'a [u8]),
}
impl<'a> Identity<'a> {
    fn extract(self, archive_path: &Path, destination: &Path) -> RestoreResult<()> {
        match self {
            Self::File(file) => { archive::extract(archive_path, file, destination)?; },
            Self::Memory(secret) => {
                archive::extract_from_memory(archive_path, secret, destination)?;
            },
        }
        Ok(())
    }
}
pub fn github_new_repository(archive_path: &Path, identity: &Path,
                             target: &str, private: bool) -> RestoreResult<String> {
    github_new_repository_with(archive_path, Identity::File(identity), target, private)
}
pub fn github_new_repository_from_memory(archive_path: &Path, secret: &[u8],
                                         target: &str, private: bool) -> RestoreResult<String> {
    github_new_repository_with(archive_path, Identity::Memory(secret), target, private)
}
fn github_new_repository_with(archive_path: &Path, identity: Identity<'_>,
                             target: &str, private: bool) -> RestoreResult<String> {
    github_slug(target)?;
    let temp = TempDir::new()?;
    let extracted = temp.path().join("decoded");
    identity.extract(archive_path, &extracted)?;
    let mut matches = Vec::new();
    for found in walkdir::WalkDir::new(&extracted).follow_links(false)
        .max_depth(12).into_iter().filter_map(Result::ok)
    {
        if !found.file_type().is_dir() { continue; }
        let path = found.path();
        if path.extension().and_then(|s|s.to_str()) != Some("git") { continue; }
        if path.parent().and_then(|p|p.file_name()).and_then(|s|s.to_str()) == Some("git") {
            matches.push(path.to_path_buf());
        }
    }
    if matches.len() != 1 {
        return Err(err("Authenticated archive must contain exactly one Git bare mirror"));
    }
    let git_dir = matches.remove(0);
    if !git_dir.join("HEAD").is_file() || !git_dir.join("objects").is_dir() {
        return Err(err("Archive did not contain a complete Git mirror"));
    }
    let source = git_dir.to_str().ok_or_else(|| err("Git mirror path is not UTF-8"))?;
    checked_command("gh", &["auth","status"])?;
    // gh repo view exits 0 for an existing destination. Refuse any overwrite.
    if Command::new("gh").args(["repo","view",target,"--json","name"])
        .output()?.status.success() {
        return Err(err("Destination GitHub repository already exists"));
    }
    checked_command("gh", &[
        "repo","create",target,
        if private {"--private"} else {"--public"}
    ])?;
    let url = format!("https://github.com/{target}.git");
    checked_command("gh", &["auth","setup-git"])?;
    checked_command("git", &["-C",source,"fsck","--full"])?;
    checked_command("git", &["-C",source,"push","--mirror",&url])?;
    // Never return success if LFS content exists but could not be pushed.
    checked_command("git", &["-C",source,"lfs","push","--all",&url])?;
    Ok(format!("https://github.com/{target}"))
}
fn dropbox_target(path: &str) -> RestoreResult<()> {
    if !path.starts_with('/') || path.ends_with('/') || path.len() > 1024 ||
        path.contains('\n') || path.contains('\r') ||
        path.split('/').skip(1).any(|p| p.is_empty() || p == "." || p == "..") {
        return Err(err("Dropbox restore target must be a new absolute file path"));
    }
    Ok(())
}
struct DropboxWriter {
    agent: Agent,
    token: String,
}
impl DropboxWriter {
    fn new() -> RestoreResult<Self> {
        let token = std::env::var("MULTI_BACKUP_DROPBOX_WRITE_TOKEN")?;
        if token.len() < 12 || token.contains('\n') || token.contains('\r') {
            return Err(err("Dropbox write token not provided"));
        }
        Ok(Self {
            agent: AgentBuilder::new()
                .timeout_connect(Duration::from_secs(20))
                .timeout_read(Duration::from_secs(180))
                .timeout_write(Duration::from_secs(180))
                .redirects(0).build(),
            token,
        })
    }
    fn upload(&self, name: &str, payload: &Value, bytes: &[u8]) -> RestoreResult<Value> {
        if !["upload_session/start", "upload_session/append_v2",
              "upload_session/finish"].contains(&name) {
            return Err(err("Unauthorized Dropbox remote write operation"));
        }
        let url = format!("https://content.dropboxapi.com/2/files/{name}");
        let response = self.agent.post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Dropbox-API-Arg", &payload.to_string())
            .set("Content-Type","application/octet-stream")
            .send_bytes(bytes);
        match response {
            Ok(value) => {
                let mut bytes = Vec::new();
                value.into_reader().take(1024 * 1024).read_to_end(&mut bytes)?;
                // Dropbox upload_session/append_v2 succeeds with an empty body.
                let content: Value = if bytes.is_empty() { json!({}) }
                    else { serde_json::from_slice(&bytes)? };
                if !content.is_object() { return Err(err("Dropbox write was not acknowledged")); }
                Ok(content)
            },
            Err(ureq::Error::Status(_, _)) => {
                Err(err("Dropbox rejected the new-path upload; no overwrite permitted"))
            },
            Err(_) => Err(err("Dropbox upload network operation failed")),
        }
    }
}
pub fn dropbox_new_file(archive_path: &Path, identity: &Path,
                        target: &str) -> RestoreResult<Value> {
    dropbox_new_file_with(archive_path, Identity::File(identity), target)
}
pub fn dropbox_new_file_from_memory(archive_path: &Path, secret: &[u8],
                                    target: &str) -> RestoreResult<Value> {
    dropbox_new_file_with(archive_path, Identity::Memory(secret), target)
}
fn dropbox_new_file_with(archive_path: &Path, identity: Identity<'_>,
                        target: &str) -> RestoreResult<Value> {
    dropbox_target(target)?;
    let temp = TempDir::new()?;
    let extracted = temp.path().join("decoded");
    identity.extract(archive_path, &extracted)?;
    let files = walkdir::WalkDir::new(&extracted).follow_links(false)
        .into_iter().filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path()).collect::<Vec<_>>();
    if files.len() != 1 ||
       files[0].file_name().and_then(|s|s.to_str()) != Some("payload") {
        return Err(err("Dropbox archive must contain exactly one payload file"));
    }
    let payload = &files[0];
    let total = fs::metadata(payload)?.len();
    let hash = crate::dropbox_hash::hash_file(payload)?;
    let mut file = File::open(payload)?;
    let writer = DropboxWriter::new()?;
    let chunk_size = 8 * 1024 * 1024;
    let mut buffer = vec![0u8; chunk_size];
    let first = file.read(&mut buffer)?;
    let session = writer.upload("upload_session/start", &json!({"close":false}),
                                &buffer[..first])?;
    let session_id = session["session_id"].as_str()
        .ok_or_else(|| err("Dropbox did not return a session id"))?;
    let mut offset = first as u64;
    while total.saturating_sub(offset) > chunk_size as u64 {
        let n = file.read(&mut buffer)?;
        if n == 0 { return Err(err("Restore input ended unexpectedly")); }
        writer.upload("upload_session/append_v2",
                      &json!({"cursor":{"session_id":session_id,"offset":offset},
                              "close":false}), &buffer[..n])?;
        offset = offset.checked_add(n as u64).ok_or_else(|| err("Upload size overflow"))?;
    }
    let mut last = Vec::with_capacity(chunk_size);
    file.read_to_end(&mut last)?;
    if offset.checked_add(last.len() as u64) != Some(total) {
        return Err(err("Restore archive file changed during upload"));
    }
    let commit = json!({
        "path":target,"mode":"add","autorename":false,
        "strict_conflict":true,"mute":false
    });
    let answer = writer.upload("upload_session/finish",
        &json!({"cursor":{"session_id":session_id,"offset":offset},
                "commit":commit}), &last)?;
    if answer["id"].as_str().is_none() ||
       answer["content_hash"].as_str() != Some(hash.as_str()) {
        return Err(err("Dropbox did not confirm the expected uploaded file integrity"));
    }
    Ok(answer)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn safe_remote_targets_only() {
        for bad in ["old","/","/foo/../bar","/a//b","/a/\nname"] {
            assert!(dropbox_target(bad).is_err());
        }
        assert!(dropbox_target("/newdir/restored.txt").is_ok());
        assert!(github_slug("org/next-repo").is_ok());
        assert!(github_slug("org/../../bad").is_err());
    }
}
