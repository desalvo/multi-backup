//! Append-only encrypted archive repository preview.
//! This does NOT itself enforce WORM: ransomware resistance requires an
//! independent remote append-only/WORM storage authority and credential boundary.
//! No mutation or deletion methods are deliberately exposed.
use crate::archive;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}};
use tempfile::NamedTempFile;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const POLICY: &str = ".multi-backup-immutable-vault-v1.json";
const SNAPSHOTS: &str = "snapshots";
#[derive(Serialize, Deserialize)]
struct Policy { schema: u32, recipient: String, enforcement: String }
#[derive(Debug, Serialize)]
pub struct VaultRecord {
    pub id: String, pub encrypted_bytes: u64, pub sha256: String,
    pub files: Option<usize>, pub external_worm_verified: bool,
}

fn acceptable_recipient(s: &str) -> bool {
    s.starts_with("age1") && s.len() >= 24 && s.len() <= 128 &&
        s.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}
fn root_directory(path: &Path) -> Result<PathBuf> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
        return Err("Vault must be an existing genuine directory (never a symlink)".into());
    }
    Ok(fs::canonicalize(path)?)
}
fn id_valid(id: &str) -> bool {
    id.len()==32 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn policy(root: &Path) -> Result<Policy> {
    let root=root_directory(root)?;
    let file=root.join(POLICY);
    let meta=fs::symlink_metadata(&file)?;
    if !meta.file_type().is_file() || meta.len()>4096 {
        return Err("Vault policy absent or unsafe".into());
    }
    let data:Policy=serde_json::from_reader(File::open(file)?)?;
    if data.schema!=1 || !acceptable_recipient(&data.recipient) ||
       data.enforcement!="external-worm-not-verified" {
        return Err("Unsupported or unsafe vault policy".into());
    }
    Ok(data)
}
pub fn init(root:&Path,recipient:&str) -> Result<()> {
    let root=root_directory(root)?;
    if !acceptable_recipient(recipient){return Err("Invalid age recipient".into());}
    let item=root.join(POLICY);
    let mut file=OpenOptions::new().write(true).create_new(true).open(item)?;
    let content=Policy{
        schema:1,recipient:recipient.to_string(),
        enforcement:"external-worm-not-verified".to_string()
    };
    serde_json::to_writer(&mut file,&content)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::create_dir(root.join(SNAPSHOTS))?;
    Ok(())
}
fn snapshot_file(root:&Path,id:&str) -> Result<PathBuf> {
    if !id_valid(id){return Err("Invalid vault snapshot identifier".into());}
    Ok(root.join(SNAPSHOTS).join(format!("{id}.tar.gz.age")))
}
fn safe_snapshot_dir(root:&Path) -> Result<PathBuf> {
    let dir=root.join(SNAPSHOTS);
    let meta=fs::symlink_metadata(&dir)?;
    if !meta.file_type().is_dir() || meta.file_type().is_symlink(){
        return Err("Invalid vault snapshots directory".into());
    }
    Ok(fs::canonicalize(dir)?)
}
fn digest(path:&Path) -> Result<(String,u64)> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() {return Err("Unsafe vaulted archive".into());}
    let mut stream=File::open(path)?;
    let mut sha=Sha256::new();
    let mut buf=[0u8;131072];
    let mut bytes=0u64;
    loop {
        let n=stream.read(&mut buf)?;
        if n==0{break;}
        bytes+=n as u64;
        sha.update(&buf[..n]);
    }
    Ok((format!("{:x}",sha.finalize()),bytes))
}
fn verify_sidecar(path:&Path)->Result<(String,u64)> {
    let (hash,bytes)=digest(path)?;
    let name=path.file_name().ok_or("Invalid snapshot filename")?.to_string_lossy();
    let sidecar=PathBuf::from(format!("{}.sha256",path.display()));
    let metadata=fs::symlink_metadata(&sidecar)?;
    if !metadata.file_type().is_file() || metadata.len()>256 {
        return Err("Unsafe or absent vault checksum".into());
    }
    let expected=fs::read_to_string(sidecar)?;
    if expected.trim_end()!=format!("{hash}  {name}"){
        return Err("Vault ciphertext checksum failed".into());
    }
    Ok((hash,bytes))
}
pub fn status(root:&Path)->Result<serde_json::Value>{
    let cfg=policy(root)?;
    let _=safe_snapshot_dir(&root_directory(root)?)?;
    Ok(serde_json::json!({
        "configured":true,"schema":cfg.schema,"recipient":cfg.recipient,
        "external_worm_verified":false,
        "protection":"append-only application API preview; external WORM NOT VERIFIED",
        "delete_supported":false, "overwrite_supported":false
    }))
}
pub fn backup(root:&Path,source:&Path)->Result<VaultRecord>{
    let root=root_directory(root)?;
    let cfg=policy(&root)?;
    let snapshots=safe_snapshot_dir(&root)?;
    let source_root=root_directory(source)?;
    if source_root.starts_with(&root)||root.starts_with(&source_root){
        return Err("Vault and input directory cannot overlap".into());
    }
    let mut random=[0u8;16];
    OsRng.fill_bytes(&mut random);
    let id=hex::encode(random);
    let dest=snapshot_file(&snapshots,&id)?;
    let member_count=archive::create(&source_root,&dest,&cfg.recipient,
        &format!("multi-backup/vault/{id}"))?;
    let (sha,bytes)=verify_sidecar(&dest)?;
    Ok(VaultRecord{id,sha256:sha,encrypted_bytes:bytes,files:Some(member_count),
        external_worm_verified:false})
}
pub fn verify(root:&Path,id:&str,identity:&Path)->Result<VaultRecord>{
    let root=root_directory(root)?;
    let _=policy(&root)?;
    let snapshots=safe_snapshot_dir(&root)?;
    let path=snapshot_file(&snapshots,id)?;
    let (sha,bytes)=verify_sidecar(&path)?;
    let files=archive::verify(&path,identity)?;
    Ok(VaultRecord{id:id.to_string(),sha256:sha,encrypted_bytes:bytes,files:Some(files),
        external_worm_verified:false})
}
pub fn restore(root:&Path,id:&str,identity:&Path,destination:&Path)->Result<VaultRecord>{
    let record=verify(root,id,identity)?;
    let snapshots=safe_snapshot_dir(&root_directory(root)?)?;
    let archive_file=snapshot_file(&snapshots,id)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err("Vault restore destination already exists".into());
    }
    let dest_parent=destination.parent().ok_or("Vault restore needs an explicit destination parent")?;
    let dest_parent=root_directory(dest_parent)?;
    if dest_parent.starts_with(&root_directory(root)?) {
        return Err("Vault restore must not write inside the protected vault".into());
    }
    archive::extract(&archive_file,identity,destination)?;
    Ok(record)
}
pub fn list(root:&Path)->Result<Vec<VaultRecord>>{
    let root=root_directory(root)?;
    let _=policy(&root)?;
    let snapshots=safe_snapshot_dir(&root)?;
    let mut out=Vec::new();
    for entry in fs::read_dir(snapshots)? {
        let entry=entry?;
        let name=entry.file_name().to_string_lossy().into_owned();
        let Some(id)=name.strip_suffix(".tar.gz.age") else {continue;};
        if !id_valid(id){return Err("Unexpected vault archive filename".into());}
        let (sha,bytes)=verify_sidecar(&entry.path())?;
        out.push(VaultRecord{id:id.to_owned(),encrypted_bytes:bytes,sha256:sha,
            files:None,external_worm_verified:false});
    }
    out.sort_by(|a,b|a.id.cmp(&b.id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_symlink_root_existing_policy_and_bad_ids(){
        let dir=tempfile::tempdir().unwrap();
        assert!(init(dir.path(),"invalid").is_err());
        assert!(!id_valid("../evil"));
        assert!(!id_valid("ABCDEF0123456789ABCDEF0123456789"));
        let recipient="age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
        init(dir.path(),recipient).unwrap();
        assert!(init(dir.path(),recipient).is_err());
        assert!(list(dir.path()).unwrap().is_empty());
        assert_eq!(status(dir.path()).unwrap()["external_worm_verified"],false);
        #[cfg(unix)]{
            let link=dir.path().join("alias");
            std::os::unix::fs::symlink(dir.path(),&link).unwrap();
            assert!(status(&link).is_err());
        }
    }
}
