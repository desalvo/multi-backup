//! Direct HTTPS/WebDAV ciphertext B-Out and restore input. This avoids
//! davfs2 delayed writeback while preserving no-overwrite semantics.
//! Only age ciphertext is uploaded; no raw user files or passwords in logs.
use crate::archive;
use base64::Engine as _;
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{fs::{self,File,OpenOptions},io::{Read,Write},
          path::{Path,PathBuf},time::Duration};
use tempfile::TempDir;
use ureq::{Agent,AgentBuilder};
use url::Url;
use zeroize::Zeroizing;
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub endpoint:String,
    pub username:String,
}
#[derive(Debug,Serialize)]
pub struct Receipt{
    pub provider:&'static str,
    pub remote_file:String,
    pub bytes:u64,
    pub sha256:String,
    pub encrypted:bool,
    pub create_only:bool,
    pub independent_https_readback:bool,
    pub verified_members:usize,
}
fn basename(name:&str)->Result<()>{
    if !name.ends_with(".tar.gz.age") || name.len()<=11 || name.len()>200 ||
       name.starts_with('.') || name.contains("..") ||
       !name.bytes().all(|b|b.is_ascii_alphanumeric() ||
                          matches!(b,b'.'|b'-'|b'_')){
        return Err("Unsafe flat WebDAV ciphertext archive filename".into());
    }
    Ok(())
}
impl Config {
    pub fn load(path:&Path)->Result<Self>{
        let data=Zeroizing::new(crate::protected_config::read_owner_only(path,16_384)?);
        let c:Self=serde_json::from_slice(&data)?;
        c.check()?;
        Ok(c)
    }
    fn check(&self)->Result<()>{
        let url=Url::parse(&self.endpoint)?;
        if url.scheme()!="https" || url.host_str().is_none() ||
           !url.username().is_empty() || url.password().is_some() ||
           url.query().is_some() || url.fragment().is_some() ||
           !url.path().ends_with('/') ||
           url.path().contains("/../") || url.path().contains("/./") {
            return Err("WebDAV requires bare trusted HTTPS collection URL ending in /".into());
        }
        if self.username.is_empty() || self.username.len()>128 ||
           !self.username.bytes().all(|c|c.is_ascii_alphanumeric()||
             matches!(c,b'.'|b'_'|b'-'|b'@')) {
            return Err("Invalid WebDAV Basic username".into());
        }
        Ok(())
    }
    fn target(&self,name:&str)->Result<String>{
        if !name.ends_with(".tar.gz.age") && !name.ends_with(".tar.gz.age.sha256") {
            return Err("Only encrypted WebDAV archives and checksum markers allowed".into());
        }
        let mut url=Url::parse(&self.endpoint)?;
        let mut parts=url.path_segments_mut()
            .map_err(|_|"Cannot append safe filename to WebDAV collection")?;
        parts.pop_if_empty().push(name);
        drop(parts);
        Ok(url.to_string())
    }
}
struct Client{
    agent:Agent,
    config:Config,
    auth:Zeroizing<String>,
}
impl Client {
    fn new(config:Config)->Result<Self>{
        config.check()?;
        let pass=Zeroizing::new(std::env::var("MULTI_BACKUP_WEBDAV_PASSWORD")?);
        if pass.is_empty() || pass.len()>4096 ||
           pass.bytes().any(|c|c==b'\r'||c==b'\n') {
            return Err("Missing or invalid WebDAV password environment variable".into());
        }
        let pair=Zeroizing::new(format!("{}:{}",config.username,pass.as_str()));
        let token=Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
        Ok(Self{
            agent:AgentBuilder::new().timeout_connect(Duration::from_secs(20))
                .timeout_read(Duration::from_secs(180))
                .timeout_write(Duration::from_secs(180))
                .redirects(0).build(),
            config,auth:Zeroizing::new(format!("Basic {}",token.as_str()))
        })
    }
    fn request(&self,method:&str,name:&str)->Result<ureq::Request>{
        let url=self.config.target(name)?;
        if !["HEAD","GET","PUT"].contains(&method){
            return Err("Only preflight HEAD, verification GET and create-only PUT allowed".into());
        }
        Ok(self.agent.request(method,&url)
            .set("Authorization",self.auth.as_str()))
    }
    fn assert_absent(&self,name:&str)->Result<()>{
        match self.request("HEAD",name)?.call(){
            Err(ureq::Error::Status(404,_))=>Ok(()),
            Ok(_)=>Err("WebDAV remote ciphertext/marker already exists".into()),
            Err(ureq::Error::Status(status,_))=>Err(format!("WebDAV HEAD rejected with HTTP status {status}; no write attempted").into()),
            Err(ureq::Error::Transport(failure))=>Err(format!("WebDAV HEAD TLS/transport failure ({:?}: {}); no write attempted",failure.kind(),failure.message().unwrap_or("no detailed reason")).into())
        }
    }
    fn put_create_only(&self,name:&str,source:&Path)->Result<()>{
        self.assert_absent(name)?;
        let meta=fs::symlink_metadata(source)?;
        if !meta.file_type().is_file() || meta.file_type().is_symlink() ||
           meta.len()==0 {
            return Err("WebDAV only uploads pre-generated ciphertext files".into());
        }
        let size=meta.len();
        let response=self.request("PUT",name)?
            .set("Content-Type","application/octet-stream")
            .set("Content-Length",&size.to_string())
            .set("If-None-Match","*")
            .send(File::open(source)?);
        match response{
            Ok(r) if r.status()==201=>Ok(()),
            Ok(_)=>Err("WebDAV did not confirm creation of a NEW object; outcome requires inspection".into()),
            Err(ureq::Error::Status(412,_))=>Err("WebDAV precondition rejected overwrite".into()),
            Err(_)=>Err("WebDAV upload may have committed; inspect target before retry".into())
        }
    }
    fn readback_sha(&self,name:&str,expected_len:u64,expected_sha:&str)->Result<()>{
        let response=self.request("GET",name)?.call()
            .map_err(|_|"Independent HTTPS/WebDAV GET failed")?;
        if response.status()!=200 {
            return Err("Independent HTTPS/WebDAV GET did not return 200".into());
        }
        let mut reader=response.into_reader();
        let mut hasher=Sha256::new();
        let mut buf=[0u8;65536];
        let mut total=0u64;
        loop{
            let n=reader.read(&mut buf)?;
            if n==0{break;}
            total=total.checked_add(n as u64).ok_or("WebDAV GET length overflow")?;
            if total>expected_len {
                return Err("Remote WebDAV ciphertext is larger than uploaded source".into());
            }
            hasher.update(&buf[..n]);
        }
        if total!=expected_len ||
           hex::encode(hasher.finalize())!=expected_sha{
            return Err("Remote WebDAV HTTPS ciphertext length/hash differ from source".into());
        }
        Ok(())
    }
    fn get_into(&self,name:&str,path:&Path,maximum:u64)->Result<()> {
        let response=self.request("GET",name)?.call()
            .map_err(|_|"WebDAV encrypted archive GET rejected")?;
        if response.status()!=200 {return Err("WebDAV GET must return HTTP 200".into());}
        let mut reader=response.into_reader();
        let mut dest=OpenOptions::new().write(true).create_new(true).open(path)?;
        let bytes=std::io::copy(&mut reader.take(maximum+1),&mut dest)?;
        if bytes>maximum{return Err("WebDAV encrypted download exceeds limit".into());}
        dest.flush()?;
        dest.sync_all()?;
        Ok(())
    }
}
fn sha256_file(path:&Path)->Result<(u64,String)>{
    let mut file=File::open(path)?;
    let mut hash=Sha256::new();
    let mut len=0u64;
    let mut buf=[0u8;65536];
    loop{
        let n=file.read(&mut buf)?;
        if n==0{break;}
        len=len.checked_add(n as u64).ok_or("Encrypted file length overflow")?;
        hash.update(&buf[..n]);
    }
    Ok((len,hex::encode(hash.finalize())))
}
fn temp_archive(path:&Path)->Result<(TempDir,PathBuf)>{
    basename(path.to_str().ok_or("WebDAV filename is not UTF8")?)?;
    let temp=tempfile::Builder::new().prefix("mb-webdav-age-").tempdir()?;
    let local=temp.path().join(path);
    Ok((temp,local))
}
pub fn backup_directory(config:Config,source:&Path,filename:&str,
    recipient:&str,identity:&Path)->Result<Receipt>{
    basename(filename)?;
    let client=Client::new(config)?;
    // Fail early before locally encrypting sensitive sources.
    let side_name=format!("{filename}.sha256");
    client.assert_absent(filename)?;
    client.assert_absent(&side_name)?;
    let (_temporary,local)=temp_archive(Path::new(filename))?;
    let members=archive::create(source,&local,recipient,"multi-backup/webdav-native")?;
    if archive::verify(&local,identity)?!=members{
        return Err("Local WebDAV age ciphertext cannot be authenticated".into());
    }
    let side=PathBuf::from(format!("{}.sha256",local.display()));
    let (bytes,sha)=sha256_file(&local)?;
    let (side_bytes,side_sha)=sha256_file(&side)?;
    // Never upload the checksum marker before ciphertext is independently
    // readable via the origin's HTTPS endpoint (no davfs2 cache).
    client.put_create_only(filename,&local)?;
    client.readback_sha(filename,bytes,&sha)?;
    client.put_create_only(&side_name,&side)?;
    client.readback_sha(&side_name,side_bytes,&side_sha)?;
    Ok(Receipt{provider:"webdav",remote_file:filename.to_owned(),
        bytes,sha256:sha,encrypted:true,create_only:true,
        independent_https_readback:true,verified_members:members})
}
pub fn restore_archive(config:Config,filename:&str,identity:&Path,
    destination:&Path)->Result<usize>{
    basename(filename)?;
    if fs::symlink_metadata(destination).is_ok(){
        return Err("WebDAV restore refuses an existing destination".into());
    }
    let client=Client::new(config)?;
    let (temp,local)=temp_archive(Path::new(filename))?;
    let side_name=format!("{filename}.sha256");
    client.get_into(&side_name,&PathBuf::from(format!("{}.sha256",local.display())),4096)?;
    client.get_into(filename,&local,1_099_511_627_776)?;
    let bytes=archive::verify(&local,identity)?;
    let extracted=archive::extract(&local,identity,destination)?;
    if bytes!=extracted.len(){
        return Err("WebDAV recovered member count mismatched verified archive".into());
    }
    drop(temp);
    Ok(bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn reject_unsafe_destinations_and_cleartext(){
        for name in ["","/etc/passwd","../x.tar.gz.age","a//b.tar.gz.age",
            ".hidden.tar.gz.age","a.b.tar","a\\b.tar.gz.age",
            "bad\n.tar.gz.age","a..b.tar.gz.age"]{
            assert!(basename(name).is_err(),"{name:?}");
        }
        assert!(basename("ceph-2026_11.tar.gz.age").is_ok());
        let c=Config{endpoint:"https://backup.example.test/webdav/".into(),
            username:"ci-user".into()};
        assert!(c.check().is_ok());
        assert_eq!(c.target("backup.tar.gz.age").unwrap(),
                   "https://backup.example.test/webdav/backup.tar.gz.age");
    }
    #[test] fn reject_http_credentials_redirect_and_url_injection(){
        for endpoint in ["http://x.test/","https://user:pass@x.test/",
            "https://x.test/?token=123","https://x.test/no-trailing",
            "https://x.test/#fragment"]{
            assert!(Config{endpoint:endpoint.into(),username:"ci".into()}
              .check().is_err(),"{endpoint}");
        }
        assert!(Config{endpoint:"https://x.test/".into(),
            username:"ci:evil".into()}.check().is_err());
    }
}
