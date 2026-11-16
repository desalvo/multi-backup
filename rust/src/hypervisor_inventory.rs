//! Native, read-only VMware vCenter and oVirt/RHV VM inventory.
//! No VMware/oVirt container guest API, VM snapshot, backup, or restore
//! is exposed. Credentials are short-lived, preissued session/bearer tokens.
use serde::{Deserialize,Serialize};
use serde_json::Value;
use std::{collections::HashSet,error::Error,fs,io::Read,path::Path,time::Duration};
use url::Url;
use zeroize::Zeroize;
type Result<T>=std::result::Result<T,Box<dyn Error+Send+Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub provider:String,
    pub endpoint:String,
    pub session_token:String,
}
impl Drop for Config {
    fn drop(&mut self){self.session_token.zeroize();}
}
impl Config {
    pub fn load(path:&Path)->Result<Self>{
        let md=fs::symlink_metadata(path)?;
        if !md.file_type().is_file()||md.len()>16_384{
            return Err("Hypervisor session config must be a bounded regular file".into());
        }
        #[cfg(unix)]{
            use std::os::unix::fs::PermissionsExt;
            if md.permissions().mode()&0o077!=0{
                return Err("Hypervisor session config must be owner-only 0600".into());
            }
        }
        let config:Self=serde_json::from_slice(&fs::read(path)?)?;
        config.validate()?;
        Ok(config)
    }
    fn validate(&self)->Result<()>{
        if !matches!(self.provider.as_str(),"vmware"|"ovirt"){
            return Err("Only VMware vCenter and oVirt Engine VM inventory supported".into());
        }
        let endpoint=Url::parse(&self.endpoint)?;
        if endpoint.scheme()!="https"||endpoint.host_str().is_none()||
           !endpoint.username().is_empty()||endpoint.password().is_some()||
           endpoint.query().is_some()||endpoint.fragment().is_some()||
           endpoint.path()!="/"{
            return Err("VM inventory requires a bare verified HTTPS origin".into());
        }
        if !(16..=512).contains(&self.session_token.len())||
           !self.session_token.bytes().all(|c|c.is_ascii_graphic()){
            return Err("VM inventory requires bounded opaque session/bearer token".into());
        }
        Ok(())
    }
}
#[derive(Debug,Serialize)]
pub struct Guest{
    pub id:String,
    pub name:String,
    pub power_state:String,
    pub kind:String,
}
#[derive(Debug,Serialize)]
pub struct Inventory{
    pub provider:String,
    pub guests:Vec<Guest>,
    pub read_only:bool,
    pub inventory_complete:bool,
    pub live_backup_certified:bool,
    pub offline_backup_certified:bool,
    pub restore_certified:bool,
    pub native_container_guests_supported:bool,
}
fn validated_id(value:&str)->Result<()>{
    if value.is_empty()||value.len()>128||value=="."||value==".."||
       !value.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.:-".contains(&b)){
        return Err("Unsafe native VM identifier".into());
    }
    Ok(())
}
fn parse_inventory(provider:&str,bytes:&[u8])->Result<Inventory>{
    if !matches!(provider,"vmware"|"ovirt"){
        return Err("Unknown native VM inventory provider".into());
    }
    let json:Value=serde_json::from_slice(bytes)?;
    let rows=match provider{
        "vmware"=>json.as_array(),
        // oVirt Engine JSON API may wrap the VM collection in 'vm' or
        // 'vms'. We reject missing collections rather than invent counts.
        "ovirt"=>json.get("vm").and_then(Value::as_array)
            .or_else(||json.pointer("/vms/vm").and_then(Value::as_array)),
        _=>None,
    }.ok_or("VM inventory response is not a native VM list")?;
    if rows.len()>4000{return Err("Native VM inventory exceeds bounded limit".into());}
    let mut seen=HashSet::new();
    let mut guests=Vec::with_capacity(rows.len());
    for item in rows{
        let id=if provider=="vmware"{
            item.get("vm").and_then(Value::as_str)
        }else{
            item.get("id").and_then(Value::as_str)
        }.ok_or("Native VM identity missing")?;
        validated_id(id)?;
        if !seen.insert(id.to_owned()){
            return Err("Native VM inventory contains a duplicate guest identifier".into());
        }
        let name=item.get("name").and_then(Value::as_str)
            .ok_or("VM name missing")?;
        if name.len()>512||name.chars().any(char::is_control){
            return Err("Unsafe VM display name".into());
        }
        let state=if provider=="vmware"{
            item.get("power_state").and_then(Value::as_str)
        }else{
            item.get("status").and_then(Value::as_str)
                .or_else(||item.pointer("/status/state").and_then(Value::as_str))
        }.unwrap_or("unknown");
        if state.len()>64||!state.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_'){
            return Err("Unsafe VM power state".into());
        }
        guests.push(Guest{id:id.into(),name:name.into(),
            power_state:state.into(),kind:"vm".into()});
    }
    guests.sort_by(|a,b|a.id.cmp(&b.id));
    // oVirt may use paging; vCenter returns at most 4000. Never suggest
    // the full host inventory is proven complete by a single API response.
    Ok(Inventory{provider:provider.into(),guests,read_only:true,
        inventory_complete:false,live_backup_certified:false,
        offline_backup_certified:false,restore_certified:false,
        native_container_guests_supported:false})
}
pub fn inventory(cfg:&Config)->Result<Inventory>{
    cfg.validate()?;
    let mut url=Url::parse(&cfg.endpoint)?;
    let path=match cfg.provider.as_str(){
        "vmware"=>"/api/vcenter/vm",
        "ovirt"=>"/ovirt-engine/api/vms",
        _=>return Err("Unsupported provider".into()),
    };
    url.set_path(path);
    let client=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20)).build();
    let mut secret=if cfg.provider=="vmware"{
        cfg.session_token.clone()
    }else{
        format!("Bearer {}",cfg.session_token)
    };
    let request=client.get(url.as_str()).set("Accept","application/json");
    let response=if cfg.provider=="vmware"{
        request.set("vmware-api-session-id",&secret).call()
    }else{
        request.set("Version","4").set("Authorization",&secret).call()
    };
    secret.zeroize();
    // Never leak authorization or server-provided failure bodies into logs.
    let reply=response.map_err(|_|"VM inventory HTTPS request failed or was denied")?;
    if reply.status()!=200{return Err("Unexpected VM inventory status".into());}
    let mut body=Vec::new();
    reply.into_reader().take(2_097_153).read_to_end(&mut body)?;
    if body.len()>2_097_152{return Err("VM inventory HTTPS body exceeds size limit".into());}
    parse_inventory(&cfg.provider,&body)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn cfg(provider:&str,endpoint:&str)->Config{
        Config{provider:provider.into(),endpoint:endpoint.into(),
            session_token:"unit-fixture-opaque-session-123456789".into()}
    }
    #[test]fn bare_https_only_and_preissued_token_required(){
        assert!(cfg("vmware","https://vcenter.example.invalid").validate().is_ok());
        assert!(cfg("ovirt","https://engine.example.invalid").validate().is_ok());
        for endpoint in ["http://vcenter.invalid",
            "https://user:password@vcenter.invalid",
            "https://vcenter.invalid/custom",
            "https://vcenter.invalid/?token=x"]{
            assert!(cfg("vmware",endpoint).validate().is_err());
        }
        assert!(cfg("proxmox","https://pve.invalid").validate().is_err());
        let mut no_token=cfg("ovirt","https://engine.invalid");
        no_token.session_token.clear();
        assert!(no_token.validate().is_err());
    }
    #[test]fn vmware_native_vms_are_only_read_only_uncertified_guests(){
        let inventory=parse_inventory("vmware",
            br#"[{"vm":"vm-42","name":"web01","power_state":"POWERED_ON"},
                 {"vm":"vm-43","name":"db01","power_state":"POWERED_OFF"}]"#).unwrap();
        assert_eq!(inventory.guests.len(),2);
        assert_eq!(inventory.guests[0].id,"vm-42");
        assert_eq!(inventory.guests[0].kind,"vm");
        assert!(inventory.read_only);
        assert!(!inventory.restore_certified&&!inventory.live_backup_certified);
        assert!(!inventory.native_container_guests_supported);
        assert!(!inventory.inventory_complete);
    }
    #[test]fn ovirt_engine_json_vm_collection_never_claims_native_ct(){
        let inventory=parse_inventory("ovirt",br#"{"vm":[
            {"id":"00000000-0000-0000-0000-000000000001","name":"ovirt-vm",
             "status":{"state":"up"}}]}"#).unwrap();
        assert_eq!(inventory.guests.len(),1);
        assert_eq!(inventory.guests[0].power_state,"up");
        assert!(!inventory.native_container_guests_supported);
        assert!(!inventory.offline_backup_certified);
        assert_eq!(parse_inventory("ovirt",br#"{"vms":{"vm":[]}}"#).unwrap().guests.len(),0);
    }
    #[test]fn reject_unknown_provider_guest_spoofing_and_malformed_response(){
        assert!(parse_inventory("vmware",br#"{"value":[]}"#).is_err());
        assert!(parse_inventory("ovirt",br#"{"ct":[{"id":"1"}]}"#).is_err());
        assert!(parse_inventory("vmware",br#"[{"vm":"vm-1","name":"x"},
            {"vm":"vm-1","name":"x"}]"#).is_err());
        assert!(parse_inventory("ovirt",br#"{"vm":[{"id":"../x","name":"unsafe"}]}"#).is_err());
        assert!(parse_inventory("vmware",br#"[{"vm":"vm-1","name":"x",
            "power_state":"BAD\\nHEADER"}]"#).is_err());
        assert!(parse_inventory("pbs",br#"[]"#).is_err());
    }
}
