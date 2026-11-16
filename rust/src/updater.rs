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
/// Exact platform/distribution detection: no cross-distro package fallback.
fn asset_target(os:&str,arch:&str,os_release:&str)->UpdateResult<(String,String)>{
    match (os,arch){
        ("macos","aarch64")=>Ok(("multi-backup-rust-preview-macos-arm64".into(),".dmg".into())),
        ("macos","x86_64")=>Ok(("multi-backup-rust-preview-macos-x86_64".into(),".dmg".into())),
        ("windows","x86_64")=>Ok(("multi-backup-rust-preview-windows-x64".into(),".exe".into())),
        ("linux","x86_64"|"aarch64")=>{
            let mut distro="";let mut version="";
            for line in os_release.lines(){
                if let Some(v)=line.strip_prefix("ID="){distro=v.trim_matches('"');}
                if let Some(v)=line.strip_prefix("VERSION_ID="){version=v.trim_matches('"');}
            }
            let major=version.split('.').next().unwrap_or("");
            match (distro,major,arch){
                ("rocky"|"almalinux","8"|"9"|"10","x86_64")=>{
                    let vendor=if distro=="rocky"{"rocky"}else{"alma"};
                    Ok((format!("multi-backup-rust-{vendor}-el{major}-"),".rpm".into()))
                }
                ("debian","12"|"13",_)=>{
                    let cpu=if arch=="x86_64"{"amd64"}else{"arm64"};
                    Ok(("multi-backup-rust_".into(),format!("+deb{major}_{cpu}.deb")))
                }
                ("ubuntu","24","x86_64")=>
                    Ok(("multi-backup-rust-preview-ubuntu24-x64".into(),".tar.gz".into())),
                _=>Err(err("No verified native Rust update for this Linux distribution")),
            }
        },
        _=>Err(err("Unsupported native update operating system or CPU")),
    }
}
fn local_asset_target()->UpdateResult<(String,String)>{
    let distro=if std::env::consts::OS=="linux"{
        fs::read_to_string("/etc/os-release")?
    }else{String::new()};
    asset_target(std::env::consts::OS,std::env::consts::ARCH,&distro)
}
pub fn asset_prefix()->UpdateResult<String>{Ok(local_asset_target()?.0)}
fn rc_number(tag:&str)->Option<u64>{
    let n=tag.strip_prefix("rc-0.1.0-rc.")?;
    if n.is_empty()||n.starts_with('0')||!n.bytes().all(|b|b.is_ascii_digit()){
        return None;
    }
    n.parse::<u64>().ok().filter(|x|*x>0)
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
        "rc" => format!("{API}?per_page=30"),
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
    let response_value: Value = serde_json::from_slice(&bytes)?;
    let release: Value=if channel=="rc" {
        let list=response_value.as_array().ok_or_else(||err("Invalid RC release list"))?;
        list.iter()
            .filter(|v|v["prerelease"]==true && v["draft"]==false)
            .filter_map(|v|rc_number(v["tag_name"].as_str()?).map(|n|(n,v)))
            .max_by_key(|(n,_)|*n)
            .map(|(_,v)|v.clone())
            .ok_or_else(||err("No valid RC release found"))?
    }else {response_value};
    let tag = release["tag_name"].as_str()
        .ok_or_else(|| err("Release tag missing"))?;
    if channel == "early-release" && tag != "early-release" {
        return Err(err("Unexpected early-release tag"));
    }
    if channel=="rc" && rc_number(tag).is_none(){
        return Err(err("Unexpected RC tag"));
    }
    if channel == "stable" && release["prerelease"] != false {
        return Err(err("Stable release is flagged prerelease"));
    }
    let (prefix,suffix) = local_asset_target()?;
    let assets = release["assets"].as_array()
        .ok_or_else(|| err("Release asset list missing"))?;
    let mut candidates = Vec::new();
    for item in assets {
        let Some(name) = item["name"].as_str() else { continue; };
        if !name.starts_with(&prefix) || !name.ends_with(&suffix) ||
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
    let source=checked_url(&asset.download_url)?;
    if source.host_str()!=Some("github.com") ||
       !source.path().starts_with("/desalvo/multi-backup/releases/download/") {
        return Err(err("Update source must be official GitHub release asset"));
    }
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
/// Atomic activation for a portable, user-owned versioned application tree.
/// The caller MUST stop the application first. This does not execute unsigned
/// downloaded installers, bypass Gatekeeper/UAC or change system package state.
/// On Windows, the caller must be an OUT-OF-PROCESS helper after process exit.
pub fn activate_portable_version(
    active:&Path,candidate:&Path,rollback:&Path,
    health:&dyn Fn(&Path)->UpdateResult<()>,
)->UpdateResult<()> {
    let parent=active.parent().ok_or_else(||err("Active app has no parent"))?;
    if candidate.parent()!=Some(parent) || rollback.parent()!=Some(parent) ||
       active==candidate || active==rollback || candidate==rollback {
        return Err(err("Version folders must be distinct siblings on one filesystem"));
    }
    let check_real_dir=|p:&Path|->UpdateResult<()>{
        let info=fs::symlink_metadata(p)?;
        if !info.file_type().is_dir() || info.file_type().is_symlink() {
            return Err(err("Portable version path is not a real directory"));
        }
        Ok(())
    };
    check_real_dir(parent)?;
    check_real_dir(active)?;
    check_real_dir(candidate)?;
    if fs::symlink_metadata(rollback).is_ok() {
        return Err(err("Rollback path must not already exist"));
    }
    // Rename is atomic within a filesystem. Never remove backups on error.
    fs::rename(active,rollback)?;
    if let Err(installation_error)=fs::rename(candidate,active) {
        let recovered=fs::rename(rollback,active);
        return match recovered {
            Ok(())=>Err(installation_error.into()),
            Err(_) =>Err(err("Installation failed and manual rollback is required")),
        };
    }
    if health(active).is_err() {
        // Put the rejected candidate back, then restore the original version.
        if fs::rename(active,candidate).is_err() {
            return Err(err("Health check failed; manual recovery from rollback directory required"));
        }
        fs::rename(rollback,active)?;
        return Err(err("New version failed health check; previous application restored"));
    }
    Ok(())
}

/// An explicit, user-requested restore of the previous portable version.
/// Move the current installation into a *new empty* rejected_version slot;
/// never delete it, and never overwrite any existing state.
pub fn rollback_portable_version(
    active:&Path,rollback:&Path,rejected_version:&Path,
    health:&dyn Fn(&Path)->UpdateResult<()>,
)->UpdateResult<()>{
    let parent=active.parent().ok_or_else(||err("Active path has no parent"))?;
    if rollback.parent()!=Some(parent)||rejected_version.parent()!=Some(parent)||
       active==rollback||active==rejected_version||rollback==rejected_version{
        return Err(err("Rollback paths must be distinct siblings"));
    }
    for p in [parent,active,rollback] {
        let md=fs::symlink_metadata(p)?;
        if !md.file_type().is_dir()||md.file_type().is_symlink(){
            return Err(err("Rollback must use genuine version directories"));
        }
    }
    if fs::symlink_metadata(rejected_version).is_ok(){
        return Err(err("Rejected-version slot already exists"));
    }
    fs::rename(active,rejected_version)?;
    if let Err(restore_error)=fs::rename(rollback,active){
        let _=fs::rename(rejected_version,active);
        return Err(restore_error.into());
    }
    if health(active).is_err(){
        // No deletion or forced replacement of an unreadable application.
        return Err(err("Previous version failed health check; operator recovery required"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn portable_activation_is_atomic_and_recovers_failed_health_check(){
        let root=tempfile::tempdir().unwrap();
        let active=root.path().join("active");
        let next=root.path().join("next");
        let previous=root.path().join("previous");
        fs::create_dir(&active).unwrap();
        fs::create_dir(&next).unwrap();
        fs::write(active.join("version"),b"old").unwrap();
        fs::write(next.join("version"),b"broken").unwrap();
        let health=|p:&Path|->UpdateResult<()>{
            if fs::read(p.join("version"))?==b"good" {Ok(())}
            else {Err(err("Invalid portable binary health check"))}
        };
        assert!(activate_portable_version(&active,&next,&previous,&health).is_err());
        assert_eq!(fs::read(active.join("version")).unwrap(),b"old");
        assert_eq!(fs::read(next.join("version")).unwrap(),b"broken");
        assert!(!previous.exists());
        fs::write(next.join("version"),b"good").unwrap();
        activate_portable_version(&active,&next,&previous,&health).unwrap();
        assert_eq!(fs::read(active.join("version")).unwrap(),b"good");
        assert_eq!(fs::read(previous.join("version")).unwrap(),b"old");
        let rejected=root.path().join("rejected");
        let accept_any=|p:&Path|->UpdateResult<()>{
            let _=fs::read(p.join("version"))?;Ok(())
        };
        rollback_portable_version(&active,&previous,&rejected,&accept_any).unwrap();
        assert_eq!(fs::read(active.join("version")).unwrap(),b"old");
        assert_eq!(fs::read(rejected.join("version")).unwrap(),b"good");
    }
    #[test] fn portable_activation_blocks_symlinks_existing_backups_and_cross_roots(){
        let root=tempfile::tempdir().unwrap();
        let active=root.path().join("active");
        let next=root.path().join("next");
        let previous=root.path().join("previous");
        fs::create_dir(&active).unwrap();
        fs::create_dir(&next).unwrap();
        fs::create_dir(&previous).unwrap();
        assert!(activate_portable_version(&active,&next,&previous,&|_|Ok(())).is_err());
        fs::remove_dir(&previous).unwrap();
        assert!(activate_portable_version(&active,&root.path().join("bad").join("next"),&previous,
            &|_|Ok(())).is_err());
        #[cfg(unix)]{
            use std::os::unix::fs::symlink;
            let link=root.path().join("fake");
            symlink(&next,&link).unwrap();
            assert!(activate_portable_version(&active,&link,&previous,&|_|Ok(())).is_err());
        }
    }


    #[test] fn rejects_untrusted_update_urls() {
        assert!(checked_url("http://github.com/example").is_err());
        assert!(checked_url("https://example.org/file").is_err());
        assert!(checked_url("https://user:pass@github.com/file").is_err());
        assert!(checked_url("https://github.com.evil.invalid/test").is_err());
        assert!(checked_url("https://github.com/release").is_ok());
    }
    #[test] fn platform_and_release_mapping_refuse_cross_distro_and_malformed_rc() {
        assert_eq!(asset_target("macos","aarch64","").unwrap().1,".dmg");
        assert_eq!(asset_target("windows","x86_64","").unwrap().1,".exe");
        assert_eq!(asset_target("linux","x86_64","ID=debian\nVERSION_ID=\"12\"").unwrap().1,
                   "+deb12_amd64.deb");
        assert_eq!(asset_target("linux","aarch64","ID=debian\nVERSION_ID=\"13\"").unwrap().1,
                   "+deb13_arm64.deb");
        assert_eq!(asset_target("linux","x86_64","ID=rocky\nVERSION_ID=\"10.1\"").unwrap().1,
                   ".rpm");
        assert!(asset_target("linux","x86_64","ID=other\nVERSION_ID=1").is_err());
        assert_eq!(rc_number("rc-0.1.0-rc.1"),Some(1));
        assert_eq!(rc_number("rc-0.1.0-rc.12"),Some(12));
        for invalid in ["rc-0.1.0-rc.0","rc-0.1.0-rc.01",
                        "rc-0.1.0-rc.-1","v0.1.0","rc-0.1.0-rc.2x"] {
            assert!(rc_number(invalid).is_none());
        }
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
