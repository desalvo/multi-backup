//! S3-compatible WORM vault transport (Ceph RGW / MinIO).
//! Explicit COMPLIANCE retention is mandatory. Credentials come from the
//! process environment, never from files, command arguments or log output.
//! No DeleteObject, DeleteObjects, PutObjectRetention or bucket mutation API.
use crate::archive;
use base64::Engine;
use md5::Md5;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, env, fs::{self, File}, io::{Read, Write},
          path::Path, time::Duration as Timeout};
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Hmac256 = Hmac<Sha256>;
const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const MAX_XML: u64 = 64 * 1024;
const MAX_MANIFEST: u64 = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub provider: String, // "ceph-rgw", "minio" or "rustfs"
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub prefix: String,
    pub recipient: String,
    pub retention_days: i64,
    #[serde(default)]
    pub allow_loopback_http_for_testing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema: u32,
    pub id: String,
    pub archive_key: String,
    pub archive_version: String,
    pub sha256: String,
    pub bytes: u64,
    pub files: usize,
    pub retain_until: String,
}

fn hex_sha(data: &[u8]) -> String { hex::encode(Sha256::digest(data)) }
fn hash_file(path: &Path) -> Result<(String,u64)> {
    let mut f = File::open(path)?;
    if !f.metadata()?.is_file() { return Err("Expected a regular encrypted file".into()); }
    let mut sha=Sha256::new();
    let mut bytes=0u64;
    let mut buffer=[0u8;128*1024];
    loop {
        let n=f.read(&mut buffer)?;
        if n==0 {break;}
        sha.update(&buffer[..n]);
        bytes=bytes.checked_add(n as u64).ok_or("File size overflow")?;
    }
    Ok((hex::encode(sha.finalize()),bytes))
}
fn content_md5_file(path:&Path)->Result<String> {
    let mut file=File::open(path)?;
    let mut hash=Md5::new();
    let mut buffer=[0u8;128*1024];
    loop {
        let n=file.read(&mut buffer)?;
        if n==0 {break;}
        hash.update(&buffer[..n]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(hash.finalize()))
}
fn encode(component: &str, keep_slash: bool) -> String {
    let mut encoded=String::new();
    for &c in component.as_bytes() {
        if c.is_ascii_alphanumeric() || b"-_.~".contains(&c) || (keep_slash && c==b'/') {
            encoded.push(c as char);
        } else { encoded.push_str(&format!("%{c:02X}")); }
    }
    encoded
}
fn xml_value<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open=format!("<{tag}>");
    let close=format!("</{tag}>");
    let start=xml.find(&open)?+open.len();
    let end=start+xml[start..].find(&close)?;
    Some(xml[start..end].trim())
}
fn valid_key(s: &str) -> bool {
    !s.is_empty() && s.len()<=900 && !s.starts_with('/')
        && !s.ends_with('/') && s.split('/').all(|p|!p.is_empty() && p!="." && p!="..")
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"/-_.!".contains(&c))
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let meta=fs::symlink_metadata(path)?;
        if !meta.file_type().is_file() || meta.file_type().is_symlink() || meta.len()>16_384 {
            return Err("S3 vault configuration must be a small regular non-symlink file".into());
        }
        let cfg: Self=serde_json::from_reader(File::open(path)?)?;
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.provider.as_str(),"ceph-rgw"|"minio"|"rustfs") {
            return Err("Provider must be ceph-rgw, minio or rustfs".into());
        }
        if self.region.is_empty() || self.region.len()>64
            || !self.region.bytes().all(|b|b.is_ascii_alphanumeric() || b"-".contains(&b)) {
            return Err("Unsafe S3 region".into());
        }
        if self.bucket.len()<3 || self.bucket.len()>63
            || !self.bucket.bytes().all(|b|b.is_ascii_lowercase() || b.is_ascii_digit() || b".-".contains(&b))
            || self.bucket.starts_with('.') || self.bucket.ends_with('.') {
            return Err("Unsafe S3 bucket name".into());
        }
        if !self.prefix.is_empty() && !valid_key(self.prefix.trim_end_matches('/')) {
            return Err("Unsafe S3 vault prefix".into());
        }
        if self.prefix.len()>500 || self.prefix.starts_with('/') || self.prefix.contains("//")
            || self.prefix.split('/').any(|p|p=="."||p=="..") {
            return Err("Unsafe S3 vault namespace".into());
        }
        if self.retention_days<1 || self.retention_days>36500 {
            return Err("S3 WORM retention must be 1..36500 days".into());
        }
        if !self.recipient.starts_with("age1") || self.recipient.len()<24
            || self.recipient.len()>128 || !self.recipient.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()) {
            return Err("Invalid public age recipient".into());
        }
        let parsed=Url::parse(&self.endpoint)?;
        let host=parsed.host_str().ok_or("S3 endpoint host is required")?;
        if !parsed.username().is_empty() || parsed.password().is_some()
            || parsed.query().is_some() || parsed.fragment().is_some()
            || parsed.path()!="/" {
            return Err("Endpoint must have only scheme, host and optional port".into());
        }
        match parsed.scheme() {
            "https" => (),
            "http" if self.allow_loopback_http_for_testing
                && ["127.0.0.1","localhost","[::1]"].contains(&host) => (),
            _ => return Err("HTTPS required; HTTP only on explicitly opted-in loopback tests".into()),
        }
        Ok(())
    }
    fn object_key(&self, suffix: &str) -> String {
        let p=self.prefix.trim_matches('/');
        if p.is_empty() { suffix.to_string() } else {format!("{p}/{suffix}")}
    }
}

