//! Durable Rust-only manual/scheduled backup and verification executor.
//! Schedules choose full/incremental and default to verifying the entire
//! incremental chain. The "once" runner can be used by cron/launchd/Task
//! Scheduler; daemon mode polls the same durable state without Python.
use crate::{
    backup_chain,
    chunk_store::{BackupKind, Store},
    plugin_executor,
    plugins::GpfsAccess,
};
use fs2::FileExt;
use serde::{Deserialize,Serialize};
use std::{
    collections::{BTreeMap,HashSet},
    fs::{self,File,OpenOptions},
    io::Write,
    path::{Path,PathBuf}
};
use tempfile::NamedTempFile;
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Clone,Copy,Debug,Deserialize,Serialize)]
#[serde(rename_all="snake_case")]
pub enum Action {Backup,Verify}

fn default_true()->bool{true}
fn default_interval()->u64{86_400}
fn default_posix()->String{"posix".into()}
fn default_namespace()->String{"files".into()}
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id:String,
    pub enabled:bool,
    #[serde(default="default_interval")]
    pub interval_seconds:u64,
    pub action:Action,
    #[serde(default)]
    pub backup_mode:BackupKind,
    pub root:PathBuf,
    pub identity:PathBuf,
    #[serde(default)]
    pub source:Option<PathBuf>,
    #[serde(default="default_posix")]
    pub input_plugin:String,
    #[serde(default="default_posix")]
    pub output_plugin:String,
    #[serde(default="default_namespace")]
    pub namespace:String,
    #[serde(default)]
    pub top_namespace:String,
    #[serde(default)]
    pub label:String,
    #[serde(default)]
    pub parent_id:Option<String>,
    #[serde(default)]
    pub backup_id:Option<String>,
    #[serde(default="default_true")]
    pub verify_full_chain:bool,
    #[serde(default)]
    pub source_snapshot_attested:bool,
    #[serde(default)]
    pub input_gpfs_access:Option<String>,
}
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {version:u32,jobs:Vec<Job>}
#[derive(Debug,Default,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct State {version:u32,jobs:BTreeMap<String,JobState>}
#[derive(Debug,Default,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct JobState {last_success_unix:u64,last_backup_id:Option<String>}
#[derive(Debug,Serialize)]
pub struct RunResult {
    pub job_id:String,
    pub action:Action,
    pub executed:bool,
    pub backup_id:Option<String>,
    pub verified_full_chain:Option<bool>,
}

