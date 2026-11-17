//! Native B-Out encrypted-archive transport to *existing exact* mounted
//! NFS/SMB/WebDAV/CephFS/GlusterFS/Lustre/GPFS-CES/SSHFS endpoints.
//! This does NOT mount remote storage, grant a WORM guarantee or overwrite.
use crate::{archive,mount_probe,plugins::{self,GpfsAccess,Mode}};
use serde::Serialize;
use std::{fs,path::{Path,PathBuf}};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

const MOUNTED: &[&str]=&[
    "nfs","smb","webdav","cephfs","glusterfs","lustre","gpfs","sshfs"
];

#[derive(Debug,Clone,Serialize)]
pub struct ArchiveReceipt {
    pub provider:String,
    pub archive:String,
    pub encrypted:bool,
    pub create_only:bool,
    pub source_members:usize,
    pub verified_members:usize,
    pub restore_input_supported:bool,
}
fn validate_filename(name:&str)->Result<()> {
    if !name.ends_with(".tar.gz.age") || name.len()<=(".tar.gz.age".len()) ||
       name.len()>200 || !name.is_ascii() ||
       !name.bytes().all(|b|b.is_ascii_alphanumeric()||
                            matches!(b,b'.'|b'-'|b'_')) ||
       name.starts_with('.') || name.contains("..") {
        return Err("Mounted output requires a flat safe *.tar.gz.age filename".into());
    }
    Ok(())
}
fn exact_mount(plugin:&str, root:&Path,access:Option<GpfsAccess>,write:bool)->Result<PathBuf>{
    if !MOUNTED.contains(&plugin) {return Err("No mounted-output adapter for this provider".into());}
    let meta=fs::symlink_metadata(root)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("A real existing exact mountpoint is required".into());
    }
    let evidence=mount_probe::probe(plugin,root,access)?;
    if write && !evidence.rw_mount_option {
        return Err("Encrypted B-Out destination must be a writable exact mountpoint".into());
    }
    Ok(fs::canonicalize(root)?)
}
/// One full encrypted backup archive into the exact writable mountpoint.
/// Age encryption finishes and SHA256 sidecar publishes before an authenticated
/// readback decrypt+tar verification. Root writer is never given file deletion.
pub fn backup_to_mount(plugin:&str,source:&Path,root:&Path,
    name:&str,recipient:&str,identity:&Path,access:Option<GpfsAccess>)
    ->Result<ArchiveReceipt> {
    plugins::plan(Mode::Backup,"posix",plugin)?;
    validate_filename(name)?;
    let destroot=exact_mount(plugin,root,access,true)?;
    let srctype=fs::symlink_metadata(source)?;
    if !srctype.file_type().is_dir()||srctype.file_type().is_symlink(){
        return Err("Mounted B-Out requires an ordinary local source directory".into());
    }
    let source=fs::canonicalize(source)?;
    if source.starts_with(&destroot)||destroot.starts_with(&source) {
        return Err("Source and remote encrypted mount must not overlap".into());
    }
    let key=fs::symlink_metadata(identity)?;
    if !key.file_type().is_file()||key.file_type().is_symlink(){
        return Err("Age private identity must be an ordinary file".into());
    }
    if fs::canonicalize(identity)?.starts_with(&source) {
        return Err("Age private key must not occur under input source".into());
    }
    let archive_path=destroot.join(name);
    let sidecar=PathBuf::from(format!("{}.sha256",archive_path.display()));
    if fs::symlink_metadata(&archive_path).is_ok() ||
       fs::symlink_metadata(&sidecar).is_ok(){
        return Err("Mounted encrypted output already exists; no overwrite".into());
    }
    let created=archive::create(&source,&archive_path,recipient,"multi-backup/mounted-output")?;
    // The remote filesystem must return the ciphertext and attest the
    // decryptable member count; a successful write syscall is not enough.
    let verified=archive::verify(&archive_path,identity)?;
    if verified!=created {
        return Err("Mounted target encrypted readback or member count mismatched".into());
    }
    Ok(ArchiveReceipt{provider:plugin.into(),
        archive:archive_path.display().to_string(),encrypted:true,
        create_only:true,source_members:created,verified_members:verified,
        restore_input_supported:true})
}
pub fn restore_from_mount(plugin:&str,root:&Path,name:&str,
    identity:&Path,destination:&Path,access:Option<GpfsAccess>)->Result<usize>{
    plugins::plan(Mode::Restore,plugin,"posix")?;
    validate_filename(name)?;
    let mount=exact_mount(plugin,root,access,false)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err("Mounted recovery must create a new nonexisting destination".into());
    }
    let target=mount.join(name);
    let sidecar=PathBuf::from(format!("{}.sha256",target.display()));
    let side=fs::symlink_metadata(sidecar)?;
    if !side.file_type().is_file()||side.file_type().is_symlink(){
        return Err("Mounted recovery requires a verified SHA256 sidecar".into());
    }
    let files=archive::verify(&target,identity)?;
    let restored=archive::extract(&target,identity,destination)?;
    if files!=restored.len() {
        return Err("Mounted recovered member count does not match verified inventory".into());
    }
    Ok(files)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn mounted_name_denies_path_injection_and_overwrite_like_names(){
        for bad in ["","../x.tar.gz.age","/x.tar.gz.age","x/y.tar.gz.age",
            "x\\y.tar.gz.age","a..b.tar.gz.age",".hidden.tar.gz.age","x.tar",
            "a\n.tar.gz.age"]{assert!(validate_filename(bad).is_err(),"{bad}");}
        assert!(validate_filename("backup-2026_11_17.tar.gz.age").is_ok());
    }
    #[test] fn plain_local_directory_cannot_impersonate_any_remote_mount(){
        let tmp=tempfile::tempdir().unwrap();
        for plugin in MOUNTED {
            let mode=if *plugin=="gpfs"{Some(GpfsAccess::CesNfs)}else{None};
            assert!(exact_mount(plugin,tmp.path(),mode,true).is_err());
            assert!(exact_mount(plugin,tmp.path(),mode,false).is_err());
        }
        assert!(exact_mount("posix",tmp.path(),None,true).is_err());
    }
    #[test] fn fake_mounted_archive_does_not_write_any_backup(){
        let d=tempfile::tempdir().unwrap();
        let input=d.path().join("input");
        let output=d.path().join("not-actually-nfs");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&output).unwrap();
        let key=d.path().join("key.age");
        fs::write(&key,b"not-needed-to-reject").unwrap();
        for plugin in ["nfs","smb","webdav","cephfs","glusterfs","lustre","gpfs","sshfs"]{
            let mode=if plugin=="gpfs"{Some(GpfsAccess::CesSmb)}else{None};
            assert!(backup_to_mount(plugin,&input,&output,"safe.tar.gz.age",
                "age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",&key,mode).is_err());
        }
        assert_eq!(fs::read_dir(&output).unwrap().count(),0);
    }
}
