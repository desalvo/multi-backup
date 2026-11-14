//! Multi Backup's provider-independent, conservative maintenance contract.
//! Policy preview is pure: no remote calls, deletion or scheduler process.
use serde::{Serialize,Deserialize};
use std::{collections::{BTreeMap,BTreeSet},path::Path};
const DAY:u64=86_400;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(default,deny_unknown_fields)]
pub struct Schedule {
    pub enabled:bool,
    pub hour_utc:u8,
    pub minute_utc:u8,
}
impl Default for Schedule {
    fn default()->Self{Self{enabled:false,hour_utc:2,minute_utc:0}}
}
impl Schedule{
    pub fn due(&self,now:u64,last:Option<u64>)->Result<bool,String>{
        if self.hour_utc>23||self.minute_utc>59 {
            return Err("Invalid UTC schedule".into());
        }
        if !self.enabled{return Ok(false);}
        let start=now/DAY*DAY + u64::from(self.hour_utc)*3600
            +u64::from(self.minute_utc)*60;
        Ok(now>=start && last.is_none_or(|v|v<start && v<=now))
    }
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(default,deny_unknown_fields)]
pub struct Retention{
    pub days:u32,
    pub keep_last_verified:u32,
    pub immutable_retention_enforced:bool,
}
impl Default for Retention{
    fn default()->Self{Self{days:30,keep_last_verified:1,
                           immutable_retention_enforced:true}}
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(default,deny_unknown_fields)]
pub struct GarbageCollection{
    pub enabled:bool,
    pub interval_hours:u16,
    pub orphan_grace_hours:u16,
}
impl Default for GarbageCollection{
    fn default()->Self{Self{enabled:true,interval_hours:24,
                           orphan_grace_hours:48}}
}
impl GarbageCollection{
    pub fn due(&self,now:u64,last:Option<u64>)->Result<bool,String>{
        if self.interval_hours==0||self.interval_hours>744||
           self.orphan_grace_hours<24 {
            return Err("Invalid GC configuration".into());
        }
        Ok(self.enabled && last.is_none_or(|v|v<=now &&
           now-v>=u64::from(self.interval_hours)*3600))
    }
}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct MaintenancePolicy {
    pub schedule:Schedule,
    pub retention:Retention,
    pub garbage_collection:GarbageCollection,
}
impl MaintenancePolicy{
    pub fn validate(&self)->Result<(),String>{
        self.schedule.due(0,None)?;
        self.garbage_collection.due(0,None)?;
        if self.retention.days==0||self.retention.days>36500||
           self.retention.keep_last_verified==0||
           self.retention.keep_last_verified>10000||
           !self.retention.immutable_retention_enforced {
            return Err("Invalid retention or disabled WORM safety".into());
        }
        Ok(())
    }
}
pub fn safe_namespace(path:&str)->bool{
    !path.is_empty() && path.len()<=512 && path.split('/').count()<=24 &&
    path.split('/').all(|x| !x.is_empty() && x.len()<=80 &&
        x!="." && x!=".." && !x.ends_with('.') && !x.ends_with(' ') &&
        x.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_'||
                      c==b'-'||c==b'.'))
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct BackupStamp {
    pub id:String,
    pub namespace:String,
    /// Missing (0) is an unknown legacy date and NEVER auto-expires.
    pub created_unix:u64,
    pub verified:bool,
    pub pinned:bool,
    pub legal_hold:bool,
    pub immutable_until:Option<u64>,
}
#[derive(Clone,Debug,Serialize)]
pub struct ExpiryPreview{
    pub expire:Vec<String>,
    pub preserve:BTreeMap<String,&'static str>,
}
pub fn expiry_preview(backups:&[BackupStamp],policy:&Retention,now:u64)
    ->Result<ExpiryPreview,String>{
    if policy.days==0||policy.keep_last_verified==0||
       !policy.immutable_retention_enforced {
        return Err("Unsafe retention policy".into());
    }
    let mut by_namespace:BTreeMap<&str,Vec<&BackupStamp>>=BTreeMap::new();
    let mut unique=BTreeSet::new();
    for entry in backups {
        if !safe_namespace(&entry.namespace)||!unique.insert(entry.id.as_str()){
            return Err("Invalid or duplicate backup record".into());
        }
        by_namespace.entry(&entry.namespace).or_default().push(entry);
    }
    let mut result=ExpiryPreview{expire:Vec::new(),preserve:BTreeMap::new()};
    for group in by_namespace.values_mut() {
        group.sort_by(|a,b|b.created_unix.cmp(&a.created_unix)
            .then(b.id.cmp(&a.id)));
        let keep:BTreeSet<&str>=group.iter().filter(|v|v.verified)
            .take(policy.keep_last_verified as usize).map(|x|x.id.as_str()).collect();
        for entry in group {
            let why=if entry.created_unix==0||entry.created_unix>now {
                Some("unknown_or_future_timestamp")
            } else if !entry.verified {
                Some("unverified_backup")
            } else if entry.pinned||entry.legal_hold {
                Some("pinned_or_legal_hold")
            } else if entry.immutable_until.is_some_and(|v|v>now) {
                Some("immutable_retention")
            } else if keep.contains(entry.id.as_str()) {
                Some("keep_last_verified")
            } else if now-entry.created_unix<(policy.days as u64).saturating_mul(DAY) {
                Some("within_retention")
            } else {None};
            if let Some(reason)=why {
                result.preserve.insert(entry.id.clone(),reason);
            } else {
                result.expire.push(entry.id.clone());
            }
        }
    }
    result.expire.sort();
    Ok(result)
}
#[derive(Clone,Debug,Serialize)]
pub struct IndexNode{
    pub path:String,
    pub file_count:u64,
    pub total_bytes:u64,
}
/// In-memory hierarchy: folder access needs O(log N) lookup, not an
/// O(N) source tree traversal on each UI expansion.
#[derive(Clone,Debug,Serialize)]
pub struct NamespaceIndex{
    pub top_namespace:String,
    pub folders:BTreeMap<String,IndexNode>,
    pub children:BTreeMap<String,Vec<String>>,
}
impl NamespaceIndex{
    pub fn from_paths(namespace:&str,paths:impl IntoIterator<Item=(String,u64)>)
        ->Result<Self,String>{
        if !safe_namespace(namespace){return Err("Unsafe top namespace".into());}
        let mut index=Self{top_namespace:namespace.into(),
            folders:BTreeMap::new(),children:BTreeMap::new()};
        let mut seen=BTreeSet::new();
        for (path,bytes) in paths {
            if path.len()>4096||path.chars().any(char::is_control)||
               path.split('/').any(|p|p.is_empty()||p=="."||p=="..")||
               !seen.insert(path.clone()) {
                return Err("Unsafe or repeated index entry".into());
            }
            let mut parent=String::new();
            let components:Vec<_>=path.split('/').collect();
            for segment in components.iter().take(components.len()-1){
                let current=if parent.is_empty(){segment.to_string()}
                    else{format!("{parent}/{segment}")};
                if !index.folders.contains_key(&current){
                    index.folders.insert(current.clone(),IndexNode {
                        path:current.clone(),file_count:0,total_bytes:0});
                    index.children.entry(parent.clone()).or_default().push(current.clone());
                }
                let node=index.folders.get_mut(&current).ok_or("Folder lost")?;
                node.file_count=node.file_count.checked_add(1).ok_or("Folder overflow")?;
                node.total_bytes=node.total_bytes.checked_add(bytes).ok_or("Index bytes overflow")?;
                parent=current;
            }
        }
        for values in index.children.values_mut(){values.sort();values.dedup();}
        Ok(index)
    }
    pub fn page(&self,parent:&str,offset:usize,limit:usize)
        ->Result<Vec<&IndexNode>,String>{
        if limit==0||limit>1000{return Err("Page size 1..1000 required".into());}
        if !parent.is_empty()&&!self.folders.contains_key(parent){
            return Err("Folder not indexed".into());
        }
        Ok(self.children.get(parent).map_or_else(Vec::new,|children|
            children.iter().skip(offset).take(limit)
                .filter_map(|key|self.folders.get(key)).collect()))
    }
}
#[derive(Clone,Debug,Serialize)]
pub struct Capacity{
    pub plugin:String,pub total:Option<u64>,pub available:Option<u64>,
    pub used:Option<u64>,pub source:&'static str,pub caveat:&'static str,
}
pub fn mounted_capacity(plugin:&str,root:&Path)->Result<Capacity,String>{
    if !["posix","nfs","smb","cephfs","glusterfs","lustre","gpfs","sshfs"].contains(&plugin){
        return Err("This provider requires a remote quota API: unknown".into());
    }
    let md=std::fs::symlink_metadata(root).map_err(|_|"Destination inaccessible".to_string())?;
    if !md.is_dir()||md.file_type().is_symlink(){
        return Err("Unsafe destination directory".into());
    }
    let total=fs2::total_space(root).map_err(|_|"Capacity query failed".to_string())?;
    let available=fs2::available_space(root).map_err(|_|"Free-space query failed".to_string())?;
    Ok(Capacity{plugin:plugin.into(),total:Some(total),available:Some(available),
       used:Some(total.saturating_sub(available)),source:"host_statvfs",
       caveat:"Mount estimate; not a provider quota, GPFS fileset quota or ACL proof"})
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum BackupMethod {
    DatabaseLiveNative,VmLiveQuiesced,VmLiveCrashConsistent,VmStopRestart,
    CtLiveSnapshot,CtSuspendResume,CtStopRestart,
}
#[derive(Clone,Debug,Serialize)]
pub struct ConsistencyPlan {
    pub method:BackupMethod,pub application_consistent:bool,
    pub ensure_restart_after_failure:bool,
}
pub fn plan_consistency(kind:&str,online_native:bool,snapshot:bool,
    quiesce:bool,allow_stop:bool)->Result<ConsistencyPlan,String>{
    let (method,application_consistent,restart)=match kind {
        "mysql"|"mariadb"|"postgresql"|"oracle" if online_native =>
            (BackupMethod::DatabaseLiveNative,true,false),
        "mysql"|"mariadb"|"postgresql"|"oracle" =>
            return Err("No consistent online DB backup; never copy live datadir".into()),
        "vm" if snapshot&&quiesce => (BackupMethod::VmLiveQuiesced,true,false),
        "vm" if snapshot => (BackupMethod::VmLiveCrashConsistent,false,false),
        "vm" if allow_stop => (BackupMethod::VmStopRestart,true,true),
        "ct" if snapshot => (BackupMethod::CtLiveSnapshot,false,false),
        "ct" if quiesce => (BackupMethod::CtSuspendResume,false,true),
        "ct" if allow_stop => (BackupMethod::CtStopRestart,true,true),
        "vm"|"ct" => return Err("No acceptable live method; stop fallback disabled".into()),
        _ => return Err("Unknown consistency provider".into()),
    };
    Ok(ConsistencyPlan{method,application_consistent,
        ensure_restart_after_failure:restart})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn defaults(){let p=MaintenancePolicy::default();p.validate().unwrap();
        assert_eq!(p.retention.days,30);assert_eq!(p.garbage_collection.interval_hours,24);
        assert!(!p.schedule.enabled);}
    #[test]fn schedule_utc_daily(){let mut s=Schedule::default();s.enabled=true;
        let day=100*DAY;
        assert!(!s.due(day,None).unwrap());
        assert!(s.due(day+2*3600,None).unwrap());
        assert!(!s.due(day+3*3600,Some(day+2*3600)).unwrap());}
    #[test]fn safe_nested_namespaces(){
        assert!(safe_namespace("INFN/Roma1/Cluster/VM"));
        for bad in ["","/A","A/","A//B","A/../B","x\\y","a/./b"]{
            assert!(!safe_namespace(bad),"{bad}");
        }}
    #[test]fn retention_fail_closed_and_respects_worm(){
        let now=100*DAY;
        let make=|id:&str,age:u64|BackupStamp{id:id.into(),namespace:"lab".into(),
            created_unix:now-age*DAY,verified:true,pinned:false,legal_hold:false,
            immutable_until:None};
        let old=make("old",70);let newest=make("newest",40);
        let mut worm=make("worm",65);worm.immutable_until=Some(now+DAY);
        let mut legal=make("legal",66);legal.legal_hold=true;
        let preview=expiry_preview(&[old,newest,worm,legal],
                                    &Retention::default(),now).unwrap();
        assert_eq!(preview.expire,vec!["old"]);
        assert_eq!(preview.preserve["worm"],"immutable_retention");
        assert_eq!(preview.preserve["newest"],"keep_last_verified");
    }
    #[test]fn index_pagination(){let i=NamespaceIndex::from_paths("team/proj",[
        ("github/a/file".into(),10),("github/b/file".into(),20),
        ("github/a/other".into(),30),("dropbox/x".into(),5)]).unwrap();
        assert_eq!(i.page("",0,10).unwrap().len(),2);
        assert_eq!(i.page("github",0,1).unwrap().len(),1);
        assert_eq!(i.folders["github/a"].file_count,2);
        assert_eq!(i.folders["github/a"].total_bytes,40);
        assert!(i.page("bad",0,10).is_err());}
    #[test]fn online_backups_reject_unverifiable_consistency(){
        for kind in ["oracle","mysql","postgresql","mariadb"]{
            assert!(plan_consistency(kind,false,false,false,true).is_err());
            assert_eq!(plan_consistency(kind,true,false,false,false).unwrap().method,
                       BackupMethod::DatabaseLiveNative);}
        let vm=plan_consistency("vm",false,true,false,false).unwrap();
        assert!(!vm.application_consistent);
        let ct=plan_consistency("ct",false,false,false,true).unwrap();
        assert!(ct.ensure_restart_after_failure);
    }
    #[test]fn never_guess_remote_quota(){
        let t=tempfile::tempdir().unwrap();
        assert!(mounted_capacity("posix",t.path()).unwrap().total.unwrap()>0);
        assert!(mounted_capacity("s3",t.path()).is_err());}
}
