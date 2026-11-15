//! PostgreSQL-backed multi-replica control plane. Every server replica uses
//! the same authoritative registry. Local volume access is *node-affine*;
//! "shared" must be explicitly registered as verified shared storage.
//! Atomic claims use PostgreSQL SKIP LOCKED and leased fencing tokens.
use multi_backup_core::{
    cluster_placement::{StorageLocation,StorageVisibility,SourceTransport},
    execution_policy::ExecutionLimits
};
use postgres::Client;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{env,fmt::Display};
use uuid::Uuid;

type BoxError=Box<dyn std::error::Error+Send+Sync>;
const SCHEMA:&str=r#"
CREATE TABLE IF NOT EXISTS mb_cluster_oidc_pending (
  state_sha256 TEXT PRIMARY KEY,
  verifier TEXT NOT NULL,
  nonce TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS mb_cluster_oidc_expires
 ON mb_cluster_oidc_pending(expires_at);
CREATE TABLE IF NOT EXISTS mb_cluster_sessions (
  token_sha256 TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  csrf_token TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS mb_cluster_sessions_expires
  ON mb_cluster_sessions(expires_at);
CREATE TABLE IF NOT EXISTS mb_cluster_storage (
  storage_id TEXT PRIMARY KEY,
  backend TEXT NOT NULL,
  visibility TEXT NOT NULL CHECK (visibility IN ('shared','node_local')),
  owner_replica TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK ((visibility='shared' AND owner_replica IS NULL)
      OR (visibility='node_local' AND owner_replica IS NOT NULL))
);
CREATE TABLE IF NOT EXISTS mb_cluster_replicas (
  replica_id TEXT PRIMARY KEY,
  max_jobs INTEGER NOT NULL CHECK (max_jobs BETWEEN 1 AND 256),
  storage_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
  last_seen TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS mb_cluster_jobs (
  job_id TEXT PRIMARY KEY,
  plugin_id TEXT NOT NULL,
  storage_id TEXT NOT NULL REFERENCES mb_cluster_storage(storage_id),
  source_transport TEXT NOT NULL,
  job_spec JSONB NOT NULL,
  execution_limits JSONB NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued','running','success','failed','cancelled','needs_reconciliation')),
  priority INTEGER NOT NULL DEFAULT 0,
  replica_id TEXT,
  lease_token TEXT,
  lease_until TIMESTAMPTZ,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  result JSONB,
  CHECK (priority BETWEEN -100 AND 100)
);
-- Migrations are forward-compatible with an existing r039 preview DB.
-- A crashed writer may continue running after losing PG connectivity: never
-- recycle its lease before proving the old process is stopped.
ALTER TABLE mb_cluster_jobs DROP CONSTRAINT IF EXISTS mb_cluster_jobs_status_check;
ALTER TABLE mb_cluster_jobs ADD CONSTRAINT mb_cluster_jobs_status_check
  CHECK (status IN ('queued','running','success','failed','cancelled','needs_reconciliation'));
CREATE INDEX IF NOT EXISTS mb_cluster_jobs_pending
 ON mb_cluster_jobs(status,priority DESC,created_at);
CREATE INDEX IF NOT EXISTS mb_cluster_jobs_lease
 ON mb_cluster_jobs(replica_id,lease_until) WHERE status='running';
CREATE TABLE IF NOT EXISTS mb_agent_registry (
  agent_id TEXT PRIMARY KEY,
  owner_user TEXT NOT NULL,
  device_public_key TEXT NOT NULL,
  token_sha256 TEXT NOT NULL,
  approved BOOLEAN NOT NULL DEFAULT FALSE,
  revoked BOOLEAN NOT NULL DEFAULT FALSE,
  last_sequence BIGINT NOT NULL DEFAULT 0,
  execution_limits JSONB NOT NULL DEFAULT '{}'::jsonb,
  last_seen TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS mb_agent_profiles (
  agent_id TEXT NOT NULL REFERENCES mb_agent_registry(agent_id),
  profile_id TEXT NOT NULL,
  profile_sha256 TEXT NOT NULL CHECK (char_length(profile_sha256)=64),
  provider TEXT NOT NULL,
  approved BOOLEAN NOT NULL DEFAULT FALSE,
  approved_at TIMESTAMPTZ,
  PRIMARY KEY(agent_id,profile_id)
);
CREATE TABLE IF NOT EXISTS mb_agent_request_nonces (
  agent_id TEXT NOT NULL REFERENCES mb_agent_registry(agent_id),
  sequence BIGINT NOT NULL,
  accepted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY(agent_id,sequence)
);
CREATE INDEX IF NOT EXISTS mb_agent_nonce_time
 ON mb_agent_request_nonces(accepted_at);
CREATE TABLE IF NOT EXISTS mb_agent_pull_jobs (
  job_id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES mb_agent_registry(agent_id),
  profile_id TEXT NOT NULL,
  profile_sha256 TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'queued'
    CHECK (state IN ('queued','leased','success','failed','cancelled')),
  lease_until TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS mb_agent_uploads (
  backup_id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES mb_agent_registry(agent_id),
  owner_user TEXT NOT NULL,
  profile_id TEXT NOT NULL,
  profile_sha256 TEXT NOT NULL,
  cell TEXT NOT NULL,
  volume TEXT NOT NULL,
  recipient TEXT NOT NULL,
  blob_path TEXT NOT NULL,
  encrypted_bytes BIGINT NOT NULL,
  ciphertext_sha256 TEXT NOT NULL,
  received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE mb_agent_uploads
  ADD COLUMN IF NOT EXISTS profile_sha256 TEXT;
"#;
fn error(message:impl Display)->BoxError{message.to_string().into()}
fn ident(value:&str)->bool{
    !value.is_empty()&&value.len()<=128&&
    value.bytes().all(|b|b.is_ascii_alphanumeric()||b"._-".contains(&b))
}
/// postgres 0.19 accepts sslmode=require but does not expose the libpq
/// verify-full enum. We require the ADMINISTRATOR to spell verify-full in
/// the URI, parse it structurally (no substring spoofing), then normalize
/// ONLY the wire transport mode to "require". The native-tls connector
/// below independently validates the server certificate AND DNS hostname.
fn strict_tls_dsn(input:&str)->Result<String,BoxError>{
    let mut uri=url::Url::parse(input)?;
    if !matches!(uri.scheme(),"postgresql"|"postgres")||
       uri.host_str().is_none()||uri.fragment().is_some()||
       uri.username().is_empty(){
        return Err(error("Cluster database requires explicit PostgreSQL TLS URI and host"));
    }
    let mut parameters=Vec::<(String,String)>::new();
    let mut count=0;
    for (key,value) in uri.query_pairs(){
        if key=="sslmode" {
            count+=1;
            if value!="verify-full"{
                return Err(error("Cluster requires sslmode=verify-full (no downgrade)"));
            }
        }else{
            // postgres-native-tls uses its OWN CA chain; libpq-style sslrootcert
            // is not passed through as a misleading or ignored parameter.
            if matches!(key.as_ref(),"host"|"hostaddr"|"sslrootcert"|"sslcert"|"sslkey"){
                return Err(error("Use URI hostname and MULTI_BACKUP_CLUSTER_TLS_CA_FILE for CA trust"));
            }
            parameters.push((key.into_owned(),value.into_owned()));
        }
    }
    if count!=1{return Err(error("Exactly one sslmode=verify-full is required"));}
    parameters.push(("sslmode".into(),"require".into()));
    uri.query_pairs_mut().clear().extend_pairs(
        parameters.iter().map(|(key,value)|(key.as_str(),value.as_str())));
    let sanitized=uri.to_string();
    let postgres:postgres::Config=sanitized.parse()?;
    if postgres.get_ssl_mode()!=postgres::config::SslMode::Require{
        return Err(error("PostgreSQL must require verified TLS"));
    }
    Ok(sanitized)
}
#[derive(Clone)]
pub struct Database {dsn:String}
impl Database {
    pub fn configured()->Result<Option<Self>,BoxError>{
        let Some(dsn)=env::var("MULTI_BACKUP_CLUSTER_DATABASE_URL").ok() else {
            return Ok(None)
        };
        // Parse before connecting: a URL containing sslmode=disable plus
        // another query value that merely mentions verify-full is rejected.
        // Native TLS verifies certificate chain and hostname, and the
        // underlying postgres 0.19 transport gets sslmode=require.
        let db=Self{dsn:strict_tls_dsn(&dsn)?};
        let mut client=db.connect()?;
        client.batch_execute(SCHEMA)?;
        Ok(Some(db))
    }
    pub(crate) fn connect(&self)->Result<Client,BoxError>{
        let mut builder=native_tls::TlsConnector::builder();
        builder.danger_accept_invalid_certs(false)
            .danger_accept_invalid_hostnames(false)
            .min_protocol_version(Some(native_tls::Protocol::Tlsv12));
        if let Some(ca)=env::var_os("MULTI_BACKUP_CLUSTER_TLS_CA_FILE"){
            let path=std::path::PathBuf::from(ca);
            let info=std::fs::symlink_metadata(&path)?;
            if !info.file_type().is_file()||info.len()>1_048_576{
                return Err(error("Invalid trusted PostgreSQL CA file"));
            }
            let cert=native_tls::Certificate::from_pem(&std::fs::read(path)?)?;
            builder.add_root_certificate(cert);
        }
        let connector=postgres_native_tls::MakeTlsConnector::new(builder.build()?);
        Ok(Client::connect(&self.dsn,connector)?)
    }
    pub fn put_oidc_pending(&self,state:&str,verifier:&str,nonce:&str)
        ->Result<(),BoxError>{
        if !(16..=512).contains(&state.len()) ||
           !(32..=512).contains(&verifier.len())||
           !(16..=512).contains(&nonce.len()){
            return Err(error("Unsafe OIDC pending challenge size"));
        }
        let digest=multi_backup_core::agent_protocol::hex_sha(state.as_bytes());
        let mut db=self.connect()?;
        // Shared and bounded across pods. The token exchange context is
        // consumed atomically below; the raw OIDC state is never stored.
        db.execute("INSERT INTO mb_cluster_oidc_pending
             (state_sha256,verifier,nonce,expires_at)
             VALUES ($1,$2,$3,now()+interval '10 minutes')",
             &[&digest,&verifier,&nonce])?;
        Ok(())
    }
    pub fn take_oidc_pending(&self,state:&str)
        ->Result<Option<(String,String)>,BoxError>{
        if !(16..=512).contains(&state.len()){
            return Err(error("Unsafe OIDC state"));
        }
        let digest=multi_backup_core::agent_protocol::hex_sha(state.as_bytes());
        let mut db=self.connect()?;
        let result=db.query_opt("DELETE FROM mb_cluster_oidc_pending
            WHERE state_sha256=$1 AND expires_at>now()
            RETURNING verifier,nonce",
            &[&digest])?;
        Ok(result.map(|row|(row.get(0),row.get(1))))
    }
    pub fn put_session(&self,token:&str,user:&str,csrf:&str)
        ->Result<(),BoxError>{
        if token.len()!=64 || csrf.len()!=64 ||
           !token.bytes().all(|v|v.is_ascii_hexdigit())||
           !csrf.bytes().all(|v|v.is_ascii_hexdigit())||
           user.is_empty()||user.len()>512{
            return Err(error("Invalid shared session identity"));
        }
        let digest=multi_backup_core::agent_protocol::hex_sha(token.as_bytes());
        let mut db=self.connect()?;
        db.execute("INSERT INTO mb_cluster_sessions
            (token_sha256,user_id,csrf_token,expires_at)
            VALUES ($1,$2,$3,now()+interval '8 hours')",
            &[&digest,&user,&csrf])?;
        Ok(())
    }
    pub fn get_session(&self,token:&str)->Result<Option<(String,String)>,BoxError>{
        if token.len()!=64 || !token.bytes().all(|v|v.is_ascii_hexdigit()){
            return Err(error("Invalid shared session token"));
        }
        let digest=multi_backup_core::agent_protocol::hex_sha(token.as_bytes());
        let mut db=self.connect()?;
        Ok(db.query_opt("SELECT user_id,csrf_token FROM mb_cluster_sessions
             WHERE token_sha256=$1 AND expires_at>now()",
            &[&digest])?.map(|r|(r.get(0),r.get(1))))
    }
    pub fn delete_session(&self,token:&str)->Result<(),BoxError>{
        if token.len()!=64{return Err(error("Invalid logout token"));}
        let digest=multi_backup_core::agent_protocol::hex_sha(token.as_bytes());
        let mut db=self.connect()?;
        db.execute("DELETE FROM mb_cluster_sessions WHERE token_sha256=$1",
            &[&digest])?;
        Ok(())
    }
    pub fn register_storage(&self,location:&StorageLocation)->Result<(),BoxError>{
        location.validate().map_err(error)?;
        let mut db=self.connect()?;
        let visibility=match location.visibility{
            StorageVisibility::Shared=>"shared",StorageVisibility::NodeLocal=>"node_local"
        };
        db.execute("INSERT INTO mb_cluster_storage
            (storage_id,backend,visibility,owner_replica) VALUES ($1,$2,$3,$4)
            ON CONFLICT (storage_id) DO UPDATE SET
              backend=EXCLUDED.backend,visibility=EXCLUDED.visibility,
              owner_replica=EXCLUDED.owner_replica",
            &[&location.storage_id,&location.backend,&visibility,&location.owner_replica])?;
        Ok(())
    }
    pub fn heartbeat_replica(&self,id:&str,capacity:u16,
                             accessible_storage:&[String])->Result<(),BoxError>{
        if !ident(id)||capacity==0||capacity>256||
           accessible_storage.iter().any(|s|!ident(s)){
            return Err(error("Invalid cluster replica identity/capacity/storage"));
        }
        let mut db=self.connect()?;
        let capabilities=json!(accessible_storage);
        db.execute("INSERT INTO mb_cluster_replicas
            (replica_id,max_jobs,storage_ids,last_seen)
            VALUES ($1,$2,$3,now())
            ON CONFLICT(replica_id) DO UPDATE SET max_jobs=EXCLUDED.max_jobs,
                storage_ids=EXCLUDED.storage_ids,last_seen=now()",
            &[&id,&(capacity as i32),&capabilities])?;
        Ok(())
    }
    pub fn enqueue(&self,id:&str,plugin:&str,storage:&str,
                   transport:SourceTransport,limits:&ExecutionLimits,
                   spec:&Value,priority:i32)->Result<(),BoxError>{
        if !ident(id)||!ident(plugin)||!ident(storage)||!(-100..=100).contains(&priority){
            return Err(error("Invalid cluster job fields"));
        }
        limits.validate().map_err(error)?;
        let transport=match transport{
            SourceTransport::ServerLocal=>"server_local",
            SourceTransport::SshAgentless=>"ssh_agentless",
            SourceTransport::ManagedAgentPull=>"managed_agent_pull",
            SourceTransport::ManagedAgentPush=>"managed_agent_push",
        };
        let limit_json=serde_json::to_value(limits)?;
        let mut db=self.connect()?;
        let result=db.execute("INSERT INTO mb_cluster_jobs
            (job_id,plugin_id,storage_id,source_transport,job_spec,execution_limits,priority)
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            ON CONFLICT(job_id) DO NOTHING",
            &[&id,&plugin,&storage,&transport,spec,&limit_json,&priority])?;
        if result!=1{return Err(error("Duplicate or previously used job ID"));}
        Ok(())
    }
    pub fn claim(&self,replica_id:&str,lease_secs:i32)
        ->Result<Option<Lease>,BoxError>{
        if !ident(replica_id)||!(30..=3600).contains(&lease_secs){
            return Err(error("Invalid replica or lease duration"));
        }
        let mut db=self.connect()?;
        let token=Uuid::new_v4().to_string();
        // Serialize admission on the target replica *before* taking the
        // capacity-count snapshot. PostgreSQL READ COMMITTED fixes the
        // snapshot per statement: a single CTE that waits on FOR UPDATE can
        // still count a stale set of running jobs and overbook the node.
        // This transaction keeps the row lock through the subsequent claim,
        // while the second statement observes newly committed active leases.
        let mut tx=db.transaction()?;
        let locked=tx.query_opt(
            "SELECT replica_id FROM mb_cluster_replicas
             WHERE replica_id=$1 AND last_seen>now()-interval '90 seconds'
             FOR UPDATE",&[&replica_id])?;
        if locked.is_none(){return Ok(None);}
        // Native UPDATE + candidate SKIP LOCKED makes cross-replica job
        // ownership unique. Capacity admission is serialized separately.
        // A shared store is eligible only when explicitly registered as shared
        // AND this live replica advertises the storage ID as accessible.
        // Locals may ONLY run on their exact owner replica.
        // Idle capacity is accounted for in the same DB transaction.
        let row=tx.query_opt(r#"
            WITH node AS MATERIALIZED (
              -- Serialize capacity admission for the SAME replica. SKIP LOCKED
              -- alone does not prevent multiple concurrent claims exceeding
              -- max_jobs when both observe the previous count.
              SELECT replica_id,max_jobs,storage_ids,last_seen
              FROM mb_cluster_replicas
              WHERE replica_id=$1 AND last_seen>now()-interval '90 seconds'
            ),
            candidate AS (
              SELECT j.job_id FROM mb_cluster_jobs j
              JOIN mb_cluster_storage s ON s.storage_id=j.storage_id
              JOIN node n ON n.replica_id=$1
              WHERE n.last_seen>now()-interval '90 seconds'
                AND n.storage_ids ? j.storage_id
                AND (s.visibility='shared' OR s.owner_replica=$1)
                AND j.status='queued'
                AND (
                  SELECT count(*) FROM mb_cluster_jobs running
                  WHERE running.replica_id=$1 AND running.status='running'
                        AND running.lease_until>now()
                ) < n.max_jobs
              ORDER BY j.priority DESC,j.created_at,j.job_id
              FOR UPDATE OF j SKIP LOCKED LIMIT 1
            )
            UPDATE mb_cluster_jobs j SET
                status='running',replica_id=$1,lease_token=$2,
                lease_until=now()+($3::int * interval '1 second'),
                attempts=j.attempts+1
            FROM candidate WHERE candidate.job_id=j.job_id
            RETURNING j.job_id,j.plugin_id,j.storage_id,
                j.source_transport,j.job_spec,j.execution_limits,
                j.lease_token,j.attempts
        "#,&[&replica_id,&token,&lease_secs])?;
        tx.commit()?;
        row.map(|r|{
            Ok(Lease{job_id:r.get(0),plugin_id:r.get(1),
                storage_id:r.get(2),source_transport:r.get(3),
                spec:r.get(4),limits:r.get(5),token:r.get(6),
                attempts:r.get(7)})
        }).transpose()
    }
    /// Expired or abandoned claimed jobs are quarantined, NOT reassigned.
    /// Without a durable manifest write fence, retrying a still-running
    /// worker could publish a second backup or race with garbage collection.
    /// Recover only after verified worker shutdown and storage reconciliation.
    /// Re-check a distributed lease using the PostgreSQL clock. A worker
    /// must call this before publishing a logical manifest, regardless of
    /// whether its local heartbeat thread has observed a network failure.
    pub fn is_current(&self,replica:&str,job:&str,token:&str)
        ->Result<bool,BoxError>{
        if !ident(replica)||!ident(job)||token.len()<32||token.len()>128{
            return Err(error("Invalid distributed lease identity"));
        }
        let mut conn=self.connect()?;
        let row=conn.query_one("SELECT EXISTS(
            SELECT 1 FROM mb_cluster_jobs
            WHERE job_id=$1 AND replica_id=$2 AND lease_token=$3
              AND status='running' AND lease_until>now())",
            &[&job,&replica,&token])?;
        Ok(row.get(0))
    }
    pub fn quarantine_expired(&self)->Result<u64,BoxError>{
        let mut db=self.connect()?;
        Ok(db.execute("UPDATE mb_cluster_jobs
            SET status='needs_reconciliation',
                result=jsonb_build_object(
                    'reason','lost_lease_requires_writer_fencing',
                    'previous_replica',replica_id,
                    'previous_attempts',attempts
                )
            WHERE status='running' AND lease_until<now()", &[])?)
    }
    /// Manual, audited recovery only. An offline heartbeat is not proof that
    /// an old writer stopped, therefore we require independent operator
    /// confirmation of both process termination and encrypted-store audit.
    /// A still-live replica is NEVER eligible for this operation.
    pub fn recover_quarantined(&self,job:&str,replica:&str,attempts:i32,
             writer_terminated:bool,store_reconciled:bool)
        ->Result<bool,BoxError>{
        if !ident(job)||!ident(replica)||attempts<1||
           !writer_terminated||!store_reconciled{
            return Err(error("Recovery requires writer termination and storage verification attestations"));
        }
        let mut conn=self.connect()?;
        let changed=conn.execute("UPDATE mb_cluster_jobs j SET
            status='queued',replica_id=NULL,lease_token=NULL,
            lease_until=NULL,finished_at=NULL,
            result=jsonb_build_object('recovery','manually_reconciled',
                'previous_attempts',j.attempts,'previous_replica',j.replica_id)
            WHERE j.job_id=$1 AND j.status='needs_reconciliation'
              AND j.replica_id=$2 AND j.attempts=$3
              AND NOT EXISTS (
                SELECT 1 FROM mb_cluster_replicas r
                WHERE r.replica_id=j.replica_id
                  AND r.last_seen>now()-interval '90 seconds'
              )",
            &[&job,&replica,&attempts])?;
        Ok(changed==1)
    }
    pub fn requires_reconciliation(&self)->Result<i64,BoxError>{
        let mut db=self.connect()?;
        let row=db.query_one("SELECT COUNT(*) FROM mb_cluster_jobs
            WHERE status='needs_reconciliation'",&[])?;
        Ok(row.get::<_,i64>(0))
    }
    pub fn renew(&self,replica:&str,job:&str,token:&str,seconds:i32)
        ->Result<bool,BoxError>{
        if !ident(replica)||!ident(job)||!(30..=3600).contains(&seconds){
            return Err(error("Invalid lease renewal"));
        }
        let mut db=self.connect()?;
        let n=db.execute("UPDATE mb_cluster_jobs
            SET lease_until=now()+($4::int*interval '1 second')
            WHERE job_id=$1 AND replica_id=$2 AND lease_token=$3
              AND status='running' AND lease_until>now()",
            &[&job,&replica,&token,&seconds])?;
        Ok(n==1)
    }
    pub fn finish(&self,replica:&str,job:&str,token:&str,
                  success:bool,result:&Value)->Result<bool,BoxError>{
        if !ident(replica)||!ident(job){return Err(error("Invalid job completion ID"));}
        let mut db=self.connect()?;
        let status=if success{"success"}else{"failed"};
        let n=db.execute("UPDATE mb_cluster_jobs SET
            status=$4, result=$5, finished_at=now(), lease_until=NULL
            WHERE job_id=$1 AND replica_id=$2 AND lease_token=$3
              AND status='running' AND lease_until>now()",
            &[&job,&replica,&token,&status,result])?;
        Ok(n==1)
    }
    pub fn status(&self)->Result<Value,BoxError>{
        let mut db=self.connect()?;
        let nodes=db.query("SELECT replica_id,max_jobs,storage_ids,
            last_seen>now()-interval '90 seconds'
            FROM mb_cluster_replicas ORDER BY replica_id", &[])?;
        let storages=db.query("SELECT storage_id,backend,visibility,owner_replica
            FROM mb_cluster_storage ORDER BY storage_id",&[])?;
        let jobs=db.query("SELECT job_id,plugin_id,storage_id,status,
            replica_id,attempts,source_transport FROM mb_cluster_jobs
            ORDER BY created_at DESC LIMIT 500",&[])?;
        Ok(json!({
            "replicas":nodes.iter().map(|r|json!({
                "id":r.get::<_,String>(0),"max_jobs":r.get::<_,i32>(1),
                "storage_ids":r.get::<_,Value>(2),"healthy":r.get::<_,bool>(3)
            })).collect::<Vec<_>>(),
            "storage":storages.iter().map(|r|json!({
                "id":r.get::<_,String>(0),"backend":r.get::<_,String>(1),
                "visibility":r.get::<_,String>(2),"owner_replica":r.get::<_,Option<String>>(3)
            })).collect::<Vec<_>>(),
            "jobs":jobs.iter().map(|r|json!({
                "id":r.get::<_,String>(0),"plugin":r.get::<_,String>(1),
                "storage":r.get::<_,String>(2),"status":r.get::<_,String>(3),
                "replica":r.get::<_,Option<String>>(4),"attempts":r.get::<_,i32>(5),
                "transport":r.get::<_,String>(6)
            })).collect::<Vec<_>>(),
        }))
    }
}
#[derive(Debug,Serialize,Deserialize)]
pub struct Lease {
    pub job_id:String,pub plugin_id:String,pub storage_id:String,
    pub source_transport:String,pub spec:Value,pub limits:Value,
    pub token:String,pub attempts:i32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn postgresql_tls_mode_is_structurally_enforced(){
        let good="postgresql://backup@db.example.org:5432/multi_backup?sslmode=verify-full";
        let verified=strict_tls_dsn(good).unwrap();
        assert!(verified.contains("sslmode=require"));
        assert!(!verified.contains("sslmode=verify-full"));
        assert!(strict_tls_dsn("postgresql://backup@db.example/multi_backup?sslmode=disable&application_name=sslmode%3Dverify-full").is_err());
        assert!(strict_tls_dsn("postgresql://backup@db.example/multi_backup?sslmode=verify-full&sslmode=disable").is_err());
        assert!(strict_tls_dsn("postgresql://backup@db.example/multi_backup?sslmode=prefer").is_err());
        assert!(strict_tls_dsn("postgresql://backup@db.example/multi_backup").is_err());
        assert!(strict_tls_dsn("postgresql:///multi_backup?sslmode=verify-full").is_err());
    }
    #[test]fn cluster_schema_has_fencing_and_storage_affinity(){
        assert!(SCHEMA.contains("lease_token"));
        assert!(SCHEMA.contains("owner_replica"));
        assert!(SCHEMA.contains("node_local"));
        assert!(SCHEMA.contains("storage_ids"));
    }
    #[test]fn expired_distributed_writers_must_be_quarantined(){
        assert!(SCHEMA.contains("needs_reconciliation"));
        assert!(SCHEMA.contains("lease_until"));
        assert!(SCHEMA.contains("mb_agent_profiles"));
        assert!(SCHEMA.contains("mb_cluster_sessions"));
    }
    #[test]fn rejects_invalid_registry_without_database(){
        let bad=StorageLocation{storage_id:"local".into(),backend:"posix".into(),
            visibility:StorageVisibility::NodeLocal,owner_replica:None};
        assert!(bad.validate().is_err());
    }
}

/// Test-only, isolated PostgreSQL service fixture. Production never uses
/// this constructor: Database::configured requires TLS verify-full.
#[cfg(test)]
pub(crate) fn live_pg_ci_fixture()->Database {
    let dsn=env::var("MULTI_BACKUP_TEST_PG_DSN")
        .expect("Real PG acceptance requires the disposable CI fixture");
    assert!(dsn.contains("@127.0.0.1:5432/") &&
            dsn.contains("sslmode=disable"),
            "CI fixture may only point to isolated loopback PostgreSQL");
    let db=Database{dsn};
    db.connect().unwrap().batch_execute(SCHEMA).unwrap();
    db
}
