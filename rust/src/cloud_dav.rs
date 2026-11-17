//! ownCloud Classic and Nextcloud native authenticated WebDAV archive B-Out.
 //! Distinct OCS product identity preflight; no generic rclone server can
 //! masquerade as a qualified Nextcloud/ownCloud service. All byte transfer
 //! reuses the verified HTTPS create-only WebDAV adapter.
 //! This is encrypted archive storage, NOT complete calendar/contacts/ACL
 //! or native application database backup.
use crate::{protected_config,webdav_output};
use serde::{Deserialize,Serialize};
use std::{io::Read,path::Path,time::Duration};
use url::Url;
use zeroize::Zeroizing;
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config{
    pub provider:String,
    pub endpoint:String,
    pub status_url:String,
    pub username:String,
}
#[derive(Debug,Serialize)]
pub struct Receipt{
    pub provider:String,
    pub archive:String,
    pub bytes:u64,
    pub sha256:String,
    pub encrypted:bool,
    pub create_only:bool,
    pub independent_remote_readback:bool,
    pub restored_members:usize,
    pub vendor_status_verified:bool
}
impl Config{
    pub fn load(path:&Path)->Result<Self>{
        let raw=Zeroizing::new(protected_config::read_owner_only(path,16_384)?);
        let c:Self=serde_json::from_slice(&raw)?;
        c.validate()?;Ok(c)
    }
    fn validate(&self)->Result<()>{
        if !matches!(self.provider.as_str(),"owncloud"|"nextcloud"){
            return Err("Cloud DAV profile must be owncloud or nextcloud".into());
        }
        let dav=Url::parse(&self.endpoint)?;
        let status=Url::parse(&self.status_url)?;
        if dav.scheme()!="https"||status.scheme()!="https"||
           dav.host_str().is_none()||dav.host_str()!=status.host_str()||
           dav.port_or_known_default()!=status.port_or_known_default()||
           !dav.username().is_empty()||!status.username().is_empty()||
           dav.password().is_some()||status.password().is_some()||
           dav.query().is_some()||status.query().is_some()||
           dav.fragment().is_some()||status.fragment().is_some()||
           !dav.path().ends_with('/') || !status.path().ends_with("/status.php")||
           dav.path().contains("..")||status.path().contains(".."){
            return Err("Provider requires same-origin trusted HTTPS DAV/status URLs".into());
        }
        let prefix=status.path().strip_suffix("status.php")
            .ok_or("Invalid product status path")?;
        let dav_path=dav.path().strip_prefix(prefix)
            .ok_or("DAV and provider status path prefixes differ")?;
        if !matches!(dav_path,"remote.php/webdav/") &&
           !(dav_path.starts_with("remote.php/dav/files/") &&
             dav_path.ends_with('/')&&dav_path.len()>25 &&
             dav_path.split('/').all(|x|!x.contains(".."))) {
            return Err("Provider DAV collection must be a direct ownCloud/Nextcloud files endpoint".into());
        }
        if self.provider=="nextcloud" && !dav_path.starts_with("remote.php/dav/files/"){
            return Err("Nextcloud requires native user-scoped DAV collection".into());
        }
        if self.username.is_empty()||self.username.len()>128||
           !self.username.bytes().all(|b|b.is_ascii_alphanumeric()||
             matches!(b,b'.'|b'_'|b'-'|b'@')){
            return Err("Invalid provider DAV user".into());
        }
        Ok(())
    }
    fn status_attestation(&self)->Result<()>{
        self.validate()?;
        let request=ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(25))
            .redirects(0).build();
        let response=request.get(&self.status_url).set("Accept","application/json")
            .call().map_err(|_|"Provider HTTPS status missing, TLS invalid, or redirected")?;
        if response.status()!=200{return Err("Provider status did not return 200".into());}
        let mut body=Vec::new();
        response.into_reader().take(16_385).read_to_end(&mut body)?;
        if body.len()>16_384{return Err("Oversized vendor capability status".into());}
        let payload:serde_json::Value=serde_json::from_slice(&body)?;
        let product=payload.get("productname").and_then(|x|x.as_str())
            .ok_or("Missing native provider product identity in HTTPS status")?;
        if !product.eq_ignore_ascii_case(&self.provider)||
           payload["installed"]!=true||payload["maintenance"]==true {
            return Err("Cloud DAV server is not the selected healthy installed provider".into());
        }
        Ok(())
    }
    fn webdav(&self)->webdav_output::Config{
        webdav_output::Config{endpoint:self.endpoint.clone(),username:self.username.clone()}
    }
}
pub fn backup(config:Config,source:&Path,filename:&str,
              recipient:&str,identity:&Path)->Result<Receipt>{
    config.status_attestation()?;
    let out=webdav_output::backup_directory(config.webdav(),source,
                                              filename,recipient,identity)?;
    Ok(Receipt{provider:config.provider,archive:out.remote_file,
        bytes:out.bytes,sha256:out.sha256,encrypted:out.encrypted,
        create_only:out.create_only,
        independent_remote_readback:out.independent_https_readback,
        restored_members:out.verified_members,vendor_status_verified:true})
}
pub fn restore(config:Config,filename:&str,identity:&Path,dest:&Path)->Result<serde_json::Value>{
    config.status_attestation()?;
    let count=webdav_output::restore_archive(config.webdav(),filename,identity,dest)?;
    Ok(serde_json::json!({"provider":config.provider,"restored_members":count,
      "age_verified":true,"create_only_local_restore":true,
      "vendor_status_verified":true}))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn does_not_accept_generic_or_cross_origin_webdav(){
        let config=Config{provider:"nextcloud".into(),
            endpoint:"https://cloud.example.test/remote.php/dav/files/ci/".into(),
            status_url:"https://cloud.example.test/status.php".into(),username:"ci".into()};
        assert!(config.validate().is_ok());
        let mut invalid=config;
        invalid.status_url="https://untrusted.example.test/status.php".into();
        assert!(invalid.validate().is_err());
        invalid.status_url="https://cloud.example.test/status.php".into();
        invalid.provider="owncloud".into();
        invalid.endpoint="https://cloud.example.test/remote.php/webdav/".into();
        assert!(invalid.validate().is_ok());
        invalid.provider="notnextcloud".into();
        assert!(invalid.validate().is_err());
    }
}
