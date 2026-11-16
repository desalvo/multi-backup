//! Opt-in replica executor for Rust filesystem input plugins. A cluster
//! coordinator claim only controls admission: a second durable write fence
//! is still needed before production exactly-once semantics can be claimed.
//! Unsupported plugins and transports FAIL CLOSED.
use crate::cluster::{Database,Lease};
use multi_backup_core::{
    chunk_store::BackupKind,
    execution_policy::ExecutionLimits,
    plugin_executor,plugins::GpfsAccess
};
use serde::Deserialize;
use serde_json::{json,Value};
use std::{error::Error,path::PathBuf,
    sync::{Arc,Mutex,mpsc,atomic::{AtomicBool,Ordering}},
    thread,time::{Duration,Instant}};
type Res<T>=Result<T,Box<dyn Error+Send+Sync>>;
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesystemSpec {
    source:PathBuf,
    store:PathBuf,
    identity:PathBuf,
    label:String,
    #[serde(default="default_namespace")]namespace:String,
    #[serde(default)]top_namespace:String,
    #[serde(default)]mode:BackupKind,
    #[serde(default)]parent_id:Option<String>,
    #[serde(default)]snapshot_attested:bool,
    #[serde(default)]gpfs_access:Option<String>
}
fn default_namespace()->String{"files".into()}
fn run(lease:&Lease,live:&dyn Fn(bool)->Result<(),String>)->Res<Value>{
    live(true)?;
    // SSH/agent delegated work is deliberately executed only via a
    // separately authorized agent dispatch route, not as a shell on a pod.
    if lease.source_transport!="server_local"{
        return Err("Replica direct executor supports server_local only; remote dispatch requires approved transport integration".into());
    }
    if !matches!(lease.plugin_id.as_str(),"posix"|"nfs"|"smb"|"sshfs"|"gpfs"|"cephfs"|"glusterfs"|"lustre"){
        return Err("Input plugin is not supported by Rust filesystem executor".into());
    }
    let limits:ExecutionLimits=serde_json::from_value(lease.limits.clone())?;
    limits.validate().map_err(|x|->Box<dyn Error+Send+Sync>{x.into()})?;
    limits.check_host().map_err(|x|->Box<dyn Error+Send+Sync>{x.into()})?;
    let profile:FilesystemSpec=serde_json::from_value(lease.spec.clone())?;
    if !profile.source.is_absolute()||!profile.store.is_absolute()||
       !profile.identity.is_absolute(){
        return Err("Cluster filesystem paths must be absolute".into());
    }
    let access=profile.gpfs_access.as_deref().map(GpfsAccess::parse)
        .transpose()?;
    let snapshot=plugin_executor::backup_with_mode_guard(&lease.plugin_id,"posix",
        &profile.source,&profile.store,&profile.identity,&profile.label,
        &profile.namespace,&profile.top_namespace,
        profile.snapshot_attested,access,profile.mode,
        profile.parent_id.as_deref(),live)?;
    Ok(json!({"backup_id":snapshot.id,"label":snapshot.label,
        "logical_files":snapshot.entries.len(),"transport":"server_local",
        "replica_lease_attempt":lease.attempts}))
}
fn execute(db:Database,replica:String,lease:Lease){
    let lost=Arc::new(AtomicBool::new(false));
    let cloned=db.clone();
    let job_id=lease.job_id.clone();
    let token=lease.token.clone();
    let id=replica.clone();
    let lost_heartbeat=lost.clone();
    let (stop,stop_receiver)=mpsc::channel::<()>();
    let renewal=thread::spawn(move||{
        while stop_receiver.recv_timeout(Duration::from_secs(20)).is_err(){
            if !matches!(cloned.renew(&id,&job_id,&token,120),Ok(true)){
                lost_heartbeat.store(true,Ordering::Release);
                break;
            }
        }
    });
    let check_db=db.clone();
    let check_replica=replica.clone();
    let check_job=lease.job_id.clone();
    let check_token=lease.token.clone();
    let checkpoint=Mutex::new(Instant::now()-Duration::from_secs(10));
    let guard=|force:bool|->Result<(),String>{
        if lost.load(Ordering::Acquire){
            return Err("Lost cluster writer lease; stop before manifest publication".into());
        }
        let mut last=checkpoint.lock().map_err(|_|"Lease guard poisoned")?;
        if force || last.elapsed()>Duration::from_secs(2){
            let valid=check_db.is_current(&check_replica,&check_job,&check_token)
                .map_err(|_|"Cluster database unreachable; refusing writes")?;
            if !valid{
                lost.store(true,Ordering::Release);
                return Err("Writer lease no longer current".into());
            }
            *last=Instant::now();
        }
        Ok(())
    };
    let result=run(&lease,&guard);
    let _=stop.send(());
    let _=renewal.join();
    // A lost lease may have been reassigned. Refuse to report success or
    // mutate another replica's state. The extra write fence for ciphertext
    // manifests is a BLOCKER, not a claimed implemented feature.
    if lost.load(Ordering::Acquire){
        eprintln!("Cluster job {} lost lease, requires reconciliation",lease.job_id);
        return;
    }
    let (success,details)=match result{
        Ok(value)=>(true,value),
        Err(error)=>(false,json!({"error":error.to_string()}))
    };
    if !matches!(db.finish(&replica,&lease.job_id,&lease.token,success,&details),Ok(true)){
        eprintln!("Cluster job {} could not commit fenced result",lease.job_id);
    }
}
/// Called once at replica startup only when the administrator opts in.
/// With a local volume, the PG claim guarantees that only its registered
/// owner replica can take the work; shared storage jobs may run on any
/// registered capable healthy replica.
pub(crate) fn maximum_active_jobs(raw:Option<&str>)->Result<usize,String>{
    let max=match raw{
        Some(value)=>value.parse::<usize>()
            .map_err(|_|"MULTI_BACKUP_CLUSTER_MAX_ACTIVE_JOBS must be 1..64")?,
        None=>4
    };
    if !(1..=64).contains(&max){
        return Err("MULTI_BACKUP_CLUSTER_MAX_ACTIVE_JOBS must be 1..64".into());
    }
    Ok(max)
}
pub(crate) fn validate_capacity_against_advertised(budget:u16,raw:Option<&str>)
    ->Result<usize,String>{
    let local=maximum_active_jobs(raw)?;
    if !(1..=64).contains(&budget)||local>usize::from(budget){
        return Err("Local active job limit must fit the advertised replica capacity (1..64)".into());
    }
    Ok(local)
}
/// Every active job holds a local permit from BEFORE the PostgreSQL claim
/// until all lease and result reconciliation has finished. This prevents
/// unbounded spawn_blocking tasks and false resource-limit compliance.
pub async fn spawn(db:Database,replica:String){
    let max=match maximum_active_jobs(
        std::env::var("MULTI_BACKUP_CLUSTER_MAX_ACTIVE_JOBS").ok().as_deref()){
        Ok(max)=>max,
        Err(error)=>{eprintln!("Cluster worker disabled: {error}");return;}
    };
    let permits=Arc::new(tokio::sync::Semaphore::new(max));
    let mut timer=tokio::time::interval(Duration::from_secs(3));
    loop{
        timer.tick().await;
        // Never claim another durable queued job if all local execution
        // slots are occupied. The queued job stays recoverable in PG.
        let permit=match permits.clone().try_acquire_owned(){
            Ok(permit)=>permit,
            Err(_)=>continue,
        };
        let local=db.clone();
        let id=replica.clone();
        let next=tokio::task::spawn_blocking(move||{
            let quarantined=local.quarantine_expired()?;
            if quarantined>0{
                eprintln!("Quarantined {quarantined} expired distributed jobs: reconciliation required before reuse");
            }
            local.claim(&id,120)
        }).await;
        match next{
            Ok(Ok(Some(lease)))=>{
                let db=db.clone();
                let id=replica.clone();
                tokio::task::spawn_blocking(move||{
                    // Keep permit across the complete sync executor,
                    // heartbeat stop/join and PostgreSQL finish.
                    let _permit=permit;
                    execute(db,id,lease);
                });
            }
            Ok(Ok(None))=>{drop(permit);}
            _=>{
                drop(permit);
                eprintln!("Cluster queue unavailable; refusing new assignment");
            }
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn bounded_worker_slots_fail_closed_on_invalid_config(){
        assert_eq!(maximum_active_jobs(None).unwrap(),4);
        assert_eq!(maximum_active_jobs(Some("1")).unwrap(),1);
        assert_eq!(maximum_active_jobs(Some("64")).unwrap(),64);
        for invalid in ["","0","65","-1","99","not-a-number","1.2"]{
            assert!(maximum_active_jobs(Some(invalid)).is_err());
        }
        assert!(validate_capacity_against_advertised(4,Some("4")).is_ok());
        assert!(validate_capacity_against_advertised(4,Some("5")).is_err());
        assert!(validate_capacity_against_advertised(0,Some("1")).is_err());
        assert!(validate_capacity_against_advertised(65,Some("1")).is_err());
    }
    #[tokio::test]
    async fn local_semaphore_exhaustion_does_not_grant_extra_job_claims(){
        let slots=Arc::new(tokio::sync::Semaphore::new(1));
        let occupied=slots.clone().try_acquire_owned().unwrap();
        assert!(slots.clone().try_acquire_owned().is_err());
        drop(occupied);
        assert!(slots.clone().try_acquire_owned().is_ok());
    }
    #[test]fn nonlocal_and_unsupported_work_is_denied(){
        let make=|plugin:&str,transport:&str|Lease{
            job_id:"j1".into(),plugin_id:plugin.into(),storage_id:"s".into(),
            source_transport:transport.into(),spec:json!({}),
            limits:serde_json::to_value(ExecutionLimits::default()).unwrap(),
            token:"t".into(),attempts:1
        };
        assert!(run(&make("afs","ssh_agentless"),&|_|Ok(())).is_err());
        assert!(run(&make("github","server_local"),&|_|Ok(())).is_err());
    }
}