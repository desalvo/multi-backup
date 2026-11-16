//! TLS-verified native PVE/PBS inventory and PVE remote vzdump task control.
//! A returned UPID is NOT proof of a completed/verified backup or restore.
//! Physical backup/restore artifact qualification requires field acceptance.
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

#[derive(Debug,Serialize)]
pub struct PveBackupTask{
    pub provider:String,
    pub guest_kind:String,
    pub guest_id:u32,
    pub node:String,
    pub mode:String,
    pub storage:String,
    pub upid:String,
    pub task_accepted:bool,
    pub archive_verified:bool,
    pub restore_certified:bool,
}
#[derive(Debug,Serialize)]
pub struct PveTaskStatus{
    pub provider:String,
    pub upid:String,
    pub state:String,
    pub finished:bool,
    pub remote_task_ok:bool,
    pub archive_verified:bool,
    pub restore_certified:bool,
}
fn validate_pve_name(name:&str)->Result<()>{
    // No path separators or control chars may reach the Proxmox URL.
    if name.is_empty()||name.len()>64||
       !name.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.-".contains(&b))||
       name.starts_with('.')||name.starts_with('-'){
        return Err("Invalid PVE node/storage identifier".into());
    }
    Ok(())
}
fn validate_upid(node:&str,upid:&str)->Result<()>{
    if upid.len()>512||upid.len()<20||!upid.starts_with(&format!("UPID:{node}:"))||
       !upid.bytes().all(|b|b.is_ascii_alphanumeric()||b":._-@!".contains(&b)){
        return Err("Invalid or cross-node PVE task identifier".into());
    }
    Ok(())
}
fn pve_task_response(node:&str,bytes:&[u8])->Result<String>{
    let data:Value=serde_json::from_slice(bytes)?;
    let upid=data.get("data").and_then(Value::as_str)
        .ok_or("PVE backup response missing task UPID")?;
    validate_upid(node,upid)?;
    Ok(upid.to_owned())
}
fn pve_task_state(node:&str,upid:&str,bytes:&[u8])->Result<PveTaskStatus>{
    validate_upid(node,upid)?;
    let data:Value=serde_json::from_slice(bytes)?;
    let task=data.get("data").ok_or("Missing PVE task status")?;
    let state=task.get("status").and_then(Value::as_str)
        .ok_or("Missing PVE task state")?;
    if !matches!(state,"running"|"stopped"){
        return Err("Unknown PVE task state".into());
    }
    let result=task.get("exitstatus").and_then(Value::as_str);
    let finished=state=="stopped";
    if finished && result.is_none(){return Err("Stopped PVE task lacks exitstatus".into());}
    Ok(PveTaskStatus{provider:"proxmox".into(),upid:upid.into(),
        state:state.into(),finished,remote_task_ok:finished&&result==Some("OK"),
        archive_verified:false,restore_certified:false})
}
fn pve_backup_form(node:&str,kind:&str,id:u32,mode:&str,storage:&str)->Result<String>{
    validate_pve_name(node)?;
    validate_pve_name(storage)?;
    if !(100..=999_999_999).contains(&id) ||
       !matches!(kind,"qemu"|"lxc") ||
       !matches!(mode,"snapshot"|"suspend"|"stop"){
        return Err("Unsupported PVE native guest ID, kind or backup mode".into());
    }
    // Explicit single-guest run; never backup all guests or prune older
    // archives. PVE decides snapshot feasibility for this guest/storage.
    Ok(url::form_urlencoded::Serializer::new(String::new())
        .append_pair("vmid",&id.to_string())
        .append_pair("mode",mode)
        .append_pair("storage",storage)
        .append_pair("compress","zstd")
        .append_pair("remove","0")
        .finish())
}
fn pve_authenticated(cfg:&Config,method:&str,url:&Url,body:Option<&str>)->Result<Vec<u8>>{
    let client=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20)).build();
    let mut authorization=format!("PVEAPIToken={}={}",cfg.token_id,cfg.token_secret);
    let result=if method=="GET" {
        client.get(url.as_str()).set("Authorization",&authorization).call()
    }else if method=="POST" {
        client.post(url.as_str()).set("Authorization",&authorization)
            .set("Content-Type","application/x-www-form-urlencoded")
            .send_string(body.ok_or("Missing PVE backup form")?)
    }else{
        authorization.zeroize();
        return Err("Unsupported PVE task HTTP method".into());
    };
    authorization.zeroize();
    // Never propagate provider HTTP errors: they may contain sensitive
    // user-configured diagnostics or token material.
    let response=result.map_err(|_|"PVE HTTPS request rejected, invalid TLS or insufficient ACL")?;
    if response.status()!=200{return Err("Unexpected PVE task response status".into());}
    let mut bytes=Vec::new();
    response.into_reader().take(8_193).read_to_end(&mut bytes)?;
    if bytes.len()>8_192{return Err("Oversized PVE task response".into());}
    Ok(bytes)
}
pub fn start_pve_backup(cfg:&Config,node:&str,kind:&str,id:u32,
    mode:&str,storage:&str,confirmation:&str)->Result<PveBackupTask>{
    cfg.validate()?;
    if cfg.provider!="proxmox"{return Err("PBS is not a PVE vzdump control endpoint".into());}
    let form=pve_backup_form(node,kind,id,mode,storage)?;
    let expected=format!("{kind}:{id}:{mode}:{storage}");
    if confirmation!=expected{
        return Err("Remote backup requires --confirm-remote-backup KIND:ID:MODE:STORAGE".into());
    }
    // This independent HTTPS inventory request confirms an exact native
    // QEMU/LXC guest and host before any remote POST. No guess from VMID.
    let inv=inventory(cfg)?;
    if !inv.items.iter().any(|v|v.id==id.to_string()&&v.kind==kind&&
        v.node.as_deref()==Some(node)){
        return Err("Target guest/kind/node not independently confirmed by live PVE inventory".into());
    }
    let mut url=Url::parse(&cfg.endpoint)?;
    url.path_segments_mut().map_err(|_|"Invalid PVE base URL")?
        .extend(["api2","json","nodes",node,"vzdump"]);
    let response=pve_authenticated(cfg,"POST",&url,Some(&form))?;
    let upid=pve_task_response(node,&response)?;
    Ok(PveBackupTask{provider:"proxmox".into(),guest_kind:kind.into(),
        guest_id:id,node:node.into(),mode:mode.into(),storage:storage.into(),upid,
        task_accepted:true,archive_verified:false,restore_certified:false})
}
pub fn pve_backup_task_status(cfg:&Config,node:&str,upid:&str)->Result<PveTaskStatus>{
    cfg.validate()?;
    if cfg.provider!="proxmox"{return Err("Only PVE provides vzdump task UPIDs".into());}
    validate_pve_name(node)?;
    validate_upid(node,upid)?;
    let mut url=Url::parse(&cfg.endpoint)?;
    url.path_segments_mut().map_err(|_|"Invalid PVE base URL")?
        .extend(["api2","json","nodes",node,"tasks",upid,"status"]);
    let response=pve_authenticated(cfg,"GET",&url,None)?;
    pve_task_state(node,upid,&response)
}