struct Client {
    cfg: Config, access: String, secret: String, token: Option<String>,
}
impl Client {
    fn new(cfg: &Config) -> Result<Self> {
        cfg.validate()?;
        let access=env::var("AWS_ACCESS_KEY_ID")?;
        let secret=env::var("AWS_SECRET_ACCESS_KEY")?;
        if access.is_empty() || secret.is_empty() || access.bytes().any(|b|b==b'\n'||b==b'\r')
            || secret.bytes().any(|b|b==b'\n'||b==b'\r') {
            return Err("Missing or unsafe S3 credentials".into());
        }
        let token=env::var("AWS_SESSION_TOKEN").ok();
        if token.as_deref().is_some_and(|s|s.bytes().any(|b|b==b'\n'||b==b'\r')) {
            return Err("Unsafe S3 session token".into());
        }
        Ok(Self{cfg:cfg.clone(),access,secret,token})
    }
    fn sign(&self, method:&str, key:Option<&str>, query:&[(&str,&str)],
            extra:&[(&str,&str)], hash:&str) -> Result<ureq::Request> {
        let base=Url::parse(&self.cfg.endpoint)?;
        let host=base.host_str().ok_or("Missing S3 host")?;
        let host=if let Some(port)=base.port() {format!("{host}:{port}")} else {host.to_string()};
        let object=match key {Some(k)=>format!("/{}/{}",encode(&self.cfg.bucket,false),encode(k,true)),
                                None=>format!("/{}",encode(&self.cfg.bucket,false))};
        let mut q=query.iter().map(|(k,v)|(encode(k,false),encode(v,false)))
            .collect::<Vec<_>>();
        q.sort();
        let qs=q.iter().map(|(k,v)|format!("{k}={v}")).collect::<Vec<_>>().join("&");
        let url=format!("{}{}{}",self.cfg.endpoint.trim_end_matches('/'),object,
                        if qs.is_empty(){String::new()}else{format!("?{qs}")});
        let now=Utc::now();
        let day=now.format("%Y%m%d").to_string();
        let stamp=now.format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers=BTreeMap::new();
        headers.insert("host".to_string(),host);
        headers.insert("x-amz-date".to_string(),stamp.clone());
        headers.insert("x-amz-content-sha256".to_string(),hash.to_string());
        if let Some(t)=&self.token {headers.insert("x-amz-security-token".to_string(),t.clone());}
        for (k,v) in extra {
            if *k!="if-none-match" && *k!="content-md5" && !k.starts_with("x-amz-") {
                return Err("Only signed S3 security headers are allowed".into());
            }
            if v.bytes().any(|b|b==b'\n'||b==b'\r') {return Err("Unsafe S3 header".into());}
            headers.insert(k.to_string(),v.trim().to_string());
        }
        let signed=headers.keys().cloned().collect::<Vec<_>>().join(";");
        let canonical_headers=headers.iter().map(|(k,v)|format!("{k}:{v}\n")).collect::<String>();
        let request=format!("{method}\n{object}\n{qs}\n{canonical_headers}\n{signed}\n{hash}");
        let scope=format!("{day}/{}/s3/aws4_request",self.cfg.region);
        let to_sign=format!("AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",hex_sha(request.as_bytes()));
        fn mac(key:&[u8], data:&[u8])->Result<Vec<u8>> {
            let mut h=Hmac256::new_from_slice(key).map_err(|_|"Invalid HMAC key")?;
            h.update(data);
            Ok(h.finalize().into_bytes().to_vec())
        }
        let kdate=mac(format!("AWS4{}",self.secret).as_bytes(),day.as_bytes())?;
        let kregion=mac(&kdate,self.cfg.region.as_bytes())?;
        let kservice=mac(&kregion,b"s3")?;
        let ksign=mac(&kservice,b"aws4_request")?;
        let signature=hex::encode(mac(&ksign,to_sign.as_bytes())?);
        let auth=format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",self.access);
        let agent=ureq::AgentBuilder::new().timeout_connect(Timeout::from_secs(10))
            .timeout_read(Timeout::from_secs(120)).build();
        let mut req=agent.request(method,&url).set("Authorization",&auth);
        for (key,value) in headers {
            req=req.set(&key,&value);
        }
        Ok(req)
    }
    fn small_get(&self, key:Option<&str>, query:&[(&str,&str)], max:u64) -> Result<(String,String)> {
        let response=self.sign("GET",key,query,&[],EMPTY_HASH)?.call()?;
        let version=response.header("x-amz-version-id").unwrap_or("").to_string();
        let mut bytes=Vec::new();
        response.into_reader().take(max+1).read_to_end(&mut bytes)?;
        if bytes.len() as u64>max {return Err("S3 response exceeds allowed bounds".into());}
        Ok((String::from_utf8(bytes)?,version))
    }
    fn put_file(&self,key:&str,file:&Path,until:&str) -> Result<String> {
        let (hash,len)=hash_file(file)?;
        let md5=content_md5_file(file)?;
        let response=self.sign("PUT",Some(key),&[],
          &[("if-none-match","*"),("content-md5",&md5),("x-amz-object-lock-mode","COMPLIANCE"),
            ("x-amz-object-lock-retain-until-date",until),
            ("x-amz-meta-sha256",&hash)],&hash)?
            .set("Content-Length",&len.to_string())
            .send(File::open(file)?)?;
        let version=response.header("x-amz-version-id").unwrap_or("").to_string();
        if version.is_empty() || version=="null" {
            return Err("S3 upload did not return an immutable object version ID".into());
        }
        self.assert_locked(key,&version,until)?;
        Ok(version)
    }
    fn put_bytes(&self,key:&str,payload:&[u8],until:&str) -> Result<String> {
        let hash=hex_sha(payload);
        let md5=base64::engine::general_purpose::STANDARD.encode(Md5::digest(payload));
        let response=self.sign("PUT",Some(key),&[],
          &[("if-none-match","*"),("content-md5",&md5),("x-amz-object-lock-mode","COMPLIANCE"),
            ("x-amz-object-lock-retain-until-date",until),
            ("x-amz-meta-sha256",&hash)],&hash)?
            .set("Content-Length",&payload.len().to_string()).send_bytes(payload)?;
        let version=response.header("x-amz-version-id").unwrap_or("").to_string();
        if version.is_empty() || version=="null" {
            return Err("S3 manifest upload did not return an object version ID".into());
        }
        self.assert_locked(key,&version,until)?;
        Ok(version)
    }
    fn assert_locked(&self,key:&str,version:&str,requested:&str)->Result<()> {
        let (xml,_)=self.small_get(Some(key),&[("retention",""),("versionId",version)],MAX_XML)?;
        if xml_value(&xml,"Mode")!=Some("COMPLIANCE") {
            return Err("S3 object version is NOT compliance-locked".into());
        }
        let current=xml_value(&xml,"RetainUntilDate").ok_or("S3 retention expiry missing")?;
        let actual=DateTime::parse_from_rfc3339(current)?;
        let expected=DateTime::parse_from_rfc3339(requested)?;
        if actual<expected {return Err("S3 server retention shorter than requested".into());}
        Ok(())
    }
    fn download(&self,key:&str,version:Option<&str>,dest:&Path,max_bytes:u64) -> Result<()> {
        if fs::symlink_metadata(dest).is_ok() {return Err("Refusing to overwrite downloaded ciphertext".into());}
        let query=version.map(|v|vec![("versionId",v)]).unwrap_or_default();
        let response=self.sign("GET",Some(key),&query,&[],EMPTY_HASH)?.call()?;
        let mut out=fs::OpenOptions::new().write(true).create_new(true).open(dest)?;
        let mut src=response.into_reader().take(max_bytes.saturating_add(1));
        let copied=std::io::copy(&mut src,&mut out)?;
        out.sync_all()?;
        if copied!=max_bytes {
            return Err("S3 response length differs from immutable manifest".into());
        }
        Ok(())
    }
    fn preflight(&self) -> Result<()> {
        let (versioning,_)=self.small_get(None,&[("versioning","")],MAX_XML)?;
        if xml_value(&versioning,"Status")!=Some("Enabled"){
            return Err("Bucket versioning must be Enabled".into());
        }
        let (locking,_)=self.small_get(None,&[("object-lock","")],MAX_XML)?;
        if xml_value(&locking,"ObjectLockEnabled")!=Some("Enabled")
            || xml_value(&locking,"Mode")!=Some("COMPLIANCE"){
            return Err("Bucket must have Object Lock with COMPLIANCE default retention".into());
        }
        Ok(())
    }
}

