//! Read-only, TLS-verified native PVE/PBS API inventory adapter. NOT a backup
//! engine. Actual VM/CT backup and restore require separate field acceptance.
use serde::{Deserialize,Serialize};
use serde_json::Value;
use std::{error::Error,fs,io::Read,path::Path,time::Duration};
use url::Url;
use zeroize::Zeroize;
type Result<T>=std::result::Result<T,Box<dyn Error+Send+Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config{
    pub provider:String,
    pub endpoint:String,
    pub token_id:String,
    pub token_secret:String,
}
impl Drop for Config{
    fn drop(&mut self){self.token_secret.zeroize();}
}
impl Config{
    pub fn load(path:&Path)->Result<Self>{
        let md=fs::symlink_metadata(path)?;
        if !md.file_type().is_file() || md.len()>16_384 {
            return Err("Hypervisor token configuration must be a small regular file".into());
        }
        #[cfg(unix)]{
            use std::os::unix::fs::PermissionsExt;
            if md.permissions().mode()&0o077!=0{
                return Err("Hypervisor API token file must be owner-only (0600)".into());
            }
        }
        let value:Self=serde_json::from_slice(&fs::read(path)?)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self)->Result<()>{
        if !matches!(self.provider.as_str(),"proxmox"|"pbs"){
            return Err("Only PVE or PBS have Rust read-only inventory adapters".into());
        }
        let url=Url::parse(&self.endpoint)?;
        if url.scheme()!="https" || url.host_str().is_none() ||
           !url.username().is_empty() || url.password().is_some() ||
           url.query().is_some() || url.fragment().is_some() || url.path()!="/" {
            return Err("Hypervisor inventory requires a bare HTTPS origin without credentials".into());
        }
        if !(6..=128).contains(&self.token_id.len()) ||
           !self.token_id.contains('@') || self.token_id.matches('!').count()!=1 ||
           !self.token_id.bytes().all(|c|c.is_ascii_alphanumeric()||
             b"!@._-".contains(&c)){
            return Err("Invalid scoped PVE/PBS API token ID".into());
        }
        if !(16..=256).contains(&self.token_secret.len()) ||
           !self.token_secret.bytes().all(|c|c.is_ascii_graphic()&&c!=b'\''){
            return Err("Hypervisor API token secret must be bounded and printable".into());
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Entry{
    pub id:String,
    pub kind:String,
    pub name:String,
    pub node:Option<String>,
    pub power_state:Option<String>,
}
#[derive(Serialize)]
pub struct Inventory{
    pub provider:String,
    pub items:Vec<Entry>,
    pub read_only:bool,
    pub live_backup_certified:bool,
    pub offline_backup_certified:bool,
    pub restore_certified:bool,
}
fn parse_api(provider:&str,bytes:&[u8])->Result<Inventory>{
    // Even an empty but otherwise valid API data array must not turn an
    // unsupported hypervisor into an apparently successful inventory.
    if !matches!(provider,"proxmox"|"pbs"){
        return Err("Unsupported hypervisor inventory provider".into());
    }
    let json:Value=serde_json::from_slice(bytes)?;
    let rows=json.get("data").and_then(Value::as_array)
        .ok_or("Missing API data array")?;
    if rows.len()>10_000 {return Err("Hypervisor inventory exceeds safe item limit".into());}
    let mut items=Vec::new();
    let mut seen=std::collections::HashSet::new();
    for row in rows {
        let record=match provider {
            "proxmox"=>{
                let kind=row["type"].as_str().unwrap_or("");
                if !matches!(kind,"qemu"|"lxc"){continue;}
                let vmid=row["vmid"].as_u64().ok_or("Missing native guest ID")?;
                let node=row["node"].as_str().ok_or("Missing PVE node identity")?;
                if node.is_empty()||node.len()>128{
                    return Err("Unsafe PVE cluster node identity".into());
                }
                Entry{id:vmid.to_string(),kind:kind.into(),
                    name:row["name"].as_str().unwrap_or("").chars().take(128).collect(),
                    node:Some(node.to_owned()),
                    power_state:Some(row["status"].as_str().unwrap_or("unknown")
                        .chars().take(32).collect())}
            },
            "pbs"=>{
                let store=row["store"].as_str().ok_or("Missing PBS datastore name")?;
                if store.is_empty()||store.len()>128{
                    return Err("Invalid PBS datastore".into());
                }
                Entry{id:store.to_owned(),kind:"datastore".into(),
                    name:store.to_owned(),node:None,power_state:None}
            },
            _=>return Err("Unsupported hypervisor inventory provider".into()),
        };
        if !seen.insert((record.kind.clone(),record.id.clone())){
            return Err("Duplicate hypervisor guest/datastore ID in inventory".into());
        }
        items.push(record);
    }
    Ok(Inventory{provider:provider.to_owned(),items,read_only:true,
        live_backup_certified:false,offline_backup_certified:false,
        restore_certified:false})
}
pub fn inventory(cfg:&Config)->Result<Inventory>{
    cfg.validate()?;
    let mut url=Url::parse(&cfg.endpoint)?;
    match cfg.provider.as_str(){
        "proxmox"=>{url.set_path("/api2/json/cluster/resources");
                   url.set_query(Some("type=vm"));}
        "pbs"=>{url.set_path("/api2/json/admin/datastore");}
        _=>return Err("Unsupported provider".into()),
    }
    let mut auth=match cfg.provider.as_str(){
        "proxmox"=>format!("PVEAPIToken={}={}",cfg.token_id,cfg.token_secret),
        "pbs"=>format!("PBSAPIToken={}:{}",cfg.token_id,cfg.token_secret),
        _=>unreachable!(),
    };
    let client=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20)).build();
    let response=client.get(url.as_str()).set("Authorization",&auth).call();
    auth.zeroize();
    let response=response.map_err(|_|"Hypervisor API inventory request rejected or untrusted TLS")?;
    if response.status()!=200{return Err("Unexpected hypervisor inventory status".into());}
    let mut bytes=Vec::new();
    response.into_reader().take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len()>1_048_576{return Err("Hypervisor API response too large".into());}
    parse_api(&cfg.provider,&bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn config(provider:&str,url:&str)->Config{
        Config{provider:provider.into(),endpoint:url.into(),
            token_id:"root@pam!monitor".into(),
            token_secret:"01234567-89ab-cdef-0123-456789abcdef".into()}
    }
    #[test]fn rejects_plain_http_url_userinfo_or_redirect_target(){
        assert!(config("proxmox","https://pve.internal:8006").validate().is_ok());
        for bad in ["http://pve.internal:8006",
                    "https://user:secret@pve.internal:8006",
                    "https://pve.internal:8006/other",
                    "https://pve.internal:8006/?token=bad"]{
            assert!(config("proxmox",bad).validate().is_err());
        }
        assert!(config("vmware","https://vcenter.internal").validate().is_err());
    }
    #[test]fn native_qemu_lxc_and_pbs_datastore_parsing_does_not_claim_backup(){
        let pve=br#"{"data":[{"vmid":100,"type":"qemu","name":"VM","node":"pve1","status":"running"},{"vmid":101,"type":"lxc","name":"CT","node":"pve2","status":"stopped"}]}"#;
        let inv=parse_api("proxmox",pve).unwrap();
        assert_eq!(inv.items.len(),2);
        assert_eq!(inv.items[1].kind,"lxc");
        assert!(!inv.live_backup_certified && !inv.restore_certified);
        let pbs=parse_api("pbs",br#"{"data":[{"store":"backupset-a"}]}"#).unwrap();
        assert_eq!(pbs.items[0].kind,"datastore");
        assert!(!pbs.offline_backup_certified);
    }
    #[test]fn fail_closed_on_invalid_or_duplicate_response(){
        assert!(parse_api("proxmox",br#"{"data":[{"type":"qemu","vmid":10,"node":"pve1"},{"type":"qemu","vmid":10,"node":"pve2"}]}"#).is_err());
        assert!(parse_api("pbs",br#"{"data":[{"store":""}]}"#).is_err());
        assert!(parse_api("vmware",br#"{"data":[]}"#).is_err());
        assert!(parse_api("proxmox",br#"{"data":null}"#).is_err());
    }
}
