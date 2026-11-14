//! Rust-native read-only release discovery and SHA-256 verified update staging.
//! A staged package is NEVER executed or installed automatically. Only
//! release assets from the official repository and allowed HTTPS redirects
//! are downloaded. The existing vault, checkpoints and backups are untouched.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use url::Url;

pub type UpdateResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const API: &str = "https://api.github.com/repos/desalvo/multi-backup/releases";
const MAX_RELEASE_JSON: u64 = 2 * 1024 * 1024;
const MAX_ASSET_SIZE: u64 = 2 * 1024 * 1024 * 1024;

fn err(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
#[derive(Clone, Debug, Serialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReleaseInfo {
    pub channel: String,
    pub tag: String,
    pub published: String,
    pub asset: Option<ReleaseAsset>,
}
fn checked_url(input: &str) -> UpdateResult<Url> {
    let parsed = Url::parse(input)?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() ||
       parsed.password().is_some() || parsed.port_or_known_default() != Some(443) {
        return Err(err("Update URLs must use authenticated HTTPS"));
    }
    let host = parsed.host_str().unwrap_or("");
    if !["github.com","objects.githubusercontent.com",
          "release-assets.githubusercontent.com"].contains(&host) {
        return Err(err("External update download host rejected"));
    }
    Ok(parsed)
}
pub fn asset_prefix() -> UpdateResult<String> {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "macos" if arch == "aarch64" => Ok("multi-backup-rust-preview-macos-arm64".into()),
        "macos" if arch == "x86_64" => Ok("multi-backup-rust-preview-macos-x86_64".into()),
        "windows" if arch == "x86_64" => Ok("multi-backup-rust-preview-windows-x64".into()),
        "linux" if arch == "x86_64" => {
            let text = fs::read_to_string("/etc/os-release")?;
            let mut distro = "";
            let mut version = "";
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("ID=") {
                    distro = value.trim_matches('"');
                }
                if let Some(value) = line.strip_prefix("VERSION_ID=") {
                    version = value.trim_matches('"');
                }
            }
            let major = version.split('.').next().unwrap_or("");
            match (distro, major) {
                ("rocky", "8" | "9" | "10") =>
                    Ok(format!("multi-backup-rust-rocky-el{major}-")),
                ("almalinux", "8" | "9" | "10") =>
                    Ok(format!("multi-backup-rust-alma-el{major}-")),
                _ => Err(err("No signed Rust update channel for this Linux platform")),
            }
        }
        _ => Err(err("Unsupported native update platform/architecture")),
    }
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build()
}
pub fn get_release(channel: &str) -> UpdateResult<ReleaseInfo> {
    let endpoint = match channel {
        "stable" => format!("{API}/latest"),
        "early-release" => format!("{API}/tags/early-release"),
        _ => return Err(err("Unsupported update channel")),
    };
    let response = agent().get(&endpoint)
        .set("User-Agent","MultiBackupRust/0.1.0")
        .set("Accept","application/vnd.github+json").call()?;
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_RELEASE_JSON + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RELEASE_JSON {
        return Err(err("Release response exceeds limit"));
    }
    let release: Value = serde_json::from_slice(&bytes)?;
    let tag = release["tag_name"].as_str()
        .ok_or_else(|| err("Release tag missing"))?;
    if channel == "early-release" && tag != "early-release" {
        return Err(err("Unexpected early-release tag"));
    }
    if channel == "stable" && release["prerelease"] != false {
        return Err(err("Stable release is flagged prerelease"));
    }
    let prefix = asset_prefix()?;
    let assets = release["assets"].as_array()
        .ok_or_else(|| err("Release asset list missing"))?;
    let mut candidates = Vec::new();
    for item in assets {
        let Some(name) = item["name"].as_str() else { continue; };
        if !name.starts_with(&prefix) ||
           ![".dmg",".exe",".rpm",".zip",".tar.gz"].iter()
             .any(|suffix|name.ends_with(suffix)) ||
           name.contains('/') || name.contains('\\') { continue; }
        let Some(url) = item["browser_download_url"].as_str() else { continue; };
        let parsed = checked_url(url)?;
        if parsed.host_str() != Some("github.com") ||
           !parsed.path().starts_with("/desalvo/multi-backup/releases/download/") {
            continue;
        }
        let Some(digest) = item["digest"].as_str().and_then(|x|x.strip_prefix("sha256:")) else { continue; };
        if digest.len() != 64 || !digest.bytes().all(|x|x.is_ascii_hexdigit()) {
            continue;
        }
        let size = item["size"].as_u64().unwrap_or(MAX_ASSET_SIZE + 1);
        if size == 0 || size > MAX_ASSET_SIZE { continue; }
        candidates.push(ReleaseAsset {
            name:name.into(),download_url:url.into(),
            sha256:digest.to_ascii_lowercase(),bytes:size
        });
    }
    candidates.sort_by_key(|a|(
        if a.name.ends_with(".exe") || a.name.ends_with(".dmg") ||
           a.name.ends_with(".rpm") {0} else {1},a.name.clone()
    ));
    Ok(ReleaseInfo {
        channel:channel.into(),tag:tag.into(),
        published:release["published_at"].as_str().unwrap_or("").into(),
        asset:candidates.into_iter().next(),
    })
}
fn safe_output_dir(folder: &Path) -> UpdateResult<()> {
    if fs::symlink_metadata(folder).is_ok_and(|m|m.file_type().is_symlink()) {
        return Err(err("Update staging directory must not be a symlink"));
    }
    fs::create_dir_all(folder)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn stage(info: &ReleaseInfo, folder: &Path) -> UpdateResult<PathBuf> {
    let asset = info.asset.as_ref().ok_or_else(||err("No native Rust release asset available"))?;
    if asset.name.contains('/') || asset.name.contains('\\') ||
       asset.name == "." || asset.name == ".." {
        return Err(err("Unsafe update asset filename"));
    }
    checked_url(&asset.download_url)?;
    if asset.sha256.len() != 64 ||
       !asset.sha256.bytes().all(|b|b.is_ascii_hexdigit()) ||
       asset.bytes > MAX_ASSET_SIZE {
        return Err(err("Invalid update digest or asset length"));
    }
    safe_output_dir(folder)?;
    let final_path=folder.join(&asset.name);
    if fs::symlink_metadata(&final_path).is_ok() {
        return Err(err("Update staging asset already exists; refusing overwrite"));
    }
    let mut temporary = NamedTempFile::new_in(folder)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let agent=agent();
    let mut url=checked_url(&asset.download_url)?;
    let mut redirects=0u8;
    let response=loop {
        let result=agent.get(url.as_str()).set("User-Agent","MultiBackupRust/0.1.0").call();
        match result {
            Ok(reply) if (300..400).contains(&reply.status()) => {
                if redirects >= 5 { return Err(err("Update redirect limit exceeded")); }
                redirects += 1;
                let next=reply.header("Location")
                    .ok_or_else(||err("Update redirect location missing"))?;
                url=checked_url(next)?;
            },
            Ok(reply) => break reply,
            Err(ureq::Error::Status(status,reply)) if (300..400).contains(&status) => {
                if redirects >= 5 { return Err(err("Update redirect limit exceeded")); }
                redirects += 1;
                let next=reply.header("Location")
                    .ok_or_else(||err("Update redirect location missing"))?;
                url=checked_url(next)?;
            },
            Err(error) => return Err(Box::new(error)),
        }
    };
    let mut hash=Sha256::new();
    let mut reader=response.into_reader();
    let mut buffer=[0u8; 1_048_576];
    let mut total=0u64;
    loop {
        let n=reader.read(&mut buffer)?;
        if n == 0 { break; }
        total=total.checked_add(n as u64).ok_or_else(||err("Update file size overflow"))?;
        if total > MAX_ASSET_SIZE { return Err(err("Native update exceeds permitted size")); }
        hash.update(&buffer[..n]);
        temporary.write_all(&buffer[..n])?;
    }
    if total != asset.bytes ||
       format!("{:x}",hash.finalize()) != asset.sha256 {
        return Err(err("Update asset length/SHA-256 verification failed"));
    }
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&final_path).map_err(|e|e.error)?;
    Ok(final_path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rejects_untrusted_update_urls() {
        assert!(checked_url("http://github.com/example").is_err());
        assert!(checked_url("https://example.org/file").is_err());
        assert!(checked_url("https://user:pass@github.com/file").is_err());
        assert!(checked_url("https://github.com.evil.invalid/test").is_err());
        assert!(checked_url("https://github.com/release").is_ok());
    }
    #[test] fn no_auto_install_or_execute_of_downloaded_code() {
        let d=tempfile::tempdir().unwrap();
        let info=ReleaseInfo {
            channel:"early-release".into(),tag:"early-release".into(),
            published:"".into(),
            asset:Some(ReleaseAsset {
                name:"../evil.exe".into(),download_url:"https://github.com/evil".into(),
                sha256:"0".repeat(64),bytes:1,
            })
        };
        assert!(stage(&info,d.path()).is_err());
    }
}