pub fn preflight(config:&Config)->Result<()> { Client::new(config)?.preflight() }

pub fn backup(config:&Config,source:&Path)->Result<Record> {
    let client=Client::new(config)?;
    client.preflight()?;
    let meta=fs::symlink_metadata(source)?;
    if !meta.file_type().is_dir() || meta.file_type().is_symlink(){
        return Err("S3 vault backup source must be a real directory".into());
    }
    let mut nonce=[0u8;16];
    OsRng.fill_bytes(&mut nonce);
    let id=hex::encode(nonce);
    let temp=tempfile::tempdir()?;
    let output=temp.path().join(format!("{id}.tar.gz.age"));
    let files=archive::create(source,&output,&config.recipient,&format!("multi-backup/s3-vault/{id}"))?;
    let (digest,bytes)=hash_file(&output)?;
    let until=(Utc::now()+Duration::days(config.retention_days))
        .to_rfc3339_opts(SecondsFormat::Secs,true);
    let archive_key=config.object_key(&format!("snapshots/{id}.tar.gz.age"));
    let version=client.put_file(&archive_key,&output,&until)?;
    let record=Record{schema:1,id:id.clone(),archive_key,archive_version:version,
                      sha256:digest,bytes,files,retain_until:until.clone()};
    // Manifest LAST: an interrupted upload creates only a locked orphan,
    // never a visible completed backup.
    let manifest_key=config.object_key(&format!("manifests/{id}.json"));
    client.put_bytes(&manifest_key,&serde_json::to_vec(&record)?,&until)?;
    Ok(record)
}
fn read_record(client:&Client,id:&str)->Result<Record>{
    if id.len()!=32 || !id.bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase()){
        return Err("Invalid S3 vault snapshot ID".into());
    }
    client.preflight()?;
    let key=client.cfg.object_key(&format!("manifests/{id}.json"));
    let (raw,version)=client.small_get(Some(&key),&[],MAX_MANIFEST)?;
    if version.is_empty() || version=="null" {return Err("Manifest is not versioned".into());}
    let record:Record=serde_json::from_str(&raw)?;
    if record.schema!=1 || record.id!=id
        || record.archive_key!=client.cfg.object_key(&format!("snapshots/{id}.tar.gz.age"))
        || record.sha256.len()!=64 || !record.sha256.bytes().all(|b|b.is_ascii_hexdigit())
        || record.archive_version.is_empty() || record.archive_version=="null"
        || record.bytes==0 {
        return Err("Unsafe or inconsistent S3 vault manifest".into());
    }
    client.assert_locked(&key,&version,&record.retain_until)?;
    client.assert_locked(&record.archive_key,&record.archive_version,&record.retain_until)?;
    Ok(record)
}
fn staged(client:&Client,record:&Record,where_to:&Path)->Result<std::path::PathBuf> {
    let output=where_to.join(format!("{}.tar.gz.age",record.id));
    client.download(&record.archive_key,Some(&record.archive_version),&output,record.bytes)?;
    let (actual,bytes)=hash_file(&output)?;
    if actual!=record.sha256 || bytes!=record.bytes{
        return Err("S3 restored ciphertext digest or size mismatch".into());
    }
    Ok(output)
}
pub fn verify(config:&Config,id:&str,identity:&Path)->Result<Record>{
    let client=Client::new(config)?;
    let record=read_record(&client,id)?;
    let temp=tempfile::tempdir()?;
    let file=staged(&client,&record,temp.path())?;
    let actual=archive::verify(&file,identity)?;
    if actual!=record.files {return Err("S3 archive member count mismatch".into());}
    Ok(record)
}
pub fn restore(config:&Config,id:&str,identity:&Path,destination:&Path)->Result<Record>{
    if fs::symlink_metadata(destination).is_ok() {
        return Err("S3 vault restore destination must not exist".into());
    }
    let client=Client::new(config)?;
    let record=read_record(&client,id)?;
    let temp=tempfile::tempdir()?;
    let file=staged(&client,&record,temp.path())?;
    let actual=archive::verify(&file,identity)?;
    if actual!=record.files {return Err("S3 vault archive membership mismatch".into());}
    archive::extract(&file,identity,destination)?;
    Ok(record)
}


