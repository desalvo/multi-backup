//! oVirt Engine v4 IMAGEIO VM disk backup B-In (bounded Preview).
 //! An external oVirt backup coordinator MUST have started an actual VM
 //! backup and a DOWNLOAD image transfer. This native Rust adapter never
 //! starts/stops a VM, creates snapshots, guesses transfer endpoints, or
 //! uses a guest-agent/container fallback. It verifies the oVirt Engine
 //! READY backup, matching pre-issued transfer ID/URL, exact VM/disk IDs,
 //! transfers bytes over OS-trusted TLS into age chunks, checks independent
 //! SHA256 + length before commit, and revalidates provider state afterward.
 //! Full VM restore/upload, backup finalization, multiple disks/checkpoints
 //! and physical oVirt field certification remain NOT implemented.
use crate::{chunk_store::{BackupKind,LogicalBackup,Store},
            protected_config};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::{cell::RefCell,fs,io::{self,Read},path::Path,rc::Rc,time::Duration};
use url::Url;
use zeroize::{Zeroize,Zeroizing};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub engine_endpoint:String,
    pub bearer_token:String,
    pub vm_id:String,
    pub backup_id:String,
    pub disk_id:String,
    pub transfer_id:String,
    pub transfer_url:String,
    pub expected_bytes:u64,
    /// Explicit actual IMAGEIO content format, not a guessed .qcow2 file.
    pub expected_format:String,
    pub expected_sha256:String,
}
impl Drop for Config{
    fn drop(&mut self){self.bearer_token.zeroize();}
}
fn uuid(s:&str)->bool{
    s.len()==36&&s.bytes().enumerate().all(|(i,b)|
      if [8,13,18,23].contains(&i){b==b'-'}
      else{b.is_ascii_digit()||matches!(b,b'a'..=b'f')})
}
fn check_url(url:&str,root:bool)->Result<Url>{
    let u=Url::parse(url)?;
    if u.scheme()!="https"||u.host_str().is_none()||
       !u.username().is_empty()||u.password().is_some()||
       u.query().is_some()||u.fragment().is_some()||
       u.path().contains(".."){
        return Err("oVirt requires direct, trusted HTTPS URL without userinfo, query or redirects".into());
    }
    if root && u.path()!="/"{
        return Err("oVirt Engine requires bare HTTPS authority".into());
    }
    if !root{
        let Some(segment)=u.path().strip_prefix("/images/") else{
            return Err("oVirt IMAGEIO transfer requires direct pinned image ticket path".into())
        };
        if !uuid(segment){return Err("oVirt IMAGEIO ticket is not a valid UUID".into());}
    }
    Ok(u)
}
impl Config{
    pub fn load(path:&Path)->Result<Self>{
        let data=Zeroizing::new(protected_config::read_owner_only(path,16_384)?);
        let config:Self=serde_json::from_slice(&data)?;
        config.validate()?;Ok(config)
    }
    fn validate(&self)->Result<()>{
        check_url(&self.engine_endpoint,true)?;
        check_url(&self.transfer_url,false)?;
        if ![&self.vm_id,&self.backup_id,&self.disk_id,&self.transfer_id]
            .iter().all(|x|uuid(x)) {
            return Err("oVirt VM, backup, disk and transfer IDs must be exact UUIDs".into());
        }
        if self.bearer_token.len()<16||self.bearer_token.len()>4096||
           self.bearer_token.bytes().any(|x|x.is_ascii_whitespace()||x.is_ascii_control())||
           !matches!(self.expected_format.as_str(),"raw"|"qcow2")||
           self.expected_sha256.len()!=64||
           !self.expected_sha256.bytes().all(|c|c.is_ascii_hexdigit())||
           self.expected_bytes==0||self.expected_bytes>1_099_511_627_776 {
            return Err("Unsafe oVirt image identity/token/hash/byte count".into());
        }
        Ok(())
    }
    fn engine_get(&self,route:&[&str])->Result<Value>{
        let mut url=check_url(&self.engine_endpoint,true)?;
        url.path_segments_mut().map_err(|_|"Invalid oVirt Engine URL")?
            .extend(route);
        let token=Zeroizing::new(format!("Bearer {}",self.bearer_token));
        let client=ureq::AgentBuilder::new().redirects(0)
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(50)).build();
        let response=client.get(url.as_str()).set("Authorization",&token)
            .set("Version","4").set("Accept","application/json")
            .call().map_err(|_|"Authenticated oVirt Engine API denied transfer proof or TLS invalid")?;
        if response.status()!=200{return Err("oVirt Engine status is not HTTP 200".into());}
        let mut data=Vec::new();
        response.into_reader().take(65_537).read_to_end(&mut data)?;
        if data.len()>65_536{return Err("oVirt API evidence exceeds size bounds".into());}
        Ok(serde_json::from_slice(&data)?)
    }
    fn backup_ready(&self)->Result<()>{
        let j=self.engine_get(&["ovirt-engine","api","vms",&self.vm_id,
                               "backups",&self.backup_id])?;
        let v=j.get("backup").unwrap_or(&j);
        if v["id"]!=self.backup_id ||
           v["phase"].as_str().is_none_or(|x|!x.eq_ignore_ascii_case("ready")){
            return Err("oVirt VM backup is not the exact READY backup ID".into());
        }
        Ok(())
    }
    fn transfer_ready(&self)->Result<()>{
        let j=self.engine_get(&["ovirt-engine","api","imagetransfers",&self.transfer_id])?;
        let t=j.get("image_transfer").unwrap_or(&j);
        if t["id"]!=self.transfer_id||
           t["transfer_url"]!=self.transfer_url ||
           t["direction"].as_str().is_none_or(|x|!x.eq_ignore_ascii_case("download")) ||
           t["phase"].as_str().is_none_or(|x|!x.eq_ignore_ascii_case("transferring")) {
            return Err("oVirt transfer is not exact READY download ticket or endpoint changed".into());
        }
        let disk=t.pointer("/disk/id").and_then(Value::as_str)
            .ok_or("oVirt transfer disk identity not independently attested")?;
        if disk!=self.disk_id {
            return Err("oVirt transfer disk differs from declared VM disk snapshot".into());
        }
        Ok(())
    }
    fn evidence(&self)->Result<()>{
        self.validate()?;
        self.backup_ready()?;
        self.transfer_ready()?;
        Ok(())
    }
}
#[derive(Default)]
struct Proof{sha:Sha256,bytes:u64}
struct Checked<R:Read>{source:R,proof:Rc<RefCell<Proof>>,max:u64}
impl<R:Read> Read for Checked<R>{
    fn read(&mut self,b:&mut [u8])->io::Result<usize>{
        let n=self.source.read(b)?;
        let mut p=self.proof.borrow_mut();
        p.bytes=p.bytes.checked_add(n as u64)
            .ok_or_else(||io::Error::other("oVirt transfer overflow"))?;
        if p.bytes>self.max{return Err(io::Error::other("oVirt IMAGEIO oversized body"));}
        p.sha.update(&b[..n]);Ok(n)
    }
}
pub fn backup(config:&Config,root:&Path,identity:&Path,label:&str)->Result<LogicalBackup>{
    config.evidence()?;
    if label.trim().is_empty()||label.len()>256{
        return Err("oVirt image input requires a non-empty label".into());
    }
    let store=Store::open(root,identity)?;
    let url=check_url(&config.transfer_url,false)?;
    let client=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300)).build();
    // Preissued IMAGEIO URL is a short-lived ticket; DO NOT forward the
    // Engine Bearer token to any imageio host or through redirects.
    let response=client.get(url.as_str()).set("Accept","application/octet-stream")
        .call().map_err(|_|"oVirt imageio HTTPS GET refused or untrusted")?;
    if response.status()!=200{return Err("oVirt IMAGEIO transfer requires HTTP 200".into());}
    let proof=Rc::new(RefCell::new(Proof::default()));
    let check=Rc::clone(&proof);
    let mut reader=Checked{source:response.into_reader(),proof,
                           max:config.expected_bytes};
    let digest=config.expected_sha256.to_ascii_lowercase();
    let logical=format!("ovirt/vm/{}/backup/{}/{}.{}",
                       config.vm_id,config.backup_id,config.disk_id,config.expected_format);
    let result=store.backup_stream(identity,label,"",&logical,&mut reader,
        BackupKind::Full,None,||{
            let p=check.borrow();
            if p.bytes!=config.expected_bytes ||
               hex::encode(p.sha.clone().finalize())!=digest{
                return Err("oVirt IMAGEIO content length or independent SHA256 mismatch".into());
            }
            drop(p);
            config.evidence().map_err(|_|"oVirt backup/transfer evidence changed midstream".to_owned())?;
            Ok(())
        })?;
    if !store.verify(identity,&result.id)?.healthy{
        return Err("oVirt encrypted chunk-store integrity verification failed".into());
    }
    Ok(result)
}
#[cfg(test)]
mod tests{
    use super::*;
    const U:&str="12345678-1234-1234-1234-123456789abc";
    fn conf()->Config{
        Config{engine_endpoint:"https://ovirt-engine.example.test/".into(),
            bearer_token:"opaque-read-only-ticket-value".into(),
            vm_id:U.into(),backup_id:U.into(),disk_id:U.into(),transfer_id:U.into(),
            transfer_url:format!("https://imageio.example.test/images/{U}"),
            expected_bytes:128,expected_format:"raw".into(),
            expected_sha256:"a".repeat(64)}
    }
    #[test]fn exact_https_readonly_vendor_identity_required(){
        let mut c=conf();assert!(c.validate().is_ok());
        c.transfer_url="http://127.0.0.1/images/12345678-1234-1234-1234-123456789abc".into();
        assert!(c.validate().is_err());
        c=conf();c.vm_id="virtual-machine".into();assert!(c.validate().is_err());
        c=conf();c.transfer_url="https://imageio.example.test/images/not-uuid".into();
        assert!(c.validate().is_err());
        c=conf();c.transfer_url="https://imageio.example.test/images/12345678-1234-1234-1234-123456789abc?token=bad".into();
        assert!(c.validate().is_err());
    }
    #[test]fn source_hash_and_byte_limit_are_mandatory(){
        let evidence=Rc::new(RefCell::new(Proof::default()));
        let mut reader=Checked{source:&b"some bytes"[..],proof:evidence,max:4};
        let mut buf=Vec::new();
        assert!(reader.read_to_end(&mut buf).is_err());
        let mut c=conf();c.expected_sha256.clear();assert!(c.validate().is_err());
        c=conf();c.expected_bytes=0;assert!(c.validate().is_err());
        c=conf();c.expected_format="vmdk".into();assert!(c.validate().is_err());
    }
}
