//! POSIX ACL/xattr-preserving filesystem snapshot backup/restore.
//! Streams a PAX archive from a genuine RO mount via libarchive's bsdtar
//! into authenticated age chunk storage. No plaintext staging is written.
//! Requires an externally consistent snapshot; never claims a live RW walk
//! is atomic. Restore is create-only on an exact RW mount.
use crate::{chunk_store::{BackupKind,Store},mount_probe};
use serde::Serialize;
use std::{fs,io::{Read,Write},path::Path,process::{Command,Stdio}};

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
    fs::rename(stage.path(),dest)?;
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