/// Native GUI path: the unlocked age identity stays in RAM, never on disk.
pub fn verify_from_memory(config:&Config,id:&str,secret:&[u8])->Result<Record>{
    let client=Client::new(config)?;
    let record=read_record(&client,id)?;
    let temp=tempfile::tempdir()?;
    let file=staged(&client,&record,temp.path())?;
    let actual=archive::verify_from_memory(&file,secret)?;
    if actual!=record.files {return Err("S3 age identity or archive verification failed".into());}
    Ok(record)
}
/// Restore using the same in-memory identity as the protected local key vault.
pub fn restore_from_memory(config:&Config,id:&str,secret:&[u8],destination:&Path)->Result<Record>{
    if fs::symlink_metadata(destination).is_ok() {
        return Err("S3 restore destination must not exist".into());
    }
    let client=Client::new(config)?;
    let record=read_record(&client,id)?;
    let temp=tempfile::tempdir()?;
    let file=staged(&client,&record,temp.path())?;
    let actual=archive::verify_from_memory(&file,secret)?;
    if actual!=record.files {return Err("S3 age integrity or membership failed".into());}
    archive::extract_from_memory(&file,secret,destination)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config()->Config {
        Config{provider:"minio".into(),endpoint:"https://example.test".into(),
            region:"us-east-1".into(),bucket:"vault-worm".into(),
            prefix:"tenant/backup".into(),recipient:format!("age1{}", "q".repeat(40)),
            retention_days:30,allow_loopback_http_for_testing:false}
    }
    #[test]
    fn refuses_insecure_remote_and_unsafe_paths(){
        let mut cfg=config();
        cfg.validate().unwrap();
        cfg.endpoint="http://example.test".into();
        assert!(cfg.validate().is_err());
        cfg.endpoint="http://localhost:9000".into();
        assert!(cfg.validate().is_err());
        cfg.allow_loopback_http_for_testing=true;
        cfg.validate().unwrap();
        cfg.endpoint="http://169.254.169.254".into();
        assert!(cfg.validate().is_err());
        cfg.endpoint="https://example.test".into();
        cfg.prefix="../escape".into();
        assert!(cfg.validate().is_err());
        cfg.prefix="tenant/backup".into();
        cfg.retention_days=0;
        assert!(cfg.validate().is_err());
    }
    #[test]
    fn permits_rustfs_s3_provider_but_never_relaxes_security(){
        let mut cfg=config();
        cfg.provider="rustfs".into();
        assert!(cfg.validate().is_ok());
        cfg.endpoint="http://rustfs.example.test:9000".into();
        assert!(cfg.validate().is_err());
        cfg.endpoint="https://rustfs.example.test".into();
        cfg.retention_days=0;
        assert!(cfg.validate().is_err());
        cfg.retention_days=30;
        cfg.prefix="../bad".into();
        assert!(cfg.validate().is_err());
    }
    #[test]
    fn encodes_canonical_s3_paths_and_requires_worm_fields(){
        assert_eq!(encode("a /b+c",true),"a%20/b%2Bc");
        assert_eq!(xml_value("<Status>Enabled</Status>","Status"),Some("Enabled"));
        assert_eq!(xml_value("<Status>Suspended</Status>","Status"),Some("Suspended"));
        assert_eq!(xml_value("<Mode>GOVERNANCE</Mode>","Mode"),Some("GOVERNANCE"));
        assert_eq!(hex_sha(b"hello"),"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert!(valid_key("snapshots/0a.tar.gz.age"));
        assert!(!valid_key("../hidden"));
    }
}
