//! Ceph RBD snapshot B-In and NEW-image R-Out. No shell, mapping or overwrite.
//! Consistency of a running VM is *not* guaranteed by a storage snapshot.
use crate::chunk_store::{BackupKind,LogicalBackup,Store};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{cell::RefCell,io::{Read,Write},path::Path,process::{Command,Stdio},rc::Rc};

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot{
    pub pool:String,pub image:String,pub snapshot:String,
    pub snapshot_id:u64,pub bytes:u64,pub sha256:String
}
#[derive(Debug,Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub backup_id:String,pub bytes:u64,
    pub sha256:String,pub created_new_image:bool,
    pub encrypted:bool,pub guest_consistency_certified:bool
}
fn safe(s:&str)->bool{
    !s.is_empty()&&s.len()<=128&&s.as_bytes()[0].is_ascii_alphanumeric()&&
    !s.contains("..")&&s.bytes().all(|b|
        b.is_ascii_alphanumeric()||matches!(b,b'.'|b'_'|b'-'))
}
fn sha(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||matches!(b,b'a'..=b'f'))}
impl Snapshot{
    pub fn load(path:&Path)->Result<Self>{
        let buf=zeroize::Zeroizing::new(crate::protected_config::read_owner_only(path,16_384)?);
        let s:Self=serde_json::from_slice(&buf)?;
        s.validate()?;
        Ok(s)
    }
    fn validate(&self)->Result<()>{
        if !safe(&self.pool)||!safe(&self.image)||!safe(&self.snapshot)||
           self.snapshot_id==0||self.bytes==0||self.bytes>1_099_511_627_776||
           !sha(&self.sha256){
            return Err("RBD snapshot requires pool/image/snapshot name, pinned numeric ID, size and SHA256".into());
        }
        Ok(())
    }
    fn path(&self)->String{format!("{}/{}@{}",self.pool,self.image,self.snapshot)}
}
fn output(args:&[&str])->Result<Vec<u8>>{
    let o=Command::new("rbd").args(args).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !o.status.success()||o.stdout.len()>1_048_576{
        return Err("RBD preflight failed or response exceeded limit".into());
    }
    Ok(o.stdout)
}
fn pinned(s:&Snapshot)->Result<()>{
    let image=format!("{}/{}",s.pool,s.image);
    let raw=output(&["snap","ls","--format","json",&image])?;
    let value:serde_json::Value=serde_json::from_slice(&raw)?;
    let snaps=value.as_array().ok_or("RBD snapshot API was not an array")?;
    if snaps.iter().filter(|x|
        x["id"].as_u64()==Some(s.snapshot_id)&&
        x["name"].as_str()==Some(&s.snapshot)).count()!=1{
        return Err("RBD immutable snapshot ID changed or missing".into());
    }
    Ok(())
}
struct State{sha:Sha256,bytes:u64}
struct Checked<R:Read>{inner:R,state:Rc<RefCell<State>>,limit:u64}
impl<R:Read> Read for Checked<R>{
    fn read(&mut self,buf:&mut [u8])->std::io::Result<usize>{
        let n=self.inner.read(buf)?;
        let mut s=self.state.borrow_mut();
        s.bytes=s.bytes.checked_add(n as u64)
            .ok_or_else(||std::io::Error::other("RBD source overflow"))?;
        if s.bytes>self.limit{return Err(std::io::Error::other("RBD exceeded pinned size"));}
        s.sha.update(&buf[..n]);Ok(n)
    }
}
pub fn backup(s:&Snapshot,root:&Path,key:&Path,label:&str)->Result<Receipt>{
    s.validate()?;pinned(s)?;
    if label.trim().is_empty()||label.len()>256{return Err("Invalid RBD backup label".into());}
    let store=Store::open(root,key)?;
    let source=s.path();
    let mut child=Command::new("rbd").args(["export","--no-progress",&source,"-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let pipe=child.stdout.take().ok_or("RBD export pipe missing")?;
    let state=Rc::new(RefCell::new(State{sha:Sha256::new(),bytes:0}));
    let result_state=Rc::clone(&state);
    let mut reader=Checked{inner:pipe,state,limit:s.bytes};
    let length=s.bytes;let expected=s.sha256.clone();
    let logical=format!("rbd/{}/{}/{}.raw",s.pool,s.image,s.snapshot);
    let result=store.backup_stream(key,label,"",&logical,&mut reader,BackupKind::Full,None,||{
        if !child.wait().map_err(|_|"RBD child wait failed")?.success(){
            return Err("RBD export returned failure".into());
        }
        let check=result_state.borrow();
        if check.bytes!=length||hex::encode(check.sha.clone().finalize())!=expected{
            return Err("RBD source differs from independently pinned SHA256 or length".into());
        }
        Ok(())
    });
    if result.is_err(){let _=child.kill();let _=child.wait();}
    let snap=result?;
    pinned(s)?;
    if !store.verify(key,&snap.id)?.healthy{
        return Err("Encrypted RBD backup failed verification".into());
    }
    Ok(Receipt{provider:"rbd",backup_id:snap.id,bytes:s.bytes,
        sha256:s.sha256.clone(),created_new_image:false,encrypted:true,
        guest_consistency_certified:false})
}
pub fn restore_new(root:&Path,key:&Path,id:&str,pool:&str,image:&str,
                   confirm:&str)->Result<Receipt>{
    if !safe(pool)||!safe(image)||confirm!=format!("{pool}/{image}"){
        return Err("RBD restore requires explicit confirmation of NEW pool/image".into());
    }
    let store=Store::open(root,key)?;
    if !store.verify(key,id)?.healthy{return Err("Corrupt encrypted RBD backup".into());}
    let snap:LogicalBackup=store.load(key,id)?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||
       !snap.top_namespace.is_empty()||snap.entries.len()!=1 {
        return Err("RBD restore requires one standalone full image".into());
    }
    let entry=&snap.entries[0];
    if !entry.path.starts_with("rbd/")||!entry.path.ends_with(".raw")||
       entry.path.split('/').count()!=4||entry.bytes==0{
        return Err("Backup is not a valid native RBD image".into());
    }
    let existing:Vec<String>=serde_json::from_slice(
        &output(&["ls","--pool",pool,"--format","json"])?)?;
    if existing.iter().any(|s|s==image){
        return Err("RBD target image exists: never overwrite".into());
    }
    let target=format!("{pool}/{image}");
    let mut writer=Command::new("rbd").args(["import","--image-format","2","-",&target])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let mut stdin=writer.stdin.take().ok_or("RBD new image import pipe missing")?;
    let transferred=(||->Result<(u64,String)>{
        let mut hash=Sha256::new();let mut bytes=0u64;
        for chunk in &entry.chunks{
            let data=store.read_chunk_ref(key,chunk)?;
            stdin.write_all(&data)?;
            hash.update(&data);
            bytes=bytes.checked_add(data.len() as u64).ok_or("RBD restore overflow")?;
        }
        stdin.flush()?;drop(stdin);
        if bytes!=entry.bytes{return Err("RBD restore size mismatch".into());}
        Ok((bytes,hex::encode(hash.finalize())))
    })();
    if transferred.is_err(){let _=writer.kill();let _=writer.wait();}
    let (length,digest)=transferred?;
    if !writer.wait()?.success(){
        return Err("RBD import failed; operator must inspect the possibly partial NEW image".into());
    }
    let mut reader=Command::new("rbd").args(["export","--no-progress",&target,"-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut source=reader.stdout.take().ok_or("RBD remote readback missing")?;
    let mut bytes=0u64;let mut hash=Sha256::new();let mut buf=[0u8;65536];
    loop{
        let count=source.read(&mut buf)?;
        if count==0{break;}
        bytes=bytes.checked_add(count as u64).ok_or("RBD readback overflow")?;
        if bytes>length{return Err("RBD remote image larger than source".into());}
        hash.update(&buf[..count]);
    }
    if !reader.wait()?.success()||bytes!=length||hex::encode(hash.finalize())!=digest{
        return Err("RBD NEW image failed independent readback integrity".into());
    }
    Ok(Receipt{provider:"rbd",backup_id:id.into(),bytes:length,
        sha256:digest,created_new_image:true,encrypted:true,guest_consistency_certified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn reject_unpinned_and_cross_target(){
        assert!(safe("data-pool"));assert!(!safe("../data"));assert!(!sha(&"A".repeat(64)));
        let p=Snapshot{pool:"rbd".into(),image:"disk".into(),snapshot:"s1".into(),
            snapshot_id:0,bytes:1024,sha256:"a".repeat(64)};
        assert!(p.validate().is_err());
        assert!(restore_new(Path::new("/none"),Path::new("/none"),"id",
            "pool","image","wrong").is_err());
    }
}