#[derive(Debug,Serialize)]
pub struct PbsSnapshot{
    pub backup_type:String,
    pub backup_id:String,
    pub backup_time:i64,
    pub bytes:Option<u64>,
    pub pbs_verification_state:String,
    pub pbs_verification_ok:bool,
    pub multibackup_archive_verified:bool,
}
#[derive(Debug,Serialize)]
pub struct PbsSnapshotInventory{
    pub provider:String,
    pub datastore:String,
    pub namespace:String,
    pub read_only:bool,
    pub snapshots:Vec<PbsSnapshot>,
    pub multibackup_restore_certified:bool,
}
fn pbs_name(value:&str)->Result<()>{
    if value.is_empty()||value.len()>128 ||
       value=="."||value==".."||value.starts_with('-') ||
       !value.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.-".contains(&b)){
        return Err("Unsafe PBS datastore or namespace component".into());
    }
    Ok(())
}
fn pbs_namespace(value:&str)->Result<()>{
    if value.len()>256 || value.split('/').any(|part|pbs_name(part).is_err()){
        return Err("Invalid or traversal-prone PBS namespace".into());
    }
    Ok(())
}
fn parse_pbs_snapshots(store:&str,namespace:&str,bytes:&[u8])->Result<PbsSnapshotInventory>{
    pbs_name(store)?;
    if !namespace.is_empty(){pbs_namespace(namespace)?;}
    let doc:Value=serde_json::from_slice(bytes)?;
    let rows=doc.get("data").and_then(Value::as_array)
        .ok_or("PBS snapshot response missing data array")?;
    if rows.len()>10_000{return Err("PBS snapshot inventory limit exceeded".into());}
    let mut seen=std::collections::HashSet::new();
    let mut snapshots=Vec::with_capacity(rows.len());
    for item in rows{
        let kind=item.get("backup-type").and_then(Value::as_str)
            .ok_or("Missing native PBS backup type")?;
        if !matches!(kind,"vm"|"ct"|"host"){
            return Err("Unsupported PBS backup type; cannot silently ignore guest snapshots".into());
        }
        let id=item.get("backup-id").and_then(Value::as_str)
            .ok_or("Missing PBS native backup ID")?;
        pbs_name(id)?;
        let time=item.get("backup-time").and_then(Value::as_i64)
            .ok_or("Missing PBS backup timestamp")?;
        if !(1..=253402300799).contains(&time) ||
           !seen.insert((kind.to_owned(),id.to_owned(),time)){
            return Err("Invalid or repeated PBS snapshot identity".into());
        }
        let size=match item.get("size"){
            Some(Value::Number(value))=>Some(value.as_u64().ok_or("Invalid PBS snapshot size")?),
            Some(Value::Null)|None=>None,
            _=>return Err("Invalid PBS snapshot size".into()),
        };
        let verify=item.get("verification").and_then(Value::as_object)
            .and_then(|v|v.get("state")).and_then(Value::as_str)
            .unwrap_or("unverified");
        if verify.len()>32||!verify.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)){
            return Err("Unsafe PBS verification result".into());
        }
        snapshots.push(PbsSnapshot{backup_type:kind.to_owned(),
            backup_id:id.to_owned(),backup_time:time,bytes:size,
            pbs_verification_state:verify.to_owned(),
            pbs_verification_ok:verify=="ok",
            multibackup_archive_verified:false});
    }
    snapshots.sort_by(|a,b|a.backup_type.cmp(&b.backup_type)
        .then(a.backup_id.cmp(&b.backup_id)).then(a.backup_time.cmp(&b.backup_time)));
    Ok(PbsSnapshotInventory{provider:"pbs".into(),datastore:store.to_owned(),
        namespace:namespace.to_owned(),read_only:true,snapshots,
        multibackup_restore_certified:false})
}
pub fn pbs_snapshots(cfg:&Config,store:&str,namespace:&str)->Result<PbsSnapshotInventory>{
    cfg.validate()?;
    if cfg.provider!="pbs"{return Err("PVE cannot list PBS native backup snapshots".into());}
    pbs_name(store)?;
    if !namespace.is_empty(){pbs_namespace(namespace)?;}
    let mut url=Url::parse(&cfg.endpoint)?;
    url.path_segments_mut().map_err(|_|"Invalid PBS origin")?
        .extend(["api2","json","admin","datastore",store,"snapshots"]);
    if !namespace.is_empty(){url.query_pairs_mut().append_pair("ns",namespace);}
    let client=ureq::AgentBuilder::new().redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20)).build();
    let mut auth=format!("PBSAPIToken={}:{}",cfg.token_id,cfg.token_secret);
    let response=client.get(url.as_str()).set("Authorization",&auth).call();
    auth.zeroize();
    let response=response.map_err(|_|"PBS HTTPS snapshot list rejected or untrusted TLS")?;
    if response.status()!=200{return Err("Unexpected PBS snapshot list status".into());}
    let mut bytes=Vec::new();
    response.into_reader().take(2_097_153).read_to_end(&mut bytes)?;
    if bytes.len()>2_097_152{return Err("Oversized PBS snapshot list".into());}
    parse_pbs_snapshots(store,namespace,&bytes)
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
    #[test]fn vetted_single_guest_vzdump_form_cannot_delete_existing_archives(){
        let form=pve_backup_form("pve-node1","lxc",101,"snapshot","pbs-store").unwrap();
        assert!(form.contains("vmid=101") && form.contains("mode=snapshot"));
        assert!(form.contains("storage=pbs-store") && form.contains("remove=0"));
        assert!(!form.contains("all=") && !form.contains("prune-backups="));
        assert!(pve_backup_form("node/../evil","qemu",101,"stop","store").is_err());
        assert!(pve_backup_form("pve1","docker",101,"snapshot","store").is_err());
        assert!(pve_backup_form("pve1","qemu",99,"stop","store").is_err());
        assert!(pve_backup_form("pve1","lxc",101,"live","store").is_err());
        assert!(pve_backup_form("pve1","lxc",101,"stop","../bad").is_err());
    }
    #[test]fn pve_upid_is_a_task_receipt_not_backup_certificate(){
        let upid="UPID:pve-node1:0000000A:00C0FFEE:AAFF00AA:vzdump:101:root@pam:";
        assert_eq!(pve_task_response("pve-node1",format!(r#"{{"data":"{upid}"}}"#).as_bytes()).unwrap(),upid);
        assert!(pve_task_response("pve-node2",format!(r#"{{"data":"{upid}"}}"#).as_bytes()).is_err());
        let done=pve_task_state("pve-node1",upid,br#"{"data":{"status":"stopped","exitstatus":"OK"}}"#).unwrap();
        assert!(done.finished && done.remote_task_ok);
        assert!(!done.archive_verified && !done.restore_certified);
        let fail=pve_task_state("pve-node1",upid,br#"{"data":{"status":"stopped","exitstatus":"failed"}}"#).unwrap();
        assert!(!fail.remote_task_ok);
        assert!(pve_task_state("pve-node1",upid,br#"{"data":{"status":"stopped"}}"#).is_err());
    }
    #[test]fn pbs_snapshot_verification_is_not_multibackup_archive_attestation(){
        let list=br#"{"data":[
            {"backup-type":"vm","backup-id":"100","backup-time":1734310600,"size":4096,
             "verification":{"state":"ok"}},
            {"backup-type":"ct","backup-id":"201","backup-time":1734310601,"size":null},
            {"backup-type":"host","backup-id":"backup-node","backup-time":1734310602,
             "verification":{"state":"failed"}}
        ]}"#;
        let inventory=parse_pbs_snapshots("backup-data","team1/team2",list).unwrap();
        assert_eq!(inventory.snapshots.len(),3);
        assert!(inventory.read_only);
        assert!(!inventory.multibackup_restore_certified);
        let vm=inventory.snapshots.iter().find(|s|s.backup_type=="vm").unwrap();
        assert!(vm.pbs_verification_ok);
        assert!(!vm.multibackup_archive_verified);
        let ct=inventory.snapshots.iter().find(|s|s.backup_type=="ct").unwrap();
        assert_eq!(ct.pbs_verification_state,"unverified");
        assert!(ct.bytes.is_none());
        assert!(parse_pbs_snapshots("backup-data","../prod",list).is_err());
    }
    #[test]fn pbs_rejects_duplicate_ids_malformed_entries_or_cross_provider(){
        assert!(parse_pbs_snapshots("s","",br#"{"data":[
            {"backup-type":"vm","backup-id":"100","backup-time":1700000000},
            {"backup-type":"vm","backup-id":"100","backup-time":1700000000}
        ]}"#).is_err());
        assert!(parse_pbs_snapshots("s","",br#"{"data":[
            {"backup-type":"docker","backup-id":"1","backup-time":1700000000}
        ]}"#).is_err());
        assert!(parse_pbs_snapshots("s","",br#"{"data":[
            {"backup-type":"vm","backup-id":"../99","backup-time":1700000000}
        ]}"#).is_err());
        assert!(parse_pbs_snapshots("s","",br#"{"data":"not-an-array"}"#).is_err());
        let cfg=config("proxmox","https://pve.internal:8006");
        assert!(pbs_snapshots(&cfg,"s","").is_err());
    }
    #[test]fn pve_only_control_must_not_operate_against_pbs(){
        let cfg=config("pbs","https://pbs.internal:8007");
        assert!(start_pve_backup(&cfg,"pbs1","lxc",101,"stop","backups","lxc:101:stop:backups").is_err());
        assert!(pve_backup_task_status(&cfg,"pbs1",
            "UPID:pbs1:0000000A:00C0FFEE:AAFF00AA:vzdump:101:root@pam:").is_err());
    }
    #[test]fn fail_closed_on_invalid_or_duplicate_response(){
        assert!(parse_api("proxmox",br#"{"data":[{"type":"qemu","vmid":10,"node":"pve1"},{"type":"qemu","vmid":10,"node":"pve2"}]}"#).is_err());
        assert!(parse_api("pbs",br#"{"data":[{"store":""}]}"#).is_err());
        assert!(parse_api("vmware",br#"{"data":[]}"#).is_err());
        assert!(parse_api("proxmox",br#"{"data":null}"#).is_err());
    }
}
