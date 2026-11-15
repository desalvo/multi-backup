//! Replica-aware cross-plugin backup placement. A job may run on any replica
//! only when the data store is explicitly registered as SHARED. Otherwise it
//! is pinned to the one replica owning the local filesystem; other replicas
//! still advertise the job and store metadata through the common registry.
use crate::execution_policy::ExecutionLimits;
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,HashMap,HashSet};

#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum StorageVisibility {Shared,NodeLocal}
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum SourceTransport {ServerLocal,SshAgentless,ManagedAgentPull,ManagedAgentPush}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageLocation{
    pub storage_id:String,
    pub backend:String,
    pub visibility:StorageVisibility,
    /// Required for node_local and forbidden for shared.
    pub owner_replica:Option<String>,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replica {
    pub id:String,
    pub healthy:bool,
    pub max_concurrent_jobs:u16,
    pub currently_running_jobs:u16,
    /// Storage capability is an *explicitly registered* ID, not an inferred
    /// mount path. Server must verify the claimed storage before registration.
    pub accessible_storage_ids:Vec<String>,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupWork {
    pub id:String,
    pub plugin_id:String,
    pub storage_id:String,
    pub transport:SourceTransport,
    pub limits:ExecutionLimits,
    /// A single AFS volume dump cannot be partitioned into multiple workers.
    /// Independent volumes or archive partitions are separate jobs.
    #[serde(default)] pub splittable:bool,
}
#[derive(Clone,Debug,Serialize)]
pub struct Assignment {
    pub job_id:String,
    pub replica_id:String,
    pub threads:u16,
    pub storage_id:String,
    pub transport:SourceTransport,
}
fn identifier(s:&str)->bool{
    !s.is_empty()&&s.len()<=128&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||b"._-".contains(&b))
}
impl StorageLocation{
    pub fn validate(&self)->Result<(),String>{
        if !identifier(&self.storage_id)||!identifier(&self.backend){
            return Err("Invalid storage registry ID/backend".into());
        }
        match (self.visibility,&self.owner_replica){
            (StorageVisibility::Shared,None)=>Ok(()),
            (StorageVisibility::NodeLocal,Some(n)) if identifier(n)=>Ok(()),
            _=>Err("Shared storage must be explicitly shared; local storage requires exactly one owner replica".into()),
        }
    }
    pub fn eligible(&self,replica:&Replica)->bool{
        replica.healthy && replica.accessible_storage_ids.contains(&self.storage_id) &&
        match self.visibility {
            StorageVisibility::Shared=>true,
            StorageVisibility::NodeLocal=>self.owner_replica.as_deref()==Some(&replica.id),
        }
    }
}
/// Greedy least-loaded placement with stable tie-breakers. Admission only;
/// durable claims in a multi-replica cluster must use the PostgreSQL lease
/// implementation, never this in-memory allocation as a distributed lock.
pub fn plan(jobs:&[BackupWork],nodes:&[Replica],storages:&[StorageLocation])
    ->Result<Vec<Assignment>,String>{
    let mut registered=HashMap::new();
    for location in storages{
        location.validate()?;
        if registered.insert(location.storage_id.as_str(),location).is_some(){
            return Err("Duplicate storage ID".into());
        }
    }
    let mut slots=BTreeMap::new();
    let mut nodes_by_id=HashMap::new();
    for node in nodes{
        if !identifier(&node.id)||node.max_concurrent_jobs==0||
           node.currently_running_jobs>node.max_concurrent_jobs||
           nodes_by_id.insert(node.id.as_str(),node).is_some(){
            return Err("Invalid replica capacity/identity".into());
        }
        slots.insert(node.id.clone(),node.currently_running_jobs);
    }
    let mut selected=Vec::new();let mut ids=HashSet::new();
    for job in jobs{
        if !identifier(&job.id)||!identifier(&job.plugin_id)||
           !ids.insert(job.id.as_str()){return Err("Invalid/duplicate backup job ID".into());}
        job.limits.validate()?;
        let store=registered.get(job.storage_id.as_str())
            .ok_or("Backup references unregistered storage")?;
        let placement=nodes.iter().filter(|node|{
            if !store.eligible(node){return false;}
            slots.get(&node.id).copied().unwrap_or(u16::MAX)<node.max_concurrent_jobs
        }).min_by(|a,b|{
            // Normalize utilization to avoid packing all jobs on the biggest node
            let a_jobs=*slots.get(&a.id).unwrap_or(&a.max_concurrent_jobs) as u32;
            let b_jobs=*slots.get(&b.id).unwrap_or(&b.max_concurrent_jobs) as u32;
            (a_jobs*b.max_concurrent_jobs as u32)
                .cmp(&(b_jobs*a.max_concurrent_jobs as u32))
                .then_with(||a.id.cmp(&b.id))
        });
        if let Some(replica)=placement{
            *slots.get_mut(&replica.id).ok_or("Unknown replica capacity")?+=1;
            selected.push(Assignment{job_id:job.id.clone(),replica_id:replica.id.clone(),
                threads:job.limits.threads_per_job,
                storage_id:job.storage_id.clone(),transport:job.transport});
        }
    }
    Ok(selected)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn j(id:&str,store:&str)->BackupWork{
        BackupWork{id:id.into(),plugin_id:"afs".into(),storage_id:store.into(),
            transport:SourceTransport::ManagedAgentPull,
            limits:ExecutionLimits::default(),splittable:false}
    }
    fn n(id:&str)->Replica{
        Replica{id:id.into(),healthy:true,max_concurrent_jobs:3,
            currently_running_jobs:0,
            accessible_storage_ids:vec!["s3-data".into(),"local-a".into()]}
    }
    #[test]fn shared_jobs_balance_but_local_jobs_pin(){
        let stores=vec![
            StorageLocation{storage_id:"s3-data".into(),backend:"s3".into(),
                visibility:StorageVisibility::Shared,owner_replica:None},
            StorageLocation{storage_id:"local-a".into(),backend:"posix".into(),
                visibility:StorageVisibility::NodeLocal,owner_replica:Some("replica-a".into())}
        ];
        let nodes=vec![n("replica-a"),n("replica-b")];
        let jobs=vec![j("j0","s3-data"),j("j1","s3-data"),j("j2","local-a")];
        let result=plan(&jobs,&nodes,&stores).unwrap();
        assert_eq!(result[0].replica_id,"replica-a");
        assert_eq!(result[1].replica_id,"replica-b");
        assert_eq!(result[2].replica_id,"replica-a");
        assert_eq!(result[2].threads,4);
    }
    #[test]fn refuse_unregistered_or_ambiguous_storage(){
        let jobs=[j("first","local-a")];
        let a=StorageLocation{storage_id:"local-a".into(),backend:"posix".into(),
            visibility:StorageVisibility::NodeLocal,owner_replica:Some("replica-a".into())};
        let mut b=a.clone();b.owner_replica=None;
        assert!(plan(&jobs,&[n("replica-a")],&[b]).is_err());
        assert!(plan(&jobs,&[n("replica-a")],&[]).is_err());
        let result=plan(&jobs,&[n("replica-b")],&[a]).unwrap();
        assert!(result.is_empty(),"Never schedule locally pinned volumes on a different node");
    }
}