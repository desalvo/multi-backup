//! Native PVE vzdump media B-In and offline restore. NOT VM/CT restore.
//! Requires real remote TLS PVE API task+media evidence, real local pvesm
//! storage resolution on the correct node, and independent size/SHA256.
use crate::{chunk_store::{BackupKind,LogicalBackup,Store},virtualization_inventory as pve};
use serde::Serialize;
use sha2::{Digest,Sha256};
use std::{cell::RefCell,fs,io::{self,Read},path::{Path,PathBuf},
          process::{Command,Stdio},rc::Rc};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub id:String,pub guest_kind:String,
    pub vmid:u32,pub archive:String,pub bytes:u64,pub encrypted:bool,
    pub restore_new_guest_certified:bool
}
fn safe(s:&str)->bool{
    !s.is_empty()&&s.len()<=220&&!s.contains("..")&&!s.starts_with('.')&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'-'|b'_'))
}
fn archive(kind:&str,id:u32,name:&str)->bool{
    safe(name)&&name.starts_with(&format!("vzdump-{kind}-{id}-"))&&
    ((kind=="qemu"&&name.ends_with(".vma.zst"))||
     (kind=="lxc"&&name.ends_with(".tar.zst")))
}
fn command(args:&[&str])->Result<String>{
    let output=Command::new(args[0]).args(&args[1..])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !output.status.success()||output.stdout.len()>16384{
        return Err("PVE host/storage identity command failed".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn local_path(node:&str,volid:&str,kind:&str,vmid:u32)->Result<PathBuf>{
    if !safe(node)||command(&["hostname","-s"])?.trim()!=node{
        return Err("PVE media adapter must run on the selected node itself".into());
    }
    let resolved=command(&["pvesm","path",volid])?;
    let path=Path::new(resolved.trim());
    if !path.is_absolute()||!archive(kind,vmid,
        path.file_name().and_then(|s|s.to_str()).ok_or("Invalid PVE archive name")?){
        return Err("PVE resolver did not return the requested genuine vzdump archive".into());
    }
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()||meta.file_type().is_symlink()||meta.len()==0{
        return Err("PVE backup media must be an ordinary nonempty file".into());
    }
    Ok(path.into())
}
struct Check{hash:Sha256,count:u64}
struct Reader<R:Read>{source:R,check:Rc<RefCell<Check>>,limit:u64}
impl<R:Read> Read for Reader<R>{
    fn read(&mut self,buf:&mut [u8])->io::Result<usize>{
        let n=self.source.read(buf)?;
        let mut proof=self.check.borrow_mut();
        proof.count=proof.count.checked_add(n as u64)
            .ok_or_else(||io::Error::other("PVE media overflow"))?;
        if proof.count>self.limit{return Err(io::Error::other("PVE media exceeds expected size"));}
        proof.hash.update(&buf[..n]);Ok(n)
    }
}
#[allow(clippy::too_many_arguments)]
pub fn backup(cfg:&pve::Config,node:&str,storage:&str,kind:&str,vmid:u32,
    upid:&str,volid:&str,sha:&str,bytes:u64,root:&Path,key:&Path,label:&str)
    ->Result<Receipt>{
    let prefix=format!("{storage}:backup/");
    if !safe(node)||!safe(storage)||!volid.starts_with(&prefix)||
       !archive(kind,vmid,volid.strip_prefix(&prefix).ok_or("Unsafe volid")?)||
       sha.len()!=64||!sha.bytes().all(|c|c.is_ascii_digit()||matches!(c,b'a'..=b'f'))||
       !(100..=999_999_999).contains(&vmid)||bytes==0||bytes>1_099_511_627_776||
       label.is_empty()||label.len()>256{
        return Err("PVE source requires exact guest, volume, size and SHA256".into());
    }
    let status=pve::pve_backup_task_status(cfg,node,upid)?;
    if !status.finished||!status.remote_task_ok{
        return Err("PVE vzdump task did not finish with OK".into());
    }
    let media=pve::pve_backup_media(cfg,node,storage,vmid)?;
    if !media.media.iter().any(|m|m.volid==volid&&m.size_bytes==Some(bytes)&&
       m.guest_kind.as_deref().is_none_or(|v|v==kind)){
        return Err("PVE API backup media inventory did not corroborate the snapshot".into());
    }
    let path=local_path(node,volid,kind,vmid)?;
    let before=fs::metadata(&path)?;
    if before.len()!=bytes{return Err("PVE remote and local media sizes mismatch".into());}
    let file=fs::File::open(&path)?;
    let check=Rc::new(RefCell::new(Check{hash:Sha256::new(),count:0}));
    let finish=Rc::clone(&check);
    let mut input=Reader{source:file,check,limit:bytes};
    let name=path.file_name().and_then(|x|x.to_str()).ok_or("Invalid PVE file")?;
    let logical=format!("pve/{kind}/{vmid}/{name}");
    let expect=sha.to_owned();
    let store=Store::open(root,key)?;
    let backup=store.backup_stream(key,label,"",&logical,&mut input,
        BackupKind::Full,None,||{
            let proof=finish.borrow();
            if proof.count!=bytes||hex::encode(proof.hash.clone().finalize())!=expect {
                return Err("PVE media failed independently supplied SHA/length".into());
            }
            Ok(())
        })?;
    let after=fs::metadata(&path)?;
    if before.len()!=after.len()||before.modified()?!=after.modified()?{
        return Err("PVE backup media changed during export".into());
    }
    if !store.verify(key,&backup.id)?.healthy{
        return Err("PVE encrypted backup verification failed".into());
    }
    Ok(Receipt{provider:"proxmox",id:backup.id,guest_kind:kind.into(),
        vmid,archive:name.into(),bytes,encrypted:true,restore_new_guest_certified:false})
}
pub fn restore_offline(root:&Path,key:&Path,id:&str,dest:&Path)->Result<Receipt>{
    if !dest.is_absolute()||fs::symlink_metadata(dest).is_ok(){
        return Err("PVE restore destination must be a new absolute directory".into());
    }
    let store=Store::open(root,key)?;
    if !store.verify(key,id)?.healthy{return Err("Encrypted PVE backup is corrupt".into());}
    let snapshot:LogicalBackup=store.load(key,id)?;
    if snapshot.kind!=BackupKind::Full||snapshot.parent_id.is_some()||
       !snapshot.top_namespace.is_empty()||snapshot.entries.len()!=1{
        return Err("PVE offline restore requires one full backup archive".into());
    }
    let row=&snapshot.entries[0];
    let parts:Vec<_>=row.path.split('/').collect();
    if parts.len()!=4||parts[0]!="pve"{return Err("Wrong provider media".into());}
    let vmid:u32=parts[2].parse()?;
    if !archive(parts[1],vmid,parts[3]){
        return Err("PVE encrypted manifest contains incompatible guest archive".into());
    }
    if store.restore(key,id,None,dest)?!=1{
        return Err("PVE recovered media count mismatch".into());
    }
    Ok(Receipt{provider:"proxmox",id:id.into(),guest_kind:parts[1].into(),
        vmid,archive:parts[3].into(),bytes:row.bytes,
        encrypted:true,restore_new_guest_certified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn reject_cross_guest_and_unsafe_archive_names(){
        assert!(archive("qemu",101,"vzdump-qemu-101-2026_11_17-00_00_01.vma.zst"));
        assert!(archive("lxc",101,"vzdump-lxc-101-2026_11_17-00_00_01.tar.zst"));
        assert!(!archive("qemu",101,"vzdump-lxc-101-2026_11_17-00_00_01.tar.zst"));
        assert!(!archive("qemu",101,"vzdump-qemu-102-2026_11_17-00_00_01.vma.zst"));
        assert!(!safe("../fake"));
    }
}
