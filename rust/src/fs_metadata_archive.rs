//! POSIX ACL/xattr-preserving filesystem snapshot backup/restore.
//! Streams a PAX archive from a genuine RO mount via libarchive's bsdtar
//! into authenticated age chunk storage. No plaintext staging is written.
//! Requires an externally consistent snapshot; never claims a live RW walk
//! is atomic. Restore is create-only on an exact RW mount.
use crate::{chunk_store::{BackupKind,Store},mount_probe};
use serde::Serialize;
use std::{fs,io::Write,path::Path,process::{Command,Stdio}};
use walkdir::WalkDir;

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Serialize)]
pub struct Receipt{
    pub provider:String,pub backup_id:String,pub archive_bytes:u64,
    pub encrypted:bool,pub restores_acl_xattr:bool,
    pub snapshot_consistency_attested:bool
}
fn supported(provider:&str)->bool{
    matches!(provider,"cephfs"|"lustre"|"nfs"|"smb"|"glusterfs"|"sshfs"|"gpfs")
}
fn ordinary_dir(path:&Path)->Result<std::path::PathBuf>{
    if !path.is_absolute(){return Err("Metadata snapshot directory must be absolute".into());}
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir()||meta.file_type().is_symlink(){
        return Err("Metadata snapshot requires existing ordinary directory".into());
    }
    Ok(fs::canonicalize(path)?)
}
fn require_mount(provider:&str,mount:&Path,rw:bool)->Result<()>{
    if !supported(provider){return Err("Unsupported metadata snapshot filesystem".into());}
    // All requested metadata backups must be from an actual mounted source,
    // never an arbitrary local directory that pretends to be CephFS.
    if provider=="gpfs" {
        return Err("GPFS requires separately attested CES protocol; unavailable in this adapter".into());
    }
    let status=mount_probe::probe(provider,mount,None)?;
    if status.rw_mount_option!=rw{
        return Err("Metadata archive requires exact read-only snapshot or RW new-target mount".into());
    }
    Ok(())
}
pub fn backup(provider:&str,source:&Path,store_root:&Path,key:&Path,
              label:&str,snapshot_attested:bool)->Result<Receipt>{
    if !snapshot_attested{return Err("Metadata snapshot must be independently attested".into());}
    if label.is_empty()||label.len()>256{return Err("Invalid metadata backup label".into());}
    let source=ordinary_dir(source)?;
    let store_root=ordinary_dir(store_root)?;
    require_mount(provider,&source,false)?;
    // Preserve ACL/xattr on ordinary files and dirs only; never let a
    // mutable link/special inode reach the external archive extractor.
    // Immutable native snapshot attestation is the caller's responsibility.
    let mut seen=0usize;
    for entry in WalkDir::new(&source).follow_links(false).max_depth(64) {
        let entry=entry?;
        let kind=entry.file_type();
        if !kind.is_dir() && !kind.is_file(){
            return Err("ACL/xattr snapshot contains symlink or special inode".into());
        }
        seen+=1;
        if seen>100_000{return Err("ACL/xattr snapshot item count exceeds safe limit".into());}
    }
    if source.starts_with(&store_root)||store_root.starts_with(&source)||
       key.starts_with(&source){
        return Err("Metadata source and encrypted store/key overlap".into());
    }
    let store=Store::open(&store_root,key)?;
    let mut child=Command::new("bsdtar")
        .args(["--acls","--xattrs","--fflags","--numeric-owner","--one-file-system",
               "-cf","-","-C"]).arg(&source).arg(".")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut input=child.stdout.take().ok_or("bsdtar archive pipe missing")?;
    let logical=format!("filesystem-metadata/{provider}/snapshot.tar");
    let result=store.backup_stream(key,label,"",&logical,&mut input,
        BackupKind::Full,None,||{
            if child.wait().map_err(|_|"Metadata archiver wait failed")?.success(){
                Ok(())
            }else{
                Err("ACL/xattr archive process failed; manifest not committed".into())
            }
        });
    if result.is_err(){let _=child.kill();let _=child.wait();}
    let snap=result?;
    if !store.verify(key,&snap.id)?.healthy{
        return Err("Encrypted metadata snapshot failed verification".into());
    }
    let bytes=snap.entries[0].bytes;
    Ok(Receipt{provider:provider.into(),backup_id:snap.id,
        archive_bytes:bytes,encrypted:true,
        restores_acl_xattr:true,snapshot_consistency_attested:true})
}
// On filesystems whose FUSE server does not support renameat2
// RENAME_NOREPLACE, the reserved NEW directory must still retain the PAX
// root-level ACL/xattr attributes (not only its descendant files).
#[cfg(target_os="linux")]
fn copy_root_xattrs(from:&Path,to:&Path)->Result<()>{
    use std::{ffi::CString,os::unix::ffi::OsStrExt,ptr};
    let a=CString::new(from.as_os_str().as_bytes())?;
    let b=CString::new(to.as_os_str().as_bytes())?;
    let size=unsafe{libc::llistxattr(a.as_ptr(),ptr::null_mut(),0)};
    if size<0{return Err(format!("Cannot enumerate PAX root ACL/xattrs: {}",
                               std::io::Error::last_os_error()).into());}
    if size>1_048_576{return Err("PAX root xattr name list exceeds limit".into());}
    let mut names=vec![0u8;size as usize];
    if size==0{return Ok(());}
    let n=unsafe{libc::llistxattr(a.as_ptr(),names.as_mut_ptr().cast(),names.len())};
    if n!=size{return Err("PAX root xattr name inventory changed".into());}
    for part in names.split(|&x|x==0).filter(|x|!x.is_empty()){
        let name=CString::new(part)?;
        let bytes=unsafe{libc::lgetxattr(a.as_ptr(),name.as_ptr(),ptr::null_mut(),0)};
        if bytes<0||bytes>16_777_216{
            return Err("PAX root xattr value missing or exceeds bound".into());
        }
        let mut value=vec![0u8;bytes as usize];
        let got=unsafe{libc::lgetxattr(a.as_ptr(),name.as_ptr(),
                         value.as_mut_ptr().cast(),value.len())};
        if got!=bytes{return Err("PAX root xattr value changed".into());}
        if unsafe{libc::lsetxattr(b.as_ptr(),name.as_ptr(),
                     value.as_ptr().cast(),value.len(),0)}!=0{
            return Err(format!("PAX root ACL/xattr publication failed: {}",
                               std::io::Error::last_os_error()).into());
        }
    }
    Ok(())
}
// Publishing a new directory MUST be atomic no-replace, not POSIX rename
// (which silently replaces an existing empty directory after a race).
#[cfg(target_os="linux")]
fn publish_new(source:&Path,target:&Path)->Result<()>{
    use std::{ffi::CString,os::unix::ffi::OsStrExt};
    let from=CString::new(source.as_os_str().as_bytes())?;
    let to=CString::new(target.as_os_str().as_bytes())?;
    let status=unsafe{libc::syscall(libc::SYS_renameat2,libc::AT_FDCWD,
        from.as_ptr(),libc::AT_FDCWD,to.as_ptr(),libc::RENAME_NOREPLACE)};
    if status==0{return Ok(());}
    let error=std::io::Error::last_os_error();
    if !matches!(error.raw_os_error(),Some(libc::EINVAL)|Some(libc::EOPNOTSUPP)){
        return Err(format!("Atomic no-replace restore publication refused: {error}").into());
    }
    // Native Ceph FUSE may reject RENAME_NOREPLACE with EINVAL. Fall back
    // only to atomic create_new DIRECTORY reservation (mkdir fails EEXIST).
    // Never issue an unrestricted rename onto an externally existing path.
    // A 0700 reserved destination is populated only from our verified,
    // privately staged PAX directory, then unsealed after all moves.
    use std::os::unix::fs::{DirBuilderExt,PermissionsExt};
    let parent=target.parent().ok_or("Restore target parent missing")?;
    let pm=fs::metadata(parent)?;
    if pm.permissions().mode()&0o022!=0{
        return Err("CephFS create-only fallback requires non-group/world-writable parent".into());
    }
    let reserved=std::fs::DirBuilder::new().mode(0o700).create(target);
    reserved.map_err(|e|format!("New restore target mkdir reservation refused: {e}"))?;
    let publication=(||->Result<()>{
        for item in fs::read_dir(source)?{
            let item=item?;
            // The destination directory was just created and is private;
            // no previously existing user file can be replaced.
            fs::rename(item.path(),target.join(item.file_name()))?;
        }
        fs::set_permissions(target,fs::metadata(source)?.permissions())?;
        copy_root_xattrs(source,target)?;
        Ok(())
    })();
    if let Err(e)=publication{
        // Failed create-only publication remains explicitly incomplete:
        // never remove it while an external observer may be inspecting it.
        let _=fs::set_permissions(target,fs::Permissions::from_mode(0o700));
        return Err(format!("Incomplete NEW restore reservation (do not treat as complete): {e}").into());
    }
    // Empty tempdir is automatically cleaned by tempfile.
    Ok(())
}
#[cfg(not(target_os="linux"))]
fn publish_new(_source:&Path,_target:&Path)->Result<()>{
    Err("Atomic metadata directory publication requires Linux renameat2".into())
}
pub fn restore_new(provider:&str,root:&Path,key:&Path,id:&str,destination:&Path)
                  ->Result<Receipt>{
    if !destination.is_absolute()||fs::symlink_metadata(destination).is_ok(){
        return Err("Metadata restore requires a NEW absolute destination".into());
    }
    let parent=ordinary_dir(destination.parent().ok_or("Metadata restore parent missing")?)?;
    let name=destination.file_name().ok_or("Metadata restore directory name missing")?;
    if name=="."||name==".."{return Err("Unsafe metadata restore name".into());}
    require_mount(provider,&parent,true)?;
    let root=ordinary_dir(root)?;
    if parent.starts_with(&root)||root.starts_with(&parent) {
        return Err("Metadata restore overlaps encrypted store".into());
    }
    let store=Store::open(&root,key)?;
    if !store.verify(key,id)?.healthy{
        return Err("Metadata archive cryptographic integrity failure".into());
    }
    let snap=store.load(key,id)?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||
       !snap.top_namespace.is_empty()||snap.entries.len()!=1||
       snap.entries[0].path!=format!("filesystem-metadata/{provider}/snapshot.tar"){
        return Err("Backup does not contain an authenticated filesystem metadata archive".into());
    }
    // Work in a new private directory on the SAME destination mount; no
    // externally visible partial restore or overwrite on error.
    let stage=tempfile::Builder::new().prefix(".mb-fsmeta-").tempdir_in(&parent)?;
    let mut process=Command::new("bsdtar")
        .args(["--safe-writes","--acls","--xattrs","--fflags","--numeric-owner",
               "--same-owner","-xpf","-","-C"])
        .arg(stage.path()).stdin(Stdio::piped())
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let mut writer=process.stdin.take().ok_or("Metadata extraction pipe missing")?;
    let entry=&snap.entries[0];
    let copied=(||->Result<u64>{
        let mut count=0u64;
        for chunk in &entry.chunks{
            let bytes=store.read_chunk_ref(key,chunk)?;
            writer.write_all(&bytes)?;
            count=count.checked_add(bytes.len() as u64).ok_or("Metadata archive overflow")?;
        }
        writer.flush()?;drop(writer);
        if count!=entry.bytes{return Err("Metadata archive length mismatch".into());}
        Ok(count)
    })();
    if copied.is_err(){let _=process.kill();let _=process.wait();}
    let bytes=copied?;
    if !process.wait()?.success(){
        return Err("POSIX metadata restore failed; nothing published".into());
    }
    let dest=parent.join(name);
    if fs::symlink_metadata(&dest).is_ok(){return Err("Concurrent restore destination exists".into());}
    publish_new(stage.path(),&dest)?;
    Ok(Receipt{provider:provider.into(),backup_id:id.into(),archive_bytes:bytes,
        encrypted:true,restores_acl_xattr:true,snapshot_consistency_attested:true})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn no_fake_mount_or_unattested_backup(){
        assert!(!supported("posix"));assert!(!supported("rbd"));
        assert!(supported("cephfs"));assert!(supported("lustre"));
        assert!(backup("cephfs",Path::new("/tmp"),Path::new("/tmp"),
            Path::new("/key"),"x",false).is_err());
    }
    #[test]fn cannot_overwrite_restore_target(){
        let d=tempfile::tempdir().unwrap();
        assert!(restore_new("cephfs",d.path(),Path::new("/bad"),
            "bad",d.path()).is_err());
    }
}
