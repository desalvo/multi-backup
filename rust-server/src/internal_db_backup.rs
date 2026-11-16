//! PostgreSQL internal database snapshots encrypted before staging, then
//! copied into age-encrypted S3 Object Lock COMPLIANCE WORM and verified.
//! PostgreSQL is one logical HA database. All replicas coordinate via a
//! transactional PostgreSQL lease, not via independent crontabs.
use crate::cluster::Database;
use multi_backup_core::s3_vault;
use serde_json::{json,Value};
use std::{
    env,error::Error,fs,io,os::unix::fs::PermissionsExt,
    path::{Path,PathBuf},process::{Command,Stdio},time::Duration,
    sync::{Arc,atomic::{AtomicBool,Ordering},mpsc::{self,RecvTimeoutError}}
};
type Result<T>=std::result::Result<T,Box<dyn Error+Send+Sync>>;

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        if let Err(error)=fs::remove_dir_all(&self.0) {
            eprintln!("DB staging cleanup requires operator attention: {error}");
        }
    }
}
fn temporary_stage()->Result<Stage>{
    let base=PathBuf::from(env::var("MULTI_BACKUP_DB_BACKUP_STAGE_ROOT")
        .unwrap_or_else(|_|"/var/lib/multi-backup/db-staging".into()));
    if !base.is_absolute(){return Err("DB staging directory must be absolute".into());}
    let metadata=fs::symlink_metadata(&base)?;
    if !metadata.file_type().is_dir() || metadata.permissions().mode()&0o077!=0 {
        return Err("DB stage root must be a real mode-0700 directory".into());
    }
    let stage=base.join(format!("db-{}",uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    fs::set_permissions(&stage,fs::Permissions::from_mode(0o700))?;
    Ok(Stage(stage))
}
fn config()->Result<s3_vault::Config>{
    let cfg=env::var("MULTI_BACKUP_DB_BACKUP_S3_CONFIG")
       .map_err(|_|"Independent DB S3 WORM config is required")?;
    let storage=s3_vault::Config::load(Path::new(&cfg))?;
    if storage.retention_days<30 {
        return Err("Internal DB WORM retention requires >=30 days".into());
    }
    Ok(storage)
}
fn pg_dump_to_encrypted_file(stage:&Path,recipient:&str)->Result<()>{
    let dsn=env::var("MULTI_BACKUP_CLUSTER_DATABASE_URL")?;
    if !dsn.starts_with("postgresql://") && !dsn.starts_with("postgres://"){
        return Err("DB backup requires a PostgreSQL URL".into());
    }
    // PGDATABASE, not command-line --dbname, avoids leaking password in ps.
    // No shell and NO plaintext dump on any filesystem.
    let mut dump=Command::new("pg_dump");
    dump.args(["--format=custom","--no-owner","--no-acl",
        "--no-password","--file=-"]);
    dump.env("PGDATABASE",dsn);
    if let Some(ca)=env::var_os("MULTI_BACKUP_CLUSTER_TLS_CA_FILE"){
        dump.env("PGSSLROOTCERT",ca);
    }
    let mut child=dump.stdin(Stdio::null()).stdout(Stdio::piped())
        .stderr(Stdio::null()).spawn()?;
    let bytes=child.stdout.take().ok_or("Unable to read pg_dump stream")?;
    let encrypted=stage.join("internal-postgresql.dump.age");
    let result=Command::new("age")
       .args(["--encrypt","--recipient",recipient,"--output"])
       .arg(&encrypted).stdin(Stdio::from(bytes))
       .stdout(Stdio::null()).stderr(Stdio::null()).status();
    let database=child.wait()?;
    let age=result?;
    if !database.success()||!age.success(){
       return Err("PostgreSQL dump/age encryption failed; rejecting incomplete snapshot".into());
    }
    let meta=fs::symlink_metadata(&encrypted)?;
    if !meta.file_type().is_file() || meta.len()<100 {
       return Err("Encrypted database dump missing or implausibly small".into());
    }
    Ok(())
}
fn verify_live_lease(db:&Database,lease:&str,active:&AtomicBool)->Result<()>{
    if !active.load(Ordering::Acquire) || !db.renew_db_backup_lease(lease)?{
        return Err("PostgreSQL DB backup lease lost; no S3 object may be certified".into());
    }
    Ok(())
}
fn perform(db:&Database,lease:&str,active:&AtomicBool)->Result<Value>{
    verify_live_lease(db,lease,active)?;
    let cfg=config()?;
    // S3 WORM preflight (versioning+COMPLIANCE) happens again during backup.
    s3_vault::preflight(&cfg)?;
    let stage=temporary_stage()?;
    pg_dump_to_encrypted_file(&stage.0,&cfg.recipient)?;
    // Never publish this new WORM snapshot if the original owner has died,
    // timed out or lost the lease during a long PostgreSQL dump.
    verify_live_lease(db,lease,active)?;
    let result=s3_vault::backup(&cfg,&stage.0)?;
    // Verify that the independently committed S3 object actually decrypts
    // with the locally-provisioned recovery identity before recording PASS.
    let key=PathBuf::from(env::var("MULTI_BACKUP_AGE_IDENTITY_FILE")?);
    let checked=s3_vault::verify(&cfg,&result.id,&key)?;
    if result.archive_version!=checked.archive_version || result.sha256!=checked.sha256 {
        return Err("WORM object version/checksum changed after storage commit".into());
    }
    verify_live_lease(db,lease,active)?;
    Ok(serde_json::to_value(result)?)
}
pub fn execute(db:Database,_replica:String,lease:String){
    // 30-minute initial lease is renewed every 45 seconds for long dumps.
    // Only the current writer's UUID fencing token can renew. Errors deny
    // successful certification; an orphaned immutable S3 object must be
    // reconciled manually rather than promoted to a valid backup.
    let active=Arc::new(AtomicBool::new(true));
    let (stop_tx,stop_rx)=mpsc::channel::<()>();
    let heartbeat_db=db.clone();
    let heartbeat_lease=lease.clone();
    let heartbeat_active=Arc::clone(&active);
    let heartbeat=std::thread::spawn(move||{
        loop {
            match stop_rx.recv_timeout(Duration::from_secs(45)){
                Ok(())|Err(RecvTimeoutError::Disconnected)=>break,
                Err(RecvTimeoutError::Timeout)=>{
                    if !matches!(heartbeat_db.renew_db_backup_lease(&heartbeat_lease),Ok(true)){
                        heartbeat_active.store(false,Ordering::Release);
                        eprintln!("DB backup heartbeat lost lease or PostgreSQL quorum");
                        break;
                    }
                }
            }
        }
    });
    let result=perform(&db,&lease,&active);
    let _=stop_tx.send(());
    if heartbeat.join().is_err(){active.store(false,Ordering::Release);}
    let result=if !active.load(Ordering::Acquire){
        Err("PostgreSQL lease heartbeat failed; manual reconciliation required".into())
    }else{result};
    let outcome=match &result {Ok(_) => "success",Err(_)=>"failed"};
    let value=result.as_ref().ok();
    let error_code=if result.is_err(){Some("encrypted_db_snapshot_failed")}else{None};
    if !matches!(db.finish_db_backup(&lease,value,error_code),Ok(true)){
        eprintln!("DB backup {} requires reconciliation; refusing false success",outcome);
    }
}
pub async fn spawn(db:Database,replica:String){
    let mut tick=tokio::time::interval(Duration::from_secs(60));
    loop {
        tick.tick().await;
        let copy=db.clone();
        let id=replica.clone();
        let claimed=tokio::task::spawn_blocking(move||copy.claim_db_backup(&id,false)).await;
        match claimed {
            Ok(Ok(Some(lease)))=>{
                let copy=db.clone();
                let id=replica.clone();
                tokio::task::spawn_blocking(move||execute(copy,id,lease));
            }
            Ok(Ok(None))=>(),
            _=>eprintln!("Internal DB backup scheduling unavailable; failing closed")
        }
    }
}
pub fn backup_configured()->bool{
    env::var_os("MULTI_BACKUP_DB_BACKUP_S3_CONFIG").is_some()
}
pub fn require_ready()->Result<()>{
    let c=config()?;
    if !Path::new(&env::var("MULTI_BACKUP_DB_BACKUP_STAGE_ROOT")
      .unwrap_or_else(|_|"/var/lib/multi-backup/db-staging".into())).is_absolute(){
      return Err("Invalid DB backup staging path".into());
    }
    c.validate()?;
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn no_production_backup_without_separate_s3_config(){
        let path=std::env::var("MULTI_BACKUP_DB_BACKUP_STAGE_ROOT")
           .unwrap_or_else(|_|"/var/lib/multi-backup/db-staging".into());
        assert!(Path::new(&path).is_absolute());
    }
}
