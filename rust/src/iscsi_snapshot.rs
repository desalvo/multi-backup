//! Native iSCSI LUN read-only snapshot B-In using the kernel initiator.
//! Authenticated device identity, exact WWID, read-only state, SHA256 and
//! bytes are mandatory. No live writable LUN, SCSI write, target login or
//! destructive restore is performed. B-Out to a new LUN remains unimplemented.
use crate::chunk_store::{BackupKind,LogicalBackup,Store};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{cell::RefCell,fs,io::{self,Read},path::{Path,PathBuf},
          process::{Command,Stdio},rc::Rc};

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot{
    pub device:String,
    pub target_iqn:String,
    pub lun:u32,
    pub expected_serial:String,
    pub bytes:u64,
    pub sha256:String
}
#[derive(Debug,Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub backup_id:String,
    pub target_iqn:String,pub lun:u32,pub bytes:u64,
    pub encrypted:bool,pub verified:bool,
    pub create_only_local_image:bool,pub iscsi_lun_restore_certified:bool
}
fn valid(s:&str,max:usize)->bool{
    !s.is_empty()&&s.len()<=max&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'-'|b'_'|b':'))
}
impl Snapshot{
    pub fn load(path:&Path)->Result<Self>{
        let raw=zeroize::Zeroizing::new(crate::protected_config::read_owner_only(path,16_384)?);
        let config:Self=serde_json::from_slice(&raw)?;
        config.validate()?;
        Ok(config)
    }
    fn validate(&self)->Result<()>{
        if !self.target_iqn.starts_with("iqn.") ||
           !valid(&self.target_iqn,220)||!valid(&self.expected_serial,256)||
           self.lun>16_383||self.bytes==0||self.bytes>1_099_511_627_776||
           self.sha256.len()!=64||
           !self.sha256.bytes().all(|b|b.is_ascii_digit()||matches!(b,b'a'..=b'f'))||
           !self.device.starts_with("/dev/disk/by-path/ip-")||
           self.device.contains("..")||self.device.contains("//")||
           !self.device.ends_with(&format!("-iscsi-{}-lun-{}",self.target_iqn,self.lun)){
            return Err("iSCSI requires a pinned kernel by-path IQN/LUN, WWID/serial, size and SHA256".into());
        }
        Ok(())
    }
}
fn command(args:&[&str])->Result<String>{
    let output=Command::new(args[0]).args(&args[1..])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !output.status.success()||output.stdout.len()>32_768{
        return Err("iSCSI kernel/block identity preflight failed".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn validate_kernel(s:&Snapshot)->Result<PathBuf>{
    s.validate()?;
    let path=Path::new(&s.device);
    #[cfg(not(unix))]
    {let _=path;return Err("Native iSCSI block snapshot requires a Linux kernel".into());}
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if !fs::symlink_metadata(path)?.file_type().is_symlink(){
            return Err("iSCSI source must be a genuine kernel by-path symlink".into());
        }
        let dev=fs::canonicalize(path)?;
        let meta=fs::symlink_metadata(&dev)?;
        if !dev.starts_with("/dev/")||!meta.file_type().is_block_device(){
            return Err("iSCSI source did not resolve to a real block device".into());
        }
        let device=dev.to_str().ok_or("Non-UTF8 iSCSI device path")?;
        let props=command(&["udevadm","info","--query=property","--name",device])?;
        let path_name=path.file_name().and_then(|x|x.to_str()).ok_or("Bad iSCSI path")?;
        if !props.lines().any(|line|line==format!("ID_PATH={path_name}")){
            return Err("iSCSI kernel udev path did not match pinned IQN/LUN".into());
        }
        if !props.lines().any(|line|
            line==format!("ID_SERIAL={}",s.expected_serial)||
            line==format!("ID_SCSI_SERIAL={}",s.expected_serial)||
            line==format!("ID_WWN={}",s.expected_serial)){
            return Err("iSCSI kernel device serial/WWID attestation mismatch".into());
        }
        if command(&["blockdev","--getro",device])?.trim()!="1"{
            return Err("iSCSI source must be an explicitly read-only LUN snapshot".into());
        }
        if command(&["blockdev","--getsize64",device])?.trim()!=s.bytes.to_string(){
            return Err("iSCSI source block size differs from pinned byte count".into());
        }
        Ok(dev)
    }
}
struct State{hash:Sha256,bytes:u64}
struct Checked<R:Read>{reader:R,proof:Rc<RefCell<State>>,limit:u64}
impl<R:Read> Read for Checked<R>{
    fn read(&mut self,buf:&mut [u8])->io::Result<usize>{
        let n=self.reader.read(buf)?;
        let mut s=self.proof.borrow_mut();
        s.bytes=s.bytes.checked_add(n as u64)
            .ok_or_else(||io::Error::other("iSCSI stream length overflow"))?;
        if s.bytes>self.limit{return Err(io::Error::other("iSCSI snapshot exceeded pinned length"));}
        s.hash.update(&buf[..n]);Ok(n)
    }
}
pub fn backup(s:&Snapshot,root:&Path,key:&Path,label:&str)->Result<Receipt>{
    if label.trim().is_empty()||label.len()>256{
        return Err("iSCSI backup requires nonempty bounded label".into());
    }
    let device=validate_kernel(s)?;
    let store=Store::open(root,key)?;
    let reader=fs::File::open(&device)?;
    let state=Rc::new(RefCell::new(State{hash:Sha256::new(),bytes:0}));
    let finish=Rc::clone(&state);
    let mut checked=Checked{reader,proof:state,limit:s.bytes};
    let expected=s.sha256.clone();let limit=s.bytes;
    let name=format!("iscsi/{}/lun_{}.raw",s.target_iqn,s.lun);
    let result=store.backup_stream(key,label,"",&name,&mut checked,
        BackupKind::Full,None,||{
            let state=finish.borrow();
            if state.bytes!=limit||hex::encode(state.hash.clone().finalize())!=expected{
                return Err("iSCSI snapshot differs from independently pinned SHA256/size".into());
            }
            Ok(())
        })?;
    validate_kernel(s)?;
    if !store.verify(key,&result.id)?.healthy{return Err("Encrypted iSCSI chunk integrity failed".into());}
    Ok(Receipt{provider:"iscsi",backup_id:result.id,target_iqn:s.target_iqn.clone(),
        lun:s.lun,bytes:s.bytes,encrypted:true,verified:true,
        create_only_local_image:false,iscsi_lun_restore_certified:false})
}
pub fn restore_new_local(root:&Path,key:&Path,id:&str,dest:&Path)->Result<Receipt>{
    if !dest.is_absolute()||fs::symlink_metadata(dest).is_ok(){
        return Err("iSCSI offline image restore requires NEW absolute directory".into());
    }
    let store=Store::open(root,key)?;
    if !store.verify(key,id)?.healthy{return Err("Corrupt encrypted iSCSI backup".into());}
    let snap:LogicalBackup=store.load(key,id)?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||snap.entries.len()!=1{
        return Err("iSCSI restore requires a standalone raw snapshot".into());
    }
    let entry=&snap.entries[0];
    let parts:Vec<_>=entry.path.split('/').collect();
    if parts.len()!=3||parts[0]!="iscsi"||
       !parts[1].starts_with("iqn.")||
       !parts[2].starts_with("lun_")||!parts[2].ends_with(".raw"){
        return Err("Encrypted backup is not an attested iSCSI LUN image".into());
    }
    let count=store.restore(key,id,None,dest)?;
    if count!=1{return Err("iSCSI offline restore missing raw member".into());}
    Ok(Receipt{provider:"iscsi",backup_id:id.to_owned(),
        target_iqn:parts[1].into(),lun:parts[2][4..parts[2].len()-4].parse()?,
        bytes:entry.bytes,encrypted:true,verified:true,
        create_only_local_image:true,iscsi_lun_restore_certified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn invalid_target_and_unfenced_blocker(){
        let c=Snapshot{device:"/tmp/fake".into(),target_iqn:"iqn.2026-11.test:target".into(),
            lun:0,expected_serial:"SERIAL1".into(),bytes:4096,sha256:"a".repeat(64)};
        assert!(c.validate().is_err());
        assert!(validate_kernel(&c).is_err());
        assert!(restore_new_local(Path::new("/nonexistent"),Path::new("/none"),
            "invalid",Path::new("/")).is_err());
    }
}
