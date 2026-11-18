//! Verified cloud portable ZIP64 recovery from seven approved rclone drives.
//! Ciphertext-only staging, age-authenticated catalog, SHA256 pinned volumes,
//! verified chunks/manifest, create-only selected plaintext restore.
use crate::{cloud_drive,portable,chunk_store::{Store,LogicalBackup}};
use serde_json::{json,Value};
use std::{fs,path::Path};
use tempfile::TempDir;
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
pub const DEFAULT_MAX_BYTES:u64=8*1024*1024*1024;
struct Ready {
    _stage:TempDir,store:Store,backup:LogicalBackup,transferred:u64,volumes:usize,
    index_sha256:String,
}
fn ordinary_directory(path:&Path)->Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir(){
        return Err("Recovery workspace must be an ordinary directory".into());
    }
    Ok(())
}
fn prepare(config:&Path,provider:&str,directory:&str,identity:&Path,
           workspace:&Path,max_bytes:u64)->Result<Ready>{
    cloud_drive::valid_path(directory,false)?;
    ordinary_directory(workspace)?;
    if !fs::symlink_metadata(identity)?.file_type().is_file(){
        return Err("Age identity must be a regular file".into());
    }
    if max_bytes==0||max_bytes>32*1024*1024*1024 {
        return Err("Recovery budget must be 1 byte..32 GiB".into());
    }
    let stage=tempfile::Builder::new().prefix(".mb-cloud-").tempdir_in(workspace)?;
    let pkg=stage.path().join("bundle");
    fs::create_dir(&pkg)?;
    let index_path=pkg.join("bundle.index.age");
    let index_remote=format!("{directory}/bundle.index.age");
    let (index_size,index_sha256)=cloud_drive::download_to_new(config,provider,
        &index_remote,&index_path,8*1024*1024)?;
    let index=portable::inventory(&fs::read(&index_path)?,identity)?;
    // Account for BOTH authenticated index reads in the strict byte budget.
    let mut total=index_size.checked_mul(2).ok_or("Recovery index budget overflow")?;
    if total>max_bytes{return Err("Cloud recovery budget below catalog read cost".into());}
    for volume in &index.volumes {
        total=total.checked_add(volume.size).ok_or("Recovery byte overflow")?;
        if total>max_bytes{return Err("Cloud backup exceeds transfer budget".into());}
    }
    for volume in &index.volumes {
        let (bytes,sha)=cloud_drive::download_to_new(config,provider,
            &format!("{directory}/{}",volume.name),
            &pkg.join(&volume.name),volume.size)?;
        if bytes!=volume.size||sha!=volume.sha256 {
            return Err("Remote encrypted ZIP volume changed or failed SHA256".into());
        }
    }
    // Recheck the complete remote ciphertext index after all volume downloads.
    // This catches a concurrent catalog replacement but is NOT atomic
    // version fencing. For true atomicity, cloud provider conditional reads
    // (e.g. If-Match + version ID) will still be required.
    let recheck_path=stage.path().join("index-postflight.age");
    let (recheck_size,recheck_sha)=cloud_drive::download_to_new(
        config,provider,&index_remote,&recheck_path,index_size)?;
    if recheck_size!=index_size || recheck_sha!=index_sha256 {
        return Err("Remote catalog changed during encrypted volume transfer".into());
    }
    let store_path=stage.path().join("authenticated-store");
    let id=portable::import(&pkg,identity,&store_path)?;
    if id!=index.backup_id{return Err("Cloud backup ID mismatch".into());}
    let store=Store::open(&store_path,identity)?;
    if !store.verify(identity,&id)?.healthy{
        return Err("Cloud backup failed authenticated chunk/parity verification".into());
    }
    let backup=store.load(identity,&id)?;
    Ok(Ready{_stage:stage,store,backup,transferred:total,
             volumes:index.volumes.len(),index_sha256})
}
fn report(ready:&Ready,provider:&str,directory:&str)->Value{
    json!({"provider":provider,"remote_directory":directory,
           "backup_id":ready.backup.id,"label":ready.backup.label,
           "files":ready.backup.entries.iter().map(|e|
               json!({"path":e.path,"size":e.bytes})).collect::<Vec<_>>(),
           "volumes":ready.volumes,"downloaded_bytes":ready.transferred,
           "age_authenticated":true,"sha256_verified":true,
           "remote_index_revalidated":true,
           "remote_index_sha256":ready.index_sha256,
           "remote_version_atomic":false})
}
/// Inspect every authenticated logical file, no plaintext restore.
pub fn inspect(config:&Path,provider:&str,directory:&str,identity:&Path,
               workspace:&Path,max_bytes:u64)->Result<Value>{
    let ready=prepare(config,provider,directory,identity,workspace,max_bytes)?;
    Ok(report(&ready,provider,directory))
}
/// Restore selected paths/folders into a new local directory after full verify.
pub fn restore(config:&Path,provider:&str,directory:&str,identity:&Path,
               workspace:&Path,destination:&Path,selections:&[String],
               max_bytes:u64)->Result<Value>{
    ordinary_directory(workspace)?;
    ordinary_directory(destination.parent().ok_or("Restore parent missing")?)?;
    if fs::symlink_metadata(destination).is_ok(){
        return Err("Refusing to overwrite an existing restore target".into());
    }
    let ready=prepare(config,provider,directory,identity,workspace,max_bytes)?;
    let count=ready.store.restore_many(identity,&ready.backup.id,selections,destination)?;
    let mut response=report(&ready,provider,directory);
    response["restored_files"]=json!(count);
    response["new_restore_directory"]=json!(destination.to_string_lossy().to_string());
    response["overwrote_existing"]=json!(false);
    Ok(response)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symlink_workspace_and_unsafe_cloud_paths_fail_closed(){
        let t=tempfile::tempdir().unwrap();
        assert!(ordinary_directory(t.path()).is_ok());
        for p in ["../bundle","X/../../bundle","-flag"]{
            assert!(cloud_drive::valid_path(p,false).is_err());
        }
        #[cfg(unix)] {
            std::os::unix::fs::symlink(t.path(),t.path().join("alias")).unwrap();
            assert!(ordinary_directory(&t.path().join("alias")).is_err());
        }
    }
}