fn load_config(file:&Path)->Res<Configuration>{
    let md=fs::symlink_metadata(file)?;
    if !md.file_type().is_file()||md.len()>1024*1024 {
        return Err("Invalid scheduler config file".into());
    }
    let config:Configuration=serde_json::from_reader(File::open(file)?)?;
    if config.version!=1||config.jobs.is_empty()||config.jobs.len()>128 {
        return Err("Unsupported scheduler configuration".into());
    }
    let mut ids=HashSet::new();
    for job in &config.jobs {
        if job.id.is_empty()||job.id.len()>80 ||
           !job.id.bytes().all(|x|x.is_ascii_alphanumeric()||b"_-.".contains(&x))||
           !ids.insert(&job.id) {
            return Err("Invalid or duplicate scheduler job ID".into());
        }
        if !(60..=366*86_400).contains(&job.interval_seconds) {
            return Err("Schedule interval must be from 60 seconds to 366 days".into());
        }
        if job.label.len()>256 {return Err("Schedule label too long".into());}
        if matches!(job.action,Action::Backup) && job.source.is_none(){
            return Err("Backup schedule requires source path".into());
        }
        if matches!(job.action,Action::Verify)&&job.source.is_some(){
            return Err("Verification schedule must not read a live source".into());
        }
        if !job.root.is_absolute()||!job.identity.is_absolute() {
            return Err("Schedule storage and age identity paths must be absolute".into());
        }
    }
    Ok(config)
}
fn load_state(path:&Path)->Res<State>{
    match fs::symlink_metadata(path){
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>
            Ok(State{version:1,..State::default()}),
        Err(e)=>Err(e.into()),
        Ok(md)=>{
            if !md.file_type().is_file()||md.len()>1024*1024{
                return Err("Unsafe scheduler state".into());
            }
            let value:State=serde_json::from_reader(File::open(path)?)?;
            if value.version!=1{return Err("Unsupported scheduler state schema".into());}
            Ok(value)
        }
    }
}
fn save_state(path:&Path,state:&State)->Res<()>{
    let parent=path.parent().ok_or("Scheduler state has no parent")?;
    if !parent.is_dir(){return Err("Scheduler state parent missing".into());}
    let mut temp=NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp,state)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e|e.error)?;
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path,fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
/// A failed job is not marked successful or advanced. The lock prevents
/// multiple scheduled runners from performing the same backup cycle.
pub fn run_once(config_path:&Path,state_path:&Path,force:bool,now:u64)
    ->Res<Vec<RunResult>>
{
    let config=load_config(config_path)?;
    let state_parent=state_path.parent().ok_or("Missing scheduler state parent")?;
    if !fs::symlink_metadata(state_parent)?.file_type().is_dir(){
        return Err("Scheduler state parent must be a real directory".into());
    }
    let lock_path=state_path.with_extension("lock");
    match fs::symlink_metadata(&lock_path) {
        Ok(md) if !md.file_type().is_file() =>
            return Err("Unsafe scheduler lock file".into()),
        Err(error) if error.kind()!=std::io::ErrorKind::NotFound =>
            return Err(error.into()),
        _=>{},
    }
    let lock=OpenOptions::new().write(true).create(true).open(&lock_path)?;
    lock.lock_exclusive()?;
    let mut state=load_state(state_path)?;
    let mut results=Vec::new();
    for job in &config.jobs {
        if !job.enabled {continue;}
        let last=state.jobs.get(&job.id).map(|v|v.last_success_unix);
        let due=force || last.is_none_or(|then|
            now>=then && now-then>=job.interval_seconds);
        if !due {
            results.push(RunResult{job_id:job.id.clone(),action:job.action,
                executed:false,backup_id:None,verified_full_chain:None});
            continue;
        }
        let store=Store::open(&job.root,&job.identity)?;
        let state_entry=state.jobs.entry(job.id.clone()).or_default();
        let (id,verified)=match job.action {
            Action::Backup=>{
                let parent=if job.backup_mode==BackupKind::Incremental {
                    state_entry.last_backup_id.as_deref()
                        .or(job.parent_id.as_deref())
                }else{None};
                let source=job.source.as_deref().ok_or("Missing source")?;
                let access=job.input_gpfs_access.as_deref().map(GpfsAccess::parse)
                    .transpose()?;
                let backup=plugin_executor::backup_with_mode(
                    &job.input_plugin,&job.output_plugin,source,&job.root,
                    &job.identity,&job.label,&job.namespace,&job.top_namespace,
                    job.source_snapshot_attested,access,job.backup_mode,parent)?;
                state_entry.last_backup_id=Some(backup.id.clone());
                (backup.id,None)
            }
            Action::Verify=>{
                let selected=job.backup_id.as_deref()
                    .or(state_entry.last_backup_id.as_deref())
                    .ok_or("Scheduled verification requires a backup ID")?;
                let health=backup_chain::verify_chain(&store,&job.identity,selected,
                    job.verify_full_chain)?;
                if !health.healthy{
                    return Err("Scheduled backup chain verification failed".into());
                }
                (selected.to_owned(),Some(job.verify_full_chain))
            }
        };
        state_entry.last_success_unix=now;
        save_state(state_path,&state)?;
        results.push(RunResult{job_id:job.id.clone(),action:job.action,
            executed:true,backup_id:Some(id),verified_full_chain:verified});
    }
    FileExt::unlock(&lock)?;
    Ok(results)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn default_chain_verification_is_enabled_and_schedule_is_explicit(){
        let raw=r#"{"id":"v","enabled":true,"action":"verify",
            "root":"/tmp/a","identity":"/tmp/key","backup_id":"id"}"#;
        let job:Job=serde_json::from_str(raw).unwrap();
        assert!(job.verify_full_chain);
        assert_eq!(job.interval_seconds,86_400);
        assert!(matches!(job.action,Action::Verify));
    }
}