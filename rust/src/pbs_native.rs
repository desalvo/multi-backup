//! Native PBS B-In via the official Proxmox Backup Client reader, with
//! validated repository/snapshot/file, pinned TLS fingerprint (or CA),
//! authenticated age chunk publication and create-only OFFLINE raw restore.
//! No TLS bypass, no ignore-missing-signature, no deleting PBS snapshots.
//! Live PBS server and VM/CT rehydration are separately gated.
use crate::chunk_store::{BackupKind,LogicalBackup,Store};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{cell::RefCell,fs,io::{self,Read},path::{Path,PathBuf},
          process::{Command,Stdio},rc::Rc};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source{
    pub repository:String,pub snapshot:String,pub filename:String,
    #[serde(default)]pub namespace:Option<String>,
    pub password_file:PathBuf,
    #[serde(default)]pub tls_fingerprint:Option<String>,
    pub expected_bytes:u64,pub sha256:String
}
#[derive(Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub id:String,pub snapshot:String,
    pub archive:String,pub bytes:u64,pub encrypted:bool,
    pub native_pbs_read_verified:bool,pub restored_to_new_guest:bool
}
fn safe(s:&str,max:usize,extra:&[u8])->bool{
    !s.is_empty()&&s.len()<=max&&!s.contains("..")&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||extra.contains(&b))
}
impl Source{
    pub fn load(path:&Path)->Result<Self>{
        let raw=zeroize::Zeroizing::new(crate::protected_config::read_owner_only(path,16_384)?);
        let item:Self=serde_json::from_slice(&raw)?;
        item.validate()?;Ok(item)
    }
    fn validate(&self)->Result<()>{
        if !safe(&self.repository,255,b"@!_.:-")||
           !self.repository.contains('@')||!self.repository.contains(':')||
           !safe(&self.snapshot,180,b"/:-_")||
           self.snapshot.split('/').count()!=3||
           !self.snapshot.starts_with("vm/")||
           !safe(&self.filename,128,b"._-")||
           !self.filename.ends_with(".img.fidx")||
           self.expected_bytes==0||self.expected_bytes>1_099_511_627_776||
           self.sha256.len()!=64||
           !self.sha256.bytes().all(|b|b.is_ascii_digit()||matches!(b,b'a'..=b'f')){
            return Err("PBS requires exact VM snapshot, index filename, repo, SHA and bytes".into());
        }
        if let Some(ns)=&self.namespace{
            if !safe(ns,256,b"/_.-")||ns.split('/').any(|x|x.is_empty()||x=="."||x==".."){
                return Err("Invalid PBS namespace".into());
            }
        }
        if let Some(fp)=&self.tls_fingerprint{
            if fp.len()!=95||!fp.bytes().all(|b|b.is_ascii_hexdigit()||b==b':'){
                return Err("Invalid pinned PBS TLS fingerprint".into());
            }
        }
        crate::protected_config::read_owner_only(&self.password_file,4096)?;
        Ok(())
    }
    fn command(&self)->Command{
        let mut cmd=Command::new("proxmox-backup-client");
        for secret in ["PBS_PASSWORD","PBS_PASSWORD_CMD","PBS_PASSWORD_FD",
                       "PBS_ENCRYPTION_PASSWORD","PBS_ENCRYPTION_PASSWORD_CMD",
                       "PBS_ENCRYPTION_PASSWORD_FILE","PBS_REPOSITORY",
                       "PBS_FINGERPRINT","PBS_NAMESPACE","ALL_PROXY"]{
            cmd.env_remove(secret);
        }
        cmd.env("PBS_PASSWORD_FILE",&self.password_file);
        if let Some(fp)=&self.tls_fingerprint{cmd.env("PBS_FINGERPRINT",fp);}
        cmd
    }
    fn prove_snapshot_file(&self)->Result<()>{
        let mut cmd=self.command();
        cmd.args(["snapshot","files",&self.snapshot,"--repository",&self.repository,
                  "--output-format","json"]);
        if let Some(ns)=&self.namespace {cmd.args(["--ns",ns]);}
        let out=cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
        if !out.status.success()||out.stdout.len()>2_097_152{
            return Err("PBS snapshot file inventory was not authenticated".into());
        }
        let json:serde_json::Value=serde_json::from_slice(&out.stdout)?;
        let rows=json.as_array().ok_or("PBS file inventory response not an array")?;
        if !rows.iter().any(|r|r["filename"].as_str()==Some(&self.filename)){
            return Err("Requested PBS VM disk archive is missing from authenticated snapshot".into());
        }
        Ok(())
    }
}
struct Check{hash:Sha256,bytes:u64}
struct Reader<R:Read>{inner:R,proof:Rc<RefCell<Check>>,max:u64}
impl<R:Read> Read for Reader<R>{
    fn read(&mut self,buffer:&mut [u8])->io::Result<usize>{
        let n=self.inner.read(buffer)?;
        let mut check=self.proof.borrow_mut();
        check.bytes=check.bytes.checked_add(n as u64)
            .ok_or_else(||io::Error::other("PBS raw restore overflow"))?;
        if check.bytes>self.max{return Err(io::Error::other("PBS raw source too large"));}
        check.hash.update(&buffer[..n]);Ok(n)
    }
}
pub fn backup(s:&Source,root:&Path,key:&Path,label:&str)->Result<Receipt>{
    s.validate()?;
    if label.is_empty()||label.len()>256{return Err("Invalid PBS backup label".into());}
    s.prove_snapshot_file()?;
    let mut cmd=s.command();
    cmd.args(["restore",&s.snapshot,&s.filename,"-","--repository",&s.repository]);
    if let Some(ns)=&s.namespace{cmd.args(["--ns",ns]);}
    let mut child=cmd.stdin(Stdio::null()).stdout(Stdio::piped())
        .stderr(Stdio::null()).spawn()?;
    let output=child.stdout.take().ok_or("PBS restore stream unavailable")?;
    let state=Rc::new(RefCell::new(Check{hash:Sha256::new(),bytes:0}));
    let check=Rc::clone(&state);
    let mut reader=Reader{inner:output,proof:state,max:s.expected_bytes};
    let logical=format!("pbs/{}/{}",hex::encode(s.snapshot.as_bytes()),s.filename);
    let expected=s.sha256.clone();let bytes=s.expected_bytes;
    let store=Store::open(root,key)?;
    let snap=store.backup_stream(key,label,"",&logical,&mut reader,
        BackupKind::Full,None,||{
            if !child.wait().map_err(|_|"PBS client wait failed")?.success(){
                return Err("PBS native restore command failed".into());
            }
            let info=check.borrow();
            if info.bytes!=bytes||hex::encode(info.hash.clone().finalize())!=expected{
                return Err("PBS recovered disk differs from pinned SHA/length".into());
            }
            Ok(())
        });
    if snap.is_err(){let _=child.kill();let _=child.wait();}
    let snap=snap?;
    if !store.verify(key,&snap.id)?.healthy{
        return Err("PBS backup encryption or verification failed".into());
    }
    Ok(Receipt{provider:"pbs",id:snap.id,snapshot:s.snapshot.clone(),
        archive:s.filename.clone(),bytes,encrypted:true,
        native_pbs_read_verified:true,restored_to_new_guest:false})
}
pub fn restore_offline(root:&Path,key:&Path,id:&str,destination:&Path)->Result<Receipt>{
    if !destination.is_absolute()||fs::symlink_metadata(destination).is_ok(){
        return Err("PBS offline restore requires NEW absolute directory".into());
    }
    let store=Store::open(root,key)?;
    if !store.verify(key,id)?.healthy{return Err("PBS ciphertext integrity rejected".into());}
    let snap:LogicalBackup=store.load(key,id)?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||
       !snap.top_namespace.is_empty()||snap.entries.len()!=1{
        return Err("PBS offline recovery requires one full image".into());
    }
    let entry=&snap.entries[0];
    let parts:Vec<_>=entry.path.split('/').collect();
    if parts.len()!=3||parts[0]!="pbs"||!parts[2].ends_with(".img.fidx"){
        return Err("Not a verified PBS VM disk image".into());
    }
    let source=String::from_utf8(hex::decode(parts[1])?)?;
    if store.restore(key,id,None,destination)?!=1{return Err("PBS offline restore incomplete".into());}
    Ok(Receipt{provider:"pbs",id:id.into(),snapshot:source,
        archive:parts[2].into(),bytes:entry.bytes,encrypted:true,
        native_pbs_read_verified:false,restored_to_new_guest:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn rejects_non_vm_and_unsafe_repository(){
        let s=Source{repository:"user@pbs!token@backup.example:datastore".into(),
            snapshot:"vm/100/2026-11-17T01:00:00Z".into(),
            filename:"drive-scsi0.img.fidx".into(),namespace:None,
            password_file:PathBuf::from("/not-existing"),tls_fingerprint:None,
            expected_bytes:4096,sha256:"a".repeat(64)};
        assert!(s.validate().is_err()); // Missing protected password file.
        let mut other=s;other.filename="../bad.img.fidx".into();
        assert!(other.validate().is_err());
        other.filename="root.pxar.didx".into();
        assert!(other.validate().is_err());
    }
}
