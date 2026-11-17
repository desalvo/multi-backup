//! Oracle RMAN media adapter. The Oracle server/operator creates a genuine
//! RMAN backup set; this compiled Rust provider ingests ONLY validated .bkp
//! pieces into the age-encrypted chunk store and restores ONLY to a new path.
//! It does not claim to perform an RMAN restore, SCN fencing or database PITR.
use crate::chunk_store::{BackupKind,LogicalBackup,Source,Store};
use serde::Serialize;
use std::{collections::HashSet,fs,path::{Path,PathBuf}};

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
const LIMIT:usize=512;
const MAX_PIECE:u64=1_099_511_627_776;
#[derive(Serialize)]
pub struct Receipt {
    pub provider:&'static str,
    pub id:String,
    pub pieces:usize,
    pub bytes:u64,
    pub encrypted:bool,
    pub verified:bool,
    pub rman_restore_validate_certified:bool,
}
fn piece_name(name:&str)->bool{
    name.len()>4 && name.len()<=220 && name.ends_with(".bkp") &&
    !name.starts_with('.') && !name.contains("..") &&
    name.bytes().all(|b|b.is_ascii_alphanumeric() ||
        matches!(b,b'-'|b'_'|b'.'))
}
fn ordinary_directory(path:&Path)->Result<PathBuf>{
    if !path.is_absolute(){return Err("Oracle media path must be absolute".into());}
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir()||meta.file_type().is_symlink(){
        return Err("Oracle media requires an existing ordinary directory".into());
    }
    Ok(fs::canonicalize(path)?)
}
fn ordinary_identity(path:&Path)->Result<PathBuf>{
    if !path.is_absolute(){return Err("Oracle age identity must be absolute".into());}
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()||meta.file_type().is_symlink()||meta.len()==0{
        return Err("Oracle age identity must be an ordinary nonempty file".into());
    }
    Ok(fs::canonicalize(path)?)
}
fn require_pieces(source:&Path)->Result<(usize,u64)>{
    let mut seen=HashSet::new();
    let mut count=0usize;
    let mut bytes=0u64;
    for item in fs::read_dir(source)?{
        let item=item?;
        let filename=item.file_name();
        let name=filename.to_str().ok_or("Non-UTF8 Oracle RMAN piece name")?;
        if !piece_name(name)||!seen.insert(name.to_owned()){
            return Err("Oracle RMAN source contains a non-piece or repeated name".into());
        }
        let meta=fs::symlink_metadata(item.path())?;
        if !meta.file_type().is_file()||meta.file_type().is_symlink()||
           meta.len()==0||meta.len()>MAX_PIECE{
            return Err("Oracle RMAN source requires ordinary nonempty bounded pieces".into());
        }
        count+=1;
        if count>LIMIT{return Err("Oracle RMAN media count exceeds limit".into());}
        bytes=bytes.checked_add(meta.len()).ok_or("Oracle backup byte overflow")?;
    }
    if count==0{return Err("No authentic RMAN media candidates found".into());}
    Ok((count,bytes))
}
fn verify_manifest(snapshot:&LogicalBackup)->Result<(usize,u64)>{
    if snapshot.kind!=BackupKind::Full||snapshot.parent_id.is_some()||
       snapshot.top_namespace!="oracle" || snapshot.entries.is_empty() ||
       snapshot.entries.len()>LIMIT {
        return Err("Backup is not a standalone Oracle RMAN media set".into());
    }
    let mut bytes=0u64;
    let mut seen=HashSet::new();
    for entry in &snapshot.entries{
        let name=entry.path.strip_prefix("oracle/rman/")
            .ok_or("Backup contains non-Oracle data")?;
        if !piece_name(name) || !seen.insert(name.to_owned()) ||
           entry.bytes==0 || entry.bytes>MAX_PIECE || entry.chunks.is_empty(){
            return Err("Oracle backup manifest has unsafe or empty media".into());
        }
        bytes=bytes.checked_add(entry.bytes).ok_or("Oracle manifest byte overflow")?;
    }
    Ok((snapshot.entries.len(),bytes))
}
pub fn backup_pieces(source:&Path,root:&Path,identity:&Path,label:&str)->Result<Receipt>{
    if label.trim().is_empty()||label.len()>256{
        return Err("Oracle backup requires a bounded label".into());
    }
    let source=ordinary_directory(source)?;
    let root=ordinary_directory(root)?;
    let identity=ordinary_identity(identity)?;
    if source.starts_with(&root)||root.starts_with(&source) ||
       identity.starts_with(&source){
        return Err("Oracle media and encrypted store/key must not overlap".into());
    }
    let (source_count,source_bytes)=require_pieces(&source)?;
    let store=Store::open(&root,&identity)?;
    let backup=store.backup_in_namespace(&identity,label,"oracle",
        &[Source{namespace:"rman".into(),directory:source}])?;
    let (count,bytes)=verify_manifest(&backup)?;
    if count!=source_count||bytes!=source_bytes ||
       !store.verify(&identity,&backup.id)?.healthy {
        return Err("Oracle RMAN source changed or encrypted manifest verification failed".into());
    }
    Ok(Receipt{provider:"oracle",id:backup.id,pieces:count,bytes,
        encrypted:true,verified:true,rman_restore_validate_certified:false})
}
pub fn restore_pieces(root:&Path,identity:&Path,id:&str,dest:&Path)->Result<Receipt>{
    let root=ordinary_directory(root)?;
    let identity=ordinary_identity(identity)?;
    if !dest.is_absolute(){return Err("Oracle restore destination must be absolute".into());}
    if fs::symlink_metadata(dest).is_ok(){
        return Err("Oracle media restore must create a new destination".into());
    }
    let parent=ordinary_directory(dest.parent().ok_or("Oracle restore parent missing")?)?;
    let name=dest.file_name().ok_or("Oracle destination has no name")?;
    if name.to_str().is_none_or(|v|v=="."||v==".."||v.is_empty()){
        return Err("Unsafe Oracle restore destination".into());
    }
    let fresh=parent.join(name);
    if fresh.starts_with(&root)||root.starts_with(&fresh) ||
       identity.starts_with(&fresh){
        return Err("Oracle restore cannot overlap encrypted store or identity".into());
    }
    let store=Store::open(&root,&identity)?;
    if !store.verify(&identity,id)?.healthy{
        return Err("Oracle encrypted backup integrity check failed".into());
    }
    let snapshot=store.load(&identity,id)?;
    let (count,bytes)=verify_manifest(&snapshot)?;
    if store.restore(&identity,id,None,&fresh)?!=count{
        return Err("Oracle RMAN restore piece count mismatch".into());
    }
    Ok(Receipt{provider:"oracle",id:id.to_owned(),pieces:count,bytes,
        encrypted:true,verified:true,rman_restore_validate_certified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn strict_oracle_piece_names(){
        assert!(piece_name("pdb_6hj234_1_1.bkp"));
        for name in [".hidden.bkp","../evil.bkp","foo/evil.bkp",
                     "not-a-piece.zip","evil..bkp","", "A\n.bkp"]{
            assert!(!piece_name(name),"{name}");
        }
    }
    #[test]fn refuse_symlinks_or_extra_directory_data(){
        let dir=tempfile::tempdir().unwrap();
        assert!(require_pieces(dir.path()).is_err());
        fs::write(dir.path().join("real_1.bkp"),b"RMAN test media fixture").unwrap();
        assert_eq!(require_pieces(dir.path()).unwrap(),(1,23));
        fs::write(dir.path().join("notes.txt"),b"should not ingest").unwrap();
        assert!(require_pieces(dir.path()).is_err());
        fs::remove_file(dir.path().join("notes.txt")).unwrap();
        #[cfg(unix)]{
            std::os::unix::fs::symlink(
                dir.path().join("real_1.bkp"),dir.path().join("link.bkp")).unwrap();
            assert!(require_pieces(dir.path()).is_err());
        }
    }
    #[test]fn invalid_new_restore_destination_refused_before_opening_store(){
        let dir=tempfile::tempdir().unwrap();
        let exists=dir.path().join("already");
        fs::create_dir(&exists).unwrap();
        assert!(restore_pieces(dir.path(),Path::new("/not-existing-age"),"x",&exists).is_err());
    }
}
