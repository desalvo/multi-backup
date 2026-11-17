//! Actual create-only Proxmox guest recovery: decrypt/verify a native
//! vzdump media archive, then qmrestore or pct restore to a NEW guest ID.
//! Only runs on local Linux PVE host with pvesh/qmrestore/pct. Never
//! overwrites an existing guest, never uses force, never auto-starts.
use crate::{chunk_store::{BackupKind,Store},pve_media};
use serde::{Deserialize,Serialize};
use std::{fs,io::Read,path::Path,process::{Command,Stdio}};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub backup_id:String,pub guest_kind:String,
    pub source_id:u32,pub new_guest_id:u32,pub storage:String,
    pub restore_command:String,pub created_new_guest:bool,
    pub guest_started:bool,pub boot_verified:bool,pub guest_data_verified:bool,
}
#[derive(Debug,Deserialize)]
struct Guest{
    vmid:u32,
    #[serde(rename="type")]kind:String,
}
fn safe(s:&str)->bool{
    !s.is_empty()&&s.len()<=64&&s.as_bytes()[0].is_ascii_alphanumeric()&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'))
}
fn cmd(program:&str,args:&[&str])->Result<Vec<u8>>{
    let output=Command::new(program).args(args)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !output.status.success()||output.stdout.len()>2_097_152{
        return Err(format!("PVE {program} failed; no implicit force or cleanup").into());
    }
    Ok(output.stdout)
}
fn guest_list()->Result<Vec<Guest>>{
    let raw=cmd("pvesh",&["get","/cluster/resources","--type","vm",
                         "--output-format","json"])?;
    let list:Vec<Guest>=serde_json::from_slice(&raw)?;
    if list.len()>100_000{return Err("Oversized PVE guest list".into());}
    Ok(list)
}
fn guest_present(id:u32,kind:&str)->Result<bool>{
    let mut found=0usize;
    for item in guest_list()?{
        if item.vmid==id{
            if item.kind!=kind{return Err("PVE guest ID belongs to another guest kind".into());}
            found+=1;
        }
    }
    if found>1{return Err("Duplicate PVE guest ID in cluster inventory".into());}
    Ok(found==1)
}
pub fn restore_new_guest(
    root:&Path,key:&Path,id:&str,new_id:u32,storage:&str,stage_root:&Path,
    confirm:&str)->Result<Receipt>{
    if unsafe{libc::geteuid()}!=0{
        return Err("Proxmox guest restoration requires local PVE root privileges".into());
    }
    if !(100..=999_999_999).contains(&new_id)||!safe(storage){
        return Err("Invalid new Proxmox VM/CT ID or destination storage".into());
    }
    if !stage_root.is_absolute()||!stage_root.is_dir() ||
       fs::symlink_metadata(stage_root)?.file_type().is_symlink(){
        return Err("PVE restore staging must be an existing ordinary absolute directory".into());
    }
    let stage_root=fs::canonicalize(stage_root)?;
    let store_root=fs::canonicalize(root)?;
    if stage_root.starts_with(&store_root)||store_root.starts_with(&stage_root)||
       key.starts_with(&stage_root){
        return Err("PVE restore staging must not overlap encrypted backup store".into());
    }
    let store=Store::open(&store_root,key)?;
    if !store.verify(key,id)?.healthy{
        return Err("PVE guest encrypted archive failed verification".into());
    }
    let snap=store.load(key,id)?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||
       !snap.top_namespace.is_empty()||snap.entries.len()!=1{
        return Err("PVE guest restoration requires one full verified backup archive".into());
    }
    let entry=&snap.entries[0];let parts:Vec<_>=entry.path.split('/').collect();
    if parts.len()!=4||parts[0]!="pve"||entry.bytes==0{
        return Err("PVE guest restore requires a native vzdump media member".into());
    }
    let kind=parts[1];
    let source_id:u32=parts[2].parse()?;
    let filename=parts[3];
    if !(100..=999_999_999).contains(&source_id)||
       !((kind=="qemu"&&filename.starts_with(&format!("vzdump-qemu-{source_id}-"))&&filename.ends_with(".vma.zst"))||
         (kind=="lxc"&&filename.starts_with(&format!("vzdump-lxc-{source_id}-"))&&filename.ends_with(".tar.zst")))||
       !safe(filename){
        return Err("PVE encrypted archive filename/guest type failed authentication".into());
    }
    let confirmation=format!("{kind}:{new_id}:{storage}");
    if confirm!=confirmation{
        return Err("PVE creation requires exact kind:NEW_VMID:storage confirmation".into());
    }
    if guest_present(new_id,kind)?{
        return Err("PVE guest already exists: refusing restore to nonempty VMID".into());
    }
    // Do not extract arbitrary archive paths: the encrypted manifest is
    // checked before restore; PVE archive itself is consumed by native tools.
    let stage=tempfile::Builder::new().prefix(".mb-pve-new-").tempdir_in(&stage_root)?;
    let recovered=pve_media::restore_offline(&store_root,key,id,&stage.path().join("media"))?;
    if recovered.guest_kind!=kind||recovered.vmid!=source_id||recovered.archive!=filename{
        return Err("PVE verified offline media identity changed".into());
    }
    let archive=stage.path().join("media").join(&entry.path);
    let actual=fs::symlink_metadata(&archive)?;
    if !actual.file_type().is_file()||actual.len()!=entry.bytes{
        return Err("PVE recovered archive missing or wrong byte count".into());
    }
    // Recheck immediately before creation; qmrestore and pct restore also
    // reject existing VMIDs unless --force is explicitly supplied (NEVER).
    if guest_present(new_id,kind)?{
        return Err("Concurrent PVE allocation claimed target guest".into());
    }
    let guest_id=new_id.to_string();
    let archive=archive.to_str().ok_or("PVE archive path not UTF8")?;
    let program=if kind=="qemu"{"qmrestore"}else{"pct"};
    let result=if kind=="qemu" {
        cmd(program,&[archive,&guest_id,"--storage",storage,
                      "--unique","1","--start","0"])
    }else{
        cmd(program,&["restore",&guest_id,archive,"--storage",storage,
                      "--unique","1","--start","0"])
    };
    if result.is_err(){
        // Failed native restoration may leave a partially-created NEW guest.
        // Never delete it automatically: explicit operator inspection only.
        return Err("Native PVE restore failed; inspect possible partial new guest".into());
    }
    if !guest_present(new_id,kind)?{
        return Err("PVE native restore returned success but new guest is missing".into());
    }
    Ok(Receipt{provider:"proxmox",backup_id:id.into(),guest_kind:kind.into(),
        source_id,new_guest_id:new_id,storage:storage.into(),
        restore_command:program.into(),created_new_guest:true,
        guest_started:false,boot_verified:false,guest_data_verified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn unsafe_targets_fail_before_pve(){
        for bad in ["..","-evil","x/y",""]{assert!(!safe(bad));}
        assert!(safe("local-lvm"));
        assert!(restore_new_guest(Path::new("/none"),Path::new("/none"),
            "bad",99,"local",Path::new("/tmp"),"qemu:99:local").is_err());
        assert!(restore_new_guest(Path::new("/none"),Path::new("/none"),
            "bad",101,"../out",Path::new("/tmp"),"qemu:101:../out").is_err());
    }
}
