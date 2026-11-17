//! Official PVE restore of a PBS-backed native VM/CT snapshot into a NEW
//! guest via qmrestore/pct restore. Restores from existing PBS storage directly
//! (not from the age chunk store). Provider B-In/age media is a SEPARATE path.
//! Restrict to local root-run PVE with PBS storage preconfigured and verified.
use serde::{Deserialize,Serialize};
use std::{process::{Command,Stdio},os::unix::fs::FileTypeExt};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Serialize)]
pub struct Receipt {
    pub provider:&'static str,pub source_storage:String,
    pub source_volid:String,pub kind:String,pub source_vmid:u32,
    pub new_vmid:u32,pub destination_storage:String,
    pub native_pve_guest_created:bool,pub guest_boot_verified:bool,
    pub guest_content_verified:bool,pub age_roundtrip_verified:bool
}
fn component(s:&str)->bool{
    !s.is_empty()&&s.len()<=64&&s.as_bytes()[0].is_ascii_alphanumeric()&&
    s.bytes().all(|c|c.is_ascii_alphanumeric()||matches!(c,b'-'|b'_'|b'.'))
}
fn volid(source_store:&str,source_kind:&str,vmid:u32,date:&str)->Result<String>{
    if !component(source_store)||!matches!(source_kind,"vm"|"ct")||
       !(100..=999_999_999).contains(&vmid)||date.len()!=20||
       !date.ends_with('Z')||!date.bytes().enumerate().all(|(i,b)|
           match i{4|7=>b==b'-',10=>b==b'T',13|16=>b==b':',19=>b==b'Z',
           _=>b.is_ascii_digit()}) {
        return Err("PBS restore requires exactly pinned store/kind/VMID/time".into());
    }
    Ok(format!("{source_store}:backup/{source_kind}/{vmid}/{date}"))
}
fn run(name:&str,args:&[&str])->Result<Vec<u8>>{
    let p=Command::new(name).args(args).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !p.status.success()||p.stdout.len()>4_194_304{
        return Err(format!("Native Proxmox {name} refused/failed").into());
    }
    Ok(p.stdout)
}
fn resources()->Result<Vec<serde_json::Value>>{
    let value=run("pvesh",&["get","/cluster/resources","--type","vm",
        "--output-format","json"])?;
    let list:Vec<serde_json::Value>=serde_json::from_slice(&value)?;
    if list.len()>100_000{return Err("PVE resource list exceeds safety bound".into());}
    Ok(list)
}
fn absent(new_id:u32)->Result<()>{
    if resources()?.iter().any(|entry|entry.get("vmid").and_then(|v|v.as_u64())==Some(new_id as u64)){
        return Err("Destination VMID is already allocated; refusing PBS guest restore".into());
    }
    Ok(())
}
pub fn restore(source_store:&str,kind:&str,source_id:u32,date:&str,
               new_id:u32,dest_store:&str,confirm:&str)->Result<Receipt>{
    if unsafe{libc::geteuid()}!=0 {
        return Err("PBS-to-PVE guest restore requires local PVE root".into());
    }
    if !component(dest_store)||!(100..=999_999_999).contains(&new_id) {
        return Err("Destination PBS restore storage or NEW VMID invalid".into());
    }
    let source=volid(source_store,kind,source_id,date)?;
    if confirm!=format!("{kind}:{new_id}:{dest_store}"){
        return Err("PBS guest create requires exact kind:NEW_VMID:storage confirmation".into());
    }
    absent(new_id)?;
    // Cross-check actual PBS volume from Proxmox storage inventory; never
    // ask qmrestore to restore arbitrary user-provided paths or local files.
    let manifest=run("pvesm",&["list",source_store,"--content","backup",
        "--output-format","json"])?;
    let rows:Vec<serde_json::Value>=serde_json::from_slice(&manifest)?;
    if rows.len()>100_000{return Err("PBS inventory exceeds limit".into());}
    let matching:Vec<_>=rows.iter().filter(|row|
        row.get("volid").and_then(|v|v.as_str())==Some(source.as_str())).collect();
    if matching.len()!=1 {
        return Err("PBS storage does not contain exactly one selected native guest snapshot".into());
    }
    let proven=matching[0];
    if proven.get("vmid").and_then(|v|v.as_u64()).is_some_and(|id|id!=source_id as u64){
        return Err("PBS snapshot guest ID contradicts provider inventory".into());
    }
    absent(new_id)?;
    let new_string=new_id.to_string();
    let (binary,args):(&str,Vec<&str>)=if kind=="vm"{
        ("qmrestore",vec![&source,&new_string,"--storage",dest_store,
            "--unique","1","--start","0","--live-restore","0"])
    }else{
        ("pct",vec!["restore",&new_string,&source,"--storage",dest_store,
            "--unique","1","--start","0"])
    };
    if run(binary,&args).is_err(){
        return Err("PVE native PBS restore failed; inspect partial NEW guest manually".into());
    }
    if !resources()?.iter().any(|entry|
        entry.get("vmid").and_then(|v|v.as_u64())==Some(new_id as u64)&&
        entry.get("type").and_then(|v|v.as_str())==Some(if kind=="vm"{"qemu"}else{"lxc"})){
        return Err("PVE did not publish a correctly typed NEW VM/CT from native PBS storage".into());
    }
    Ok(Receipt{provider:"pbs",source_storage:source_store.into(),source_volid:source,
        kind:kind.into(),source_vmid:source_id,new_vmid:new_id,
        destination_storage:dest_store.into(),native_pve_guest_created:true,
        guest_boot_verified:false,guest_content_verified:false,
        age_roundtrip_verified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn strict_pbs_snapshots_and_confirmation(){
        assert!(volid("pbs","vm",100,"2026-11-17T07:30:00Z").is_ok());
        assert!(volid("pbs","ct",101,"2026-11-17T07:30:00Z").is_ok());
        assert!(volid("pbs","vm",100,"../../malicious").is_err());
        assert!(volid("pbs/evil","vm",100,"2026-11-17T07:30:00Z").is_err());
        assert!(restore("pbs","vm",100,"2026-11-17T07:30:00Z",
            110,"local-lvm","wrong").is_err());
    }
}
