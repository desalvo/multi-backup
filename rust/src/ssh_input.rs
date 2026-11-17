//! SSH B-In: pinned OpenSSH host key, no shell interpolation or plaintext
//! staging. Read one attested immutable remote file into age-encrypted chunks.
use crate::chunk_store::{BackupKind,LogicalBackup,Store};
use serde::Deserialize;
use sha2::{Digest,Sha256};
use std::{cell::RefCell,fs,io::{self,Read},path::{Path,PathBuf},
          process::{Command,Stdio},rc::Rc};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Deserialize,Debug)]
#[serde(deny_unknown_fields)]
pub struct Config{
    pub host:String,
    pub port:u16,
    pub user:String,
    pub identity_file:PathBuf,
    pub known_hosts_file:PathBuf,
    pub remote_file:String,
    pub expected_sha256:String,
    pub expected_bytes:u64,
}
fn simple_id(s:&str)->bool {
    !s.is_empty()&&s.len()<=253&&s.as_bytes()[0].is_ascii_alphanumeric()&&
      s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'.'||b==b'_')
}
fn safe_absolute_remote(s:&str)->bool{
    s.starts_with('/')&&s.len()<1024&&s.len()>1&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||
                  matches!(b,b'/'|b'.'|b'_'|b'-')) &&
    s.split('/').skip(1).all(|c|!c.is_empty() && c!="." && c!="..")
}
fn regular_file(path:&Path)->Result<()>{
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()||meta.file_type().is_symlink()||meta.len()==0{
        return Err("Pinned SSH requires a nonempty ordinary secret/known-hosts file".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode()&0o077!=0 {
            return Err("SSH private key and pinned known_hosts must be 0600".into());
        }
    }
    Ok(())
}
impl Config {
    pub fn load(path:&Path)->Result<Self>{
        let data=zeroize::Zeroizing::new(crate::protected_config::read_owner_only(path,16_384)?);
        let c:Config=serde_json::from_slice(&data)?;
        c.validate()?;
        Ok(c)
    }
    fn validate(&self)->Result<()> {
        if !simple_id(&self.host)||!simple_id(&self.user) ||
           self.port==0 || self.expected_bytes==0 ||
           self.expected_bytes>1_099_511_627_776 ||
           !safe_absolute_remote(&self.remote_file) ||
           self.expected_sha256.len()!=64 ||
           !self.expected_sha256.bytes().all(|b|b.is_ascii_hexdigit()){
            return Err("Invalid pinned SSH endpoint/path/SHA256/length".into());
        }
        if !self.identity_file.is_absolute()||!self.known_hosts_file.is_absolute() {
            return Err("Pinned SSH key and known_hosts paths must be absolute".into());
        }
        regular_file(&self.identity_file)?;
        regular_file(&self.known_hosts_file)?;
        Ok(())
    }
}
struct Evidence {sha:Sha256, bytes:u64}
struct GuardedReader<R:Read> {source:R, proof:Rc<RefCell<Evidence>>,limit:u64}
impl<R:Read> Read for GuardedReader<R>{
    fn read(&mut self,buffer:&mut [u8])->io::Result<usize>{
        let n=self.source.read(buffer)?;
        let mut check=self.proof.borrow_mut();
        check.bytes=check.bytes.checked_add(n as u64)
            .ok_or_else(||io::Error::other("SSH byte count overflow"))?;
        if check.bytes>self.limit{return Err(io::Error::other("SSH source exceeded pinned size"));}
        check.sha.update(&buffer[..n]);
        Ok(n)
    }
}
pub fn backup_file(config:&Config,root:&Path,identity:&Path,label:&str,
                   namespace:&str)->Result<LogicalBackup>{
    config.validate()?;
    if label.is_empty()||label.len()>256 {
        return Err("SSH B-In requires a label".into());
    }
    let store=Store::open(root,identity)?;
    let destination=format!("{}@{}",config.user,config.host);
    let mut child=Command::new("ssh")
        .args(["-T","-p",&config.port.to_string(),"-i"])
        .arg(&config.identity_file)
        .args(["-o","BatchMode=yes",
               "-o","StrictHostKeyChecking=yes",
               "-o","IdentitiesOnly=yes",
               "-o","PreferredAuthentications=publickey",
               "-o","PasswordAuthentication=no",
               "-o","NumberOfPasswordPrompts=0",
               "-o","ConnectTimeout=15",
               "-o","ServerAliveInterval=15",
               "-o","ServerAliveCountMax=3",
               "-o","ForwardAgent=no",
               "-o","ForwardX11=no",
               "-o","ClearAllForwardings=yes",
               "-o","PermitLocalCommand=no",
               "-o","ControlMaster=no",
               "-o","LogLevel=ERROR",
               "-o","GlobalKnownHostsFile=/dev/null",
               "-o"])
        .arg(format!("UserKnownHostsFile={}",config.known_hosts_file.display()))
        .arg(destination)
        .arg(format!("cat -- {}",config.remote_file))
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn()?;
    let mut stdout=child.stdout.take().ok_or("SSH stdout pipe missing")?;
    let proof=Rc::new(RefCell::new(Evidence{sha:Sha256::new(),bytes:0}));
    let check=Rc::clone(&proof);
    let expected=config.expected_sha256.to_ascii_lowercase();
    let length=config.expected_bytes;
    let mut reader=GuardedReader{source:&mut stdout,proof,limit:length};
    let logical=format!("ssh/{}/{}/{}",config.host,config.user,
                       config.remote_file.trim_start_matches('/'));
    let result=store.backup_stream(identity,label,namespace,&logical,&mut reader,
        BackupKind::Full,None,||{
            let status=child.wait().map_err(|_|"SSH child wait failed")?;
            if !status.success(){return Err(format!("SSH server command did not succeed (exit={:?})",status.code()));}
            let data=check.borrow();
            if data.bytes!=length{return Err("SSH stream incomplete".into());}
            if hex::encode(data.sha.clone().finalize())!=expected{
                return Err("SSH source SHA-256 differs from caller's independent digest".into());
            }
            Ok(())
        });
    if result.is_err(){let _=child.kill();let _=child.wait();}
    let snapshot=result?;
    if !store.verify(identity,&snapshot.id)?.healthy {
        return Err("SSH encrypted source chunk verification failed".into());
    }
    Ok(snapshot)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn untrusted_remote_commands_cannot_be_shell_injected(){
        for bad in ["/a;touch /tmp/x","/a b","/../../a","/a//b",
                    "/a'b","/a$(whoami)","/a\nb","a/path","/"]{
            assert!(!safe_absolute_remote(bad),"{bad:?}");
        }
        assert!(safe_absolute_remote("/datasets/snap-01/file.bin"));
        assert!(!simple_id("-evil"));
        assert!(!simple_id("user@other"));
        assert!(simple_id("ci_user"));
    }
    #[test] fn invalid_or_missing_host_keys_fail_before_network(){
        let tmp=tempfile::tempdir().unwrap();
        let bad=Config{host:"localhost".into(),port:22,user:"ci".into(),
            identity_file:tmp.path().join("key"),known_hosts_file:tmp.path().join("known"),
            remote_file:"/source.tar".into(),expected_sha256:"a".repeat(64),expected_bytes:10};
        assert!(bad.validate().is_err());
    }
    #[test] fn reader_rejects_stream_larger_than_independent_length(){
        let p=Rc::new(RefCell::new(Evidence{sha:Sha256::new(),bytes:0}));
        let mut r=GuardedReader{source:&b"abcdef"[..],proof:p,limit:5};
        let mut out=Vec::new();
        assert!(r.read_to_end(&mut out).is_err());
    }
}
