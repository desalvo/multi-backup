//! Native SSH/SCP B-Out: encrypted archives, strict pinned host key,
//! create-only remote reservation and independent remote SHA256 readback.
//! Never runs remote arbitrary user-provided command text or SCP overwrite.
//! This does NOT claim SFTP-server-only support; requires restricted POSIX shell.
use crate::{archive,protected_config};
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{fs::{self,File,OpenOptions},io::{Read,Write},
          path::{Path,PathBuf},process::{Command,Stdio}};
use tempfile::TempDir;
use zeroize::Zeroizing;
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config{
    pub host:String,pub port:u16,pub user:String,
    pub identity_file:PathBuf,pub known_hosts_file:PathBuf,
    pub remote_directory:String
}
#[derive(Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub remote_directory:String,
    pub archive:String,pub bytes:u64,pub sha256:String,
    pub encrypted:bool,pub create_only:bool,
    pub independent_remote_readback:bool,pub restored_members:usize
}
fn ident(s:&str,max:usize)->bool{
    !s.is_empty()&&s.len()<=max&&s.as_bytes()[0].is_ascii_alphanumeric()&&
    s.bytes().all(|c|c.is_ascii_alphanumeric()||matches!(c,b'.'|b'_'|b'-'))
}
fn path(s:&str)->bool{
    s.starts_with('/')&&s.len()>1&&s.len()<1024&&
    s.split('/').skip(1).all(|x|ident(x,128)) &&
    !s.contains("//")&&!s.contains("..")
}
fn archive_name(s:&str)->bool{
    s.ends_with(".tar.gz.age")&&s.len()>11&&s.len()<=180&&
    !s.contains("..")&&ident(&s[..s.len()-".tar.gz.age".len()],160)
}
fn secret(path:&Path)->Result<()>{
    let meta=fs::symlink_metadata(path)?;
    if !path.is_absolute()||!meta.file_type().is_file()||meta.file_type().is_symlink()||
       meta.len()==0 {
        return Err("SSH source secrets must be existing regular absolute files".into());
    }
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode()&0o077!=0{
            return Err("SSH private key and known_hosts must be 0600".into());
        }
    }
    Ok(())
}
impl Config{
    pub fn load(path:&Path)->Result<Self>{
        let b=Zeroizing::new(protected_config::read_owner_only(path,16_384)?);
        let c:Self=serde_json::from_slice(&b)?;
        c.validate()?;Ok(c)
    }
    fn validate(&self)->Result<()>{
        if !ident(&self.host,253)||!ident(&self.user,64)||self.port==0||
           !path(&self.remote_directory){
            return Err("SSH B-Out requires pinned host, user, port and safe absolute base directory".into());
        }
        secret(&self.identity_file)?;
        secret(&self.known_hosts_file)?;
        Ok(())
    }
    fn ssh(&self,remote:&str)->Result<Command>{
        self.validate()?;
        let mut cmd=Command::new("ssh");
        cmd.arg("-T").arg("-p").arg(self.port.to_string())
            .arg("-i").arg(&self.identity_file);
        for flag in ["BatchMode=yes","StrictHostKeyChecking=yes",
          "IdentitiesOnly=yes","PreferredAuthentications=publickey",
          "PasswordAuthentication=no","NumberOfPasswordPrompts=0",
          "ConnectTimeout=15","ServerAliveInterval=15","ServerAliveCountMax=3",
          "ForwardAgent=no","ForwardX11=no","ClearAllForwardings=yes",
          "PermitLocalCommand=no","ControlMaster=no","LogLevel=ERROR",
          "GlobalKnownHostsFile=/dev/null"]{
            cmd.arg("-o").arg(flag);
        }
        cmd.arg("-o").arg(format!("UserKnownHostsFile={}",self.known_hosts_file.display()));
        cmd.arg(format!("{}@{}",self.user,self.host)).arg(remote);
        cmd.stderr(Stdio::null()).stdout(Stdio::piped());
        Ok(cmd)
    }
    fn new_path(&self,name:&str)->Result<(String,String,String)>{
        if !archive_name(name){return Err("Unsafe SSH ciphertext archive filename".into());}
        let dirname=&name[..name.len()-".tar.gz.age".len()];
        let folder=format!("{}/{}",self.remote_directory,dirname);
        let ciphertext=format!("{folder}/{name}");
        let marker=format!("{ciphertext}.sha256");
        Ok((folder,ciphertext,marker))
    }
}
fn run(c:&Config,cmd:&str)->Result<()>{
    let state=c.ssh(cmd)?.stdin(Stdio::null()).output()?;
    if !state.status.success(){return Err("Remote SSH operation failed; no overwrite fallback".into());}
    Ok(())
}
fn upload(c:&Config,destination:&str,source:&Path)->Result<()>{
    let mut ssh=c.ssh(&format!("set -Ceu; umask 077; cat > {destination}"))?;
    let mut proc=ssh.stdin(Stdio::piped()).spawn()?;
    let input=File::open(source)?;
    let put=(||->Result<()>{
        let stream=proc.stdin.as_mut().ok_or("SSH upload pipe absent")?;
        let mut reader=input;
        std::io::copy(&mut reader,stream)?;
        Ok(())
    })();
    drop(proc.stdin.take());
    if put.is_err(){let _=proc.kill();let _=proc.wait();return put;}
    if !proc.wait()?.success(){return Err("SSH remote create-only upload failed".into());}
    Ok(())
}
fn readback(c:&Config,source:&str,max:u64)->Result<(u64,String)>{
    let mut child=c.ssh(&format!("set -eu; cat {source}"))?.stdin(Stdio::null()).spawn()?;
    let result=(||->Result<(u64,String)>{
        let mut stdout=child.stdout.take().ok_or("SSH GET pipe missing")?;
        let mut sha=Sha256::new();let mut total=0u64;let mut buf=[0u8;65536];
        loop{
            let n=stdout.read(&mut buf)?;
            if n==0{break;}
            total=total.checked_add(n as u64).ok_or("Remote readback size overflow")?;
            if total>max{return Err("Remote SSH archive exceeded pinned size".into());}
            sha.update(&buf[..n]);
        }
        Ok((total,hex::encode(sha.finalize())))
    })();
    if result.is_err(){let _=child.kill();}
    let status=child.wait()?;
    if !status.success(){return Err("SSH independent remote GET failed".into());}
    result
}
fn hash_file(file:&Path)->Result<(u64,String)>{
    let mut sha=Sha256::new();let mut input=File::open(file)?;
    let mut total=0u64;let mut buf=[0u8;65536];
    loop{let n=input.read(&mut buf)?;if n==0{break;}
        total=total.checked_add(n as u64).ok_or("Encrypted archive byte count overflow")?;
        sha.update(&buf[..n]);}
    Ok((total,hex::encode(sha.finalize())))
}
pub fn backup(c:&Config,source:&Path,name:&str,recipient:&str,identity:&Path)->Result<Receipt>{
    c.validate()?;let (folder,ciphertext,marker)=c.new_path(name)?;
    let temp=TempDir::new()?;
    let local=temp.path().join(name);
    let members=archive::create(source,&local,recipient,"multi-backup/ssh-native")?;
    if archive::verify(&local,identity)?!=members{
        return Err("SSH B-Out local ciphertext identity verification failed".into());
    }
    let (size,sha)=hash_file(&local)?;
    // Atomically reserve ONE NEW remote directory. No --force, no rm.
    // This fails if another backup already owns the basename.
    run(c,&format!("set -eu; umask 077; test -d {} && test ! -L {}; mkdir -m 0700 {}",
       c.remote_directory,c.remote_directory,folder))?;
    upload(c,&ciphertext,&local)?;
    let (remote_size,remote_sha)=readback(c,&ciphertext,size)?;
    if size!=remote_size||sha!=remote_sha{
        return Err("Remote SSH independently downloaded ciphertext differs".into());
    }
    let side=PathBuf::from(format!("{}.sha256",local.display()));
    // archive::create already produces the authoritative SHA sidecar.
    // Never truncate or recreate it after local ciphertext verification.
    let side_text=fs::read_to_string(&side)?;
    if side_text.split_whitespace().next()!=Some(&sha){
        return Err("SSH upload sidecar did not match independently hashed ciphertext".into());
    }
    upload(c,&marker,&side)?;
    let (m_size,m_sha)=hash_file(&side)?;
    if readback(c,&marker,m_size)?!=(m_size,m_sha){
        return Err("Remote SSH checksum marker failed independent readback".into());
    }
    Ok(Receipt{provider:"ssh",remote_directory:folder,archive:name.into(),
        bytes:size,sha256:sha,encrypted:true,create_only:true,
        independent_remote_readback:true,restored_members:members})
}
pub fn restore(c:&Config,name:&str,identity:&Path,destination:&Path)->Result<usize>{
    c.validate()?;let (_,ciphertext,marker)=c.new_path(name)?;
    if !destination.is_absolute()||fs::symlink_metadata(destination).is_ok(){
        return Err("SSH encrypted recovery requires NEW absolute destination".into());
    }
    let temp=TempDir::new()?;let local=temp.path().join(name);
    let bytes_from_marker={
        let mut proc=c.ssh(&format!("set -eu; cat {marker}"))?.stdin(Stdio::null()).spawn()?;
        let mut result=Vec::new();
        let collected=proc.stdout.take().ok_or("SSH marker pipe missing")?
            .take(4097).read_to_end(&mut result);
        let status=proc.wait()?;
        collected?;
        if !status.success()||result.len()>4096{return Err("No verified remote marker".into());}
        result
    };
    let str_marker=String::from_utf8(bytes_from_marker)?;
    let expected=str_marker.split_whitespace().next().ok_or("Missing SSH marker SHA256")?;
    if expected.len()!=64||!expected.bytes().all(|b|b.is_ascii_hexdigit()){
        return Err("Invalid SSH marker format".into());
    }
    let mut proc=c.ssh(&format!("set -eu; cat {ciphertext}"))?.stdin(Stdio::null()).spawn()?;
    let mut dest=OpenOptions::new().write(true).create_new(true).open(&local)?;
    let count=std::io::copy(&mut proc.stdout.take().ok_or("SSH GET pipe missing")?
        .take(1_099_511_627_777),&mut dest)?;
    dest.sync_all()?;drop(dest);
    if count>1_099_511_627_776||!proc.wait()?.success(){
        return Err("SSH remote archive read failed or exceeded size limit".into());
    }
    let (_,sha)=hash_file(&local)?;
    if sha!=expected{return Err("SSH remote marker failed SHA256 integrity".into());}
    let side=PathBuf::from(format!("{}.sha256",local.display()));
    let mut local_side=OpenOptions::new().write(true).create_new(true).open(&side)?;
    local_side.write_all(str_marker.as_bytes())?;
    local_side.sync_all()?;
    drop(local_side);
    let members=archive::verify(&local,identity)?;
    let entries=archive::extract(&local,identity,destination)?;
    if members!=entries.len(){return Err("SSH restore verified member count mismatch".into());}
    Ok(members)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn unsafe_paths_and_names_fail_closed(){
        for v in ["/","/tmp/../evil","/tmp/path space","tmp/dir","/tmp/a//b"]{assert!(!path(v));}
        assert!(path("/home/mbci/backup"));
        for v in ["../foo.tar.gz.age","/etc.tar.gz.age","foo.sh",
                  "foo..tar.gz.age","a/b.tar.gz.age"]{assert!(!archive_name(v));}
        assert!(archive_name("nightly-001.tar.gz.age"));
    }
}
