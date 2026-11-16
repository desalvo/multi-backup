//! Server-owned PostgreSQL logical safety backups, not a physical WAL/PITR
//! replacement. Settings are persisted in PostgreSQL for all HA replicas.
//! pg_dump uses PGDATABASE environment: credentials never enter argv/logs.
//! A session advisory lock ensures exactly one scheduler executor per cluster.
use crate::cluster::Database;
use multi_backup_core::{archive,s3_vault};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{env,fs,path::{Path,PathBuf},process::{Command,Stdio}};
use uuid::Uuid;

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
const SCHEDULE_LOCK:i64=704148005382;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled:bool,
    pub interval_seconds:i64,
    pub destination:String,
    pub include_data:bool,
    pub retain_last:i32,
}
impl Policy {
    pub fn validate(&self)->Result<()>{
        if !(3600..=31_622_400).contains(&self.interval_seconds) ||
           !(1..=1000).contains(&self.retain_last) ||
           !matches!(self.destination.as_str(),"s3_worm"|"local_encrypted"){
            return Err("Invalid server database backup policy".into());
        }
        Ok(())
    }
}
fn load(db:&Database)->Result<(Policy,bool)>{
    let mut pg=db.connect()?;
    let row=pg.query_one("SELECT enabled,interval_seconds,destination,
         include_data,retain_last,
         (last_success_at IS NULL OR
          last_success_at + make_interval(secs => interval_seconds::double precision) <= now())
         FROM mb_server_database_policy WHERE policy_id='server'",&[])?;
    Ok((Policy{
        enabled:row.get(0),interval_seconds:row.get(1),
        destination:row.get(2),include_data:row.get(3),retain_last:row.get(4)
    },row.get(5)))
}
pub fn policy(db:&Database)->Result<Value>{
    let (policy,due)=load(db)?;
    Ok(json!({"policy":policy,"due":due,
        "consistency_note":"Database pg_dump is transactionally consistent; independent data-root copy is NOT an atomic application snapshot",
        "restore_policy":"never overwrite live PostgreSQL or the active data root"}))
}
pub fn change_policy(db:&Database,requested:&Policy)->Result<Value>{
    requested.validate()?;
    let mut pg=db.connect()?;
    pg.execute("UPDATE mb_server_database_policy SET
         enabled=$1,interval_seconds=$2,destination=$3,include_data=$4,
         retain_last=$5,updated_at=now() WHERE policy_id='server'",
         &[&requested.enabled,&requested.interval_seconds,&requested.destination,
           &requested.include_data,&requested.retain_last])?;
    policy(db)
}
pub fn list(db:&Database)->Result<Value>{
    let mut pg=db.connect()?;
    let rows=pg.query("SELECT backup_id,destination,storage_reference,data_reference,
         archive_sha256,recovery_consistency,schema_version,
         to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
         FROM mb_server_database_backups ORDER BY created_at DESC LIMIT 100",&[])?;
    Ok(json!({"items":rows.iter().map(|r|json!({
        "id":r.get::<_,String>(0),"destination":r.get::<_,String>(1),
        "db_snapshot":r.get::<_,String>(2),"data_snapshot":r.get::<_,Option<String>>(3),
        "archive_sha256":r.get::<_,String>(4),
        "consistency":r.get::<_,String>(5),"schema_version":r.get::<_,i32>(6),
        "created_at":r.get::<_,String>(7)
    })).collect::<Vec<_>>(),"outside_database_recovery":"retain the snapshot IDs independently; this catalog is lost with the DB"}))
}
fn pg_dump_original_connection(db:&Database,target:&Path)->Result<()>{
    let dsn=db.backup_dsn()?;
    let mut command=Command::new("pg_dump");
    command.arg("--format=custom").arg("--no-owner").arg("--no-acl")
        .arg("--file").arg(target).env("PGDATABASE",dsn)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(ca)=env::var_os("MULTI_BACKUP_CLUSTER_TLS_CA_FILE"){
        command.env("PGSSLROOTCERT",ca);
    }
    if !command.status()?.success(){
        return Err("pg_dump failed; no recovery point published".into());
    }
    let meta=fs::symlink_metadata(target)?;
    if !meta.file_type().is_file() || meta.len()<128{
        return Err("pg_dump output is not a regular nonempty custom-format archive".into());
    }
    if !Command::new("pg_restore").arg("--list").arg(target)
        .stdout(Stdio::null()).stderr(Stdio::null()).status()?.success(){
        return Err("pg_restore could not parse the generated PostgreSQL dump".into());
    }
    Ok(())
}
fn local_storage(root:&Path,live:&Path)->Result<PathBuf>{
    let real=fs::canonicalize(root)?;
    let data=fs::canonicalize(live)?;
    if !real.is_dir()||real.starts_with(&data)||data.starts_with(&real) {
        return Err("Database safety backup root must be a separate real directory".into());
    }
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if fs::symlink_metadata(&real)?.permissions().mode()&0o077!=0 {
            return Err("Local database backup root requires private mode 0700".into());
        }
    }
    Ok(real)
}
fn local_archive(source:&Path,root:&Path,id:&str,recipient:&str,identity:&Path)
    ->Result<(String,String)>
{
    let output=root.join(format!("mb-server-db-{id}.tar.gz.age"));
    archive::create(source,&output,recipient,&format!("server-db/{id}"))?;
    archive::verify(&output,identity)?;
    // sha256 is for display and correlation; authenticated age verification
    // above is the independent cryptographic integrity check.
    let digest=multi_backup_core::agent_protocol::hex_sha(&fs::read(&output)?);
    Ok((output.display().to_string(),digest))
}
/// When called by schedulers on multiple server replicas, exactly one
/// PostgreSQL session can hold this lock. Manual invocation uses the same
/// lock to avoid overlapping operational recovery points.
///
/// The two independent snapshots are marked non-atomic unless an external
/// write-freeze/data-snapshot mechanism has been established.
pub fn run(db:&Database,config:Option<s3_vault::Config>,
           data_root:&Path,identity:Option<&Path>,local_root:Option<&Path>,
           manual:bool)->Result<Option<Value>>
{
    let (settings,due)=load(db)?;
    settings.validate()?;
    if !manual && (!settings.enabled||!due) {return Ok(None);}
    let mut guard=db.connect()?;
    let lock:bool=guard.query_one("SELECT pg_try_advisory_lock($1)",
        &[&SCHEDULE_LOCK])?.get(0);
    if !lock{return Ok(None);}
    let result=(||->Result<Value>{
        // Recheck after acquisition: another worker might have completed.
        if !manual {
            let (current,still_due)=load(db)?;
            if !current.enabled||!still_due{return Ok(json!({"skipped":"not_due"}));}
        }
        if guard.query_one("SELECT pg_is_in_recovery()",&[])?.get::<_,bool>(0){
            return Err("Database backups must run against the HA primary".into());
        }
        let id=Uuid::new_v4().simple().to_string();
        let staging=tempfile::tempdir()?;
        let dump=staging.path().join("server.pgdump");
        let (recipient,target_dir)=match settings.destination.as_str(){
            "s3_worm"=>{
                let vault=config.as_ref().ok_or("S3 WORM backup destination not configured")?;
                s3_vault::preflight(vault)?;
                (vault.recipient.clone(),None)
            },
            "local_encrypted"=>{
                let recipient=env::var("MULTI_BACKUP_SERVER_DB_AGE_RECIPIENT")?;
                let location=local_root.ok_or("Local database backup root not configured")?;
                let dir=local_storage(location,data_root)?;
                if recipient.trim().len()<24 || !recipient.starts_with("age1"){
                    return Err("Local database backup requires a valid age recipient".into());
                }
                (recipient,Some(dir))
            },
            _=>return Err("Unsupported server DB backup storage".into())
        };
        let identity=identity.ok_or("Age identity must be configured to verify a server DB safety backup")?;
        let data_reference=if settings.include_data {
            if let Some(vault)=config.as_ref().filter(|_|settings.destination=="s3_worm"){
                let record=s3_vault::backup(vault,data_root)?;
                s3_vault::verify(vault,&record.id,identity)?;
                Some(record.id)
            }else{
                let path=target_dir.as_ref().ok_or("Local output destination missing")?;
                let (label,_)=local_archive(data_root,path,
                    &format!("{id}-data"),&recipient,identity)?;
                Some(label)
            }
        } else {None};
        pg_dump_original_connection(db,&dump)?;
        let schema:i32=guard.query_one(
            "SELECT COALESCE(MAX(version),0) FROM mb_schema_migrations",&[])?.get(0);
        let consistency=if data_reference.is_some(){
            "sequential_uncoordinated"
        }else{"database_snapshot"};
        let manifest=json!({
            "schema":1,"type":"multi_backup_server_database_recovery",
            "id":id,"schema_version":schema,
            "database_file":"server.pgdump",
            "data_snapshot":data_reference,
            "consistency":consistency,
            "data_root_restore_requires_offline_service":true,
            "restore_must_target_new_empty_database":true
        });
        fs::write(staging.path().join("recovery.json"),
            serde_json::to_vec_pretty(&manifest)?)?;
        let (reference,digest)=if settings.destination=="s3_worm" {
            let vault=config.as_ref().ok_or("S3 config missing")?;
            let record=s3_vault::backup(vault,staging.path())?;
            s3_vault::verify(vault,&record.id,identity)?;
            (record.id,record.sha256)
        }else{
            local_archive(staging.path(),target_dir.as_ref()
                .ok_or("Local directory missing")?,&id,&recipient,identity)?
        };
        guard.execute("INSERT INTO mb_server_database_backups(
           backup_id,destination,storage_reference,data_reference,
           archive_sha256,recovery_consistency,schema_version)
           VALUES ($1,$2,$3,$4,$5,$6,$7)",&[
               &id,&settings.destination,&reference,&data_reference,
               &digest,&consistency,&schema
           ])?;
        guard.execute("UPDATE mb_server_database_policy SET
             last_success_at=now() WHERE policy_id='server'",&[])?;
        Ok(json!({"id":id,"destination":settings.destination,
            "db_snapshot":reference,"data_snapshot":data_reference,
            "archive_sha256":digest,"schema_version":schema,
            "consistency":consistency,"restore_is_offline_only":true}))
    })();
    // The session lock is also released by PostgreSQL if this connection
    // dies during a failover; unsuccessful runs never advance last_success.
    let _=guard.query_one("SELECT pg_advisory_unlock($1)",&[&SCHEDULE_LOCK]);
    result.map(Some)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn policy_rejects_unknown_destinations_and_unsafe_intervals(){
        let mut p=Policy{enabled:true,interval_seconds:86400,
            destination:"s3_worm".into(),include_data:false,retain_last:5};
        assert!(p.validate().is_ok());
        p.destination="unencrypted".into();assert!(p.validate().is_err());
        p.destination="local_encrypted".into();assert!(p.validate().is_ok());
        p.interval_seconds=60;assert!(p.validate().is_err());
        p.interval_seconds=86400;p.retain_last=0;
        assert!(p.validate().is_err());
    }
}
