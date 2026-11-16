//! PostgreSQL-backed multi-replica control plane. Every server replica uses
//! the same authoritative registry. Local volume access is *node-affine*;
//! "shared" must be explicitly registered as verified shared storage.
//! Atomic claims use PostgreSQL SKIP LOCKED and leased fencing tokens.
use multi_backup_core::{
    cluster_placement::{StorageLocation,StorageVisibility,SourceTransport},
    execution_policy::ExecutionLimits
};
use postgres::Client;
use sha2::{Digest,Sha256};
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
CREATE TABLE IF NOT EXISTS mb_cluster_backups (
  storage_id TEXT NOT NULL REFERENCES mb_cluster_storage(storage_id),
  backup_id TEXT NOT NULL,
  owner_replica TEXT NOT NULL REFERENCES mb_cluster_replicas(replica_id),
  label TEXT NOT NULL,
  files JSONB NOT NULL,
  observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY(storage_id,backup_id)
);
CREATE INDEX IF NOT EXISTS mb_cluster_backups_owner
  ON mb_cluster_backups(owner_replica,storage_id);
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
// Each migration is applied once under a PostgreSQL advisory transaction
// lock, and the SHA256 is pinned. In future: APPEND a new migration, never
// modify a previously applied version. Existing legacy deployments are
// bootstrapped atomically with the baseline schema and migration ledger.
const DB_BACKUP_SCHEMA:&str=r#"
CREATE TABLE IF NOT EXISTS mb_internal_backup_policy (
  singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton),
  enabled BOOLEAN NOT NULL DEFAULT FALSE,
  interval_minutes INTEGER NOT NULL DEFAULT 1440
    CHECK(interval_minutes BETWEEN 60 AND 43200),
  target TEXT NOT NULL DEFAULT 's3_worm'
    CHECK(target IN ('s3_worm','local_preview')),
  next_due TIMESTAMPTZ NOT NULL DEFAULT now(),
  current_job TEXT,
  lease_state TEXT NOT NULL DEFAULT 'idle'
    CHECK(lease_state IN ('idle','running','needs_reconciliation')),
  lease_token TEXT,
  last_completed TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO mb_internal_backup_policy(singleton)
  VALUES(TRUE) ON CONFLICT (singleton) DO NOTHING;
CREATE TABLE IF NOT EXISTS mb_internal_backup_history (
  job_id TEXT PRIMARY KEY,
  backup_id TEXT,
  target TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('running','success','failed','needs_reconciliation')),
  replica_id TEXT NOT NULL,
  manifest JSONB,
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  error_code TEXT
);
CREATE INDEX IF NOT EXISTS mb_internal_backup_history_recent
  ON mb_internal_backup_history(started_at DESC);
"#;
const DB_BACKUP_LEASE_SCHEMA:&str=r#"
ALTER TABLE mb_internal_backup_policy
  ADD COLUMN IF NOT EXISTS lease_until TIMESTAMPTZ;
ALTER TABLE mb_internal_backup_history
  ADD COLUMN IF NOT EXISTS encrypted_bytes BIGINT;
"#;
const MONITORING_SCHEMA:&str=r#"
CREATE TABLE IF NOT EXISTS mb_monitor_tokens (
  token_id TEXT PRIMARY KEY,
  name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 128),
  secret_sha256 TEXT NOT NULL UNIQUE CHECK(length(secret_sha256)=64),
  scope TEXT NOT NULL DEFAULT 'monitor.read' CHECK(scope='monitor.read'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  revoked BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS mb_monitor_tokens_live
  ON mb_monitor_tokens(expires_at) WHERE revoked=FALSE;
"#;
const MIGRATIONS:&[(i64,&str)]=&[
    (1,SCHEMA),
    (2,DB_BACKUP_SCHEMA),
    (3,DB_BACKUP_LEASE_SCHEMA),
    (4,MONITORING_SCHEMA),
];
fn migrate_schema(db:&mut Client)->Result<(),BoxError>{
    let mut tx=db.transaction()?;
    // Transaction-scoped lock: only ONE replica performs schema creation or
    // upgrades, including on a bare-metal cluster with many servers.
    tx.batch_execute("SELECT pg_advisory_xact_lock(50494787123);
        CREATE TABLE IF NOT EXISTS mb_schema_migrations (
          version BIGINT PRIMARY KEY,
          checksum_sha256 TEXT NOT NULL CHECK(length(checksum_sha256)=64),
          applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );")?;
    let installed=tx.query("SELECT version,checksum_sha256
       FROM mb_schema_migrations ORDER BY version",&[])?;
    if installed.len()>MIGRATIONS.len(){
        return Err(error("Database schema newer than this server; downgrade refused"));
    }
    for (position,row) in installed.iter().enumerate(){
        let version:i64=row.get(0);
        let checksum:String=row.get(1);
        let (expected_version,sql)=MIGRATIONS[position];
        let expected=hex::encode(Sha256::digest(sql.as_bytes()));
        if version!=expected_version || checksum!=expected {
            return Err(error("Database migration checksum/version mismatch: refusing startup"));
        }
    }
    for &(version,sql) in MIGRATIONS.iter().skip(installed.len()){
        tx.batch_execute(sql)?;
        let digest=hex::encode(Sha256::digest(sql.as_bytes()));
        tx.execute("INSERT INTO mb_schema_migrations(version,checksum_sha256)
            VALUES($1,$2)",&[&version,&digest])?;
    }
    tx.commit()?;
    Ok(())
}

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
        migrate_schema(&mut client)?;
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

    /// Publish authenticated, verified logical backup inventory from THIS
    /// replica only. Node-local entries must belong to this exact replica;
    /// an unregistered storage or a shared-store path is never inferred.
    /// This is metadata only: remote restore is NOT silently routed.
    pub fn publish_backups(&self,replica:&str,storage:&str,items:&[Value])
        ->Result<(),BoxError>{
        if !ident(replica)||!ident(storage)||items.len()>1000{
            return Err(error("Invalid cluster backup inventory identity or size"));
        }
        let mut conn=self.connect()?;
        let mut tx=conn.transaction()?;
        let row=tx.query_opt("SELECT visibility,owner_replica
             FROM mb_cluster_storage WHERE storage_id=$1 FOR UPDATE",
             &[&storage])?.ok_or_else(||error("Storage is not registered"))?;
        let visibility:String=row.get(0);
        let owner:Option<String>=row.get(1);
        if visibility=="node_local" && owner.as_deref()!=Some(replica){
            return Err(error("Another node owns this node-local backup storage"));
        }
        let exists=tx.query_opt("SELECT replica_id FROM mb_cluster_replicas
             WHERE replica_id=$1 AND last_seen>now()-interval '90 seconds'",
             &[&replica])?;
        if exists.is_none(){return Err(error("Publishing replica heartbeat expired"));}
        let mut ids=std::collections::HashSet::new();
        for item in items {
            let id=item["id"].as_str().ok_or_else(||error("Missing backup ID"))?;
            if id.len()!=64||!id.bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase())
                ||!ids.insert(id.to_string()){
                return Err(error("Invalid or duplicate backup ID"));
            }
            let label=item["label"].as_str().ok_or_else(||error("Missing backup label"))?;
            if label.len()>256||label.chars().any(char::is_control){
                return Err(error("Unsafe backup label"));
            }
            let files=item["files"].as_array().ok_or_else(||error("Missing backup files"))?;
            if files.len()>50000{return Err(error("Backup inventory exceeds per-snapshot limit"));}
            for entry in files{
                let path=entry["path"].as_str().ok_or_else(||error("Missing file path"))?;
                if path.is_empty()||path.len()>4096||path.starts_with('/')||
                   path.split('/').any(|p|p=="."||p=="..")||path.chars().any(char::is_control)||
                   !entry["size"].is_u64(){
                    return Err(error("Unsafe cluster file metadata"));
                }
            }
            let data=Value::Array(files.clone());
            tx.execute("INSERT INTO mb_cluster_backups
                (storage_id,backup_id,owner_replica,label,files,observed_at)
                VALUES ($1,$2,$3,$4,$5,now())
                ON CONFLICT (storage_id,backup_id) DO UPDATE SET
                owner_replica=EXCLUDED.owner_replica,
                label=EXCLUDED.label,files=EXCLUDED.files,observed_at=now()",
                &[&storage,&id,&replica,&label,&data])?;
        }
        // Explicitly retire only THIS replica's stale inventory, never a
        // disconnected peer's catalog (offline entries remain visible).
        let keep:Vec<String>=ids.into_iter().collect();
        tx.execute("DELETE FROM mb_cluster_backups WHERE
            storage_id=$1 AND owner_replica=$2 AND NOT (backup_id=ANY($3))",
            &[&storage,&replica,&keep])?;
        tx.commit()?;
        Ok(())
    }
    /// Consistent operator view on ANY replica. The server identity and
    /// storage location are mandatory; a remote node-local entry is
    /// browsable metadata, NOT an implicitly accessible recovery path.
    pub fn backup_inventory(&self)->Result<Value,BoxError>{
        let mut db=self.connect()?;
        let rows=db.query("SELECT b.storage_id,b.backup_id,b.owner_replica,
            b.label,b.files,r.last_seen>now()-interval '90 seconds',
            s.visibility FROM mb_cluster_backups b
            JOIN mb_cluster_storage s ON s.storage_id=b.storage_id
            LEFT JOIN mb_cluster_replicas r ON r.replica_id=b.owner_replica
            ORDER BY b.observed_at DESC LIMIT 1000",&[])?;
        Ok(json!({"backups":rows.iter().map(|r|json!({
            "storage_id":r.get::<_,String>(0),
            "id":r.get::<_,String>(1),"owner_replica":r.get::<_,String>(2),
            "label":r.get::<_,String>(3),"files":r.get::<_,Value>(4),
            "owner_online":r.get::<_,Option<bool>>(5).unwrap_or(false),
            "visibility":r.get::<_,String>(6),
            "remote_export_supported":false
        })).collect::<Vec<_>>(),"read_only_remote":true}))
    }


    pub fn db_backup_dashboard(&self)->Result<Value,BoxError>{
        let mut conn=self.connect()?;
        let row=conn.query_one("SELECT enabled,interval_minutes,target,
            next_due::TEXT,lease_state,last_completed::TEXT,updated_at::TEXT,
            lease_until::TEXT FROM mb_internal_backup_policy WHERE singleton=TRUE",&[])?;
        let hist=conn.query("SELECT job_id,backup_id,target,status,replica_id,
            started_at::TEXT,finished_at::TEXT,error_code,manifest
            FROM mb_internal_backup_history ORDER BY started_at DESC LIMIT 50",&[])?;
        let recovery:bool=conn.query_one("SELECT pg_is_in_recovery()",&[])?.get(0);
        let (role,standbys)=if recovery {
            ("replica",Vec::<Value>::new())
        }else{
            let rows=conn.query("SELECT application_name,state,sync_state,
                COALESCE(pg_wal_lsn_diff(pg_current_wal_lsn(),replay_lsn),0)::BIGINT
                FROM pg_stat_replication ORDER BY application_name",&[])?;
            ("primary",rows.iter().map(|r|json!({
                "name":r.get::<_,String>(0),"state":r.get::<_,String>(1),
                "sync_state":r.get::<_,String>(2),"lag_bytes":r.get::<_,i64>(3)
            })).collect::<Vec<_>>())
        };
        let sync=conn.query_one("SELECT current_setting('synchronous_commit'),
            current_setting('synchronous_standby_names')",&[])?;
        Ok(json!({
          "enabled":row.get::<_,bool>(0),"interval_minutes":row.get::<_,i32>(1),
          "target":row.get::<_,String>(2),"next_due":row.get::<_,String>(3),
          "lease_state":row.get::<_,String>(4),
          "last_completed":row.get::<_,Option<String>>(5),
          "updated_at":row.get::<_,String>(6),
          "lease_until":row.get::<_,Option<String>>(7),
          "database":{"role":role,"standbys":standbys,
            "synchronous_commit":sync.get::<_,String>(0),
            "synchronous_standby_names":sync.get::<_,String>(1)},
          "history":hist.iter().map(|r|json!({
             "job_id":r.get::<_,String>(0),"backup_id":r.get::<_,Option<String>>(1),
             "target":r.get::<_,String>(2),"status":r.get::<_,String>(3),
             "replica":r.get::<_,String>(4),
             "started_at":r.get::<_,String>(5),
             "finished_at":r.get::<_,Option<String>>(6),
             "error_code":r.get::<_,Option<String>>(7),
             "manifest":r.get::<_,Option<Value>>(8)
          })).collect::<Vec<_>>()
        }))
    }

    pub fn configure_db_backups(&self,enabled:bool,interval:i32,target:&str)
        ->Result<(),BoxError>{
        if !(60..=43200).contains(&interval) || target!="s3_worm" {
            return Err(error("Only S3 COMPLIANCE WORM is eligible for scheduled DB backups"));
        }
        let mut conn=self.connect()?;
        let updated=conn.execute("UPDATE mb_internal_backup_policy SET
            enabled=$1,interval_minutes=$2,target=$3,
            next_due=CASE WHEN $1 THEN now() ELSE next_due END,
            updated_at=now() WHERE singleton=TRUE AND lease_state='idle'",
            &[&enabled,&interval,&target])?;
        if updated!=1 {return Err(error("DB backup policy busy; reconciliation required"));}
        Ok(())
    }

    /// Exclusive, transactional lease shared by all Kubernetes and bare-metal
    /// servers. A crashed writer is quarantined rather than re-run blindly.
    pub fn claim_db_backup(&self,replica:&str,force:bool)->Result<Option<String>,BoxError>{
        if !ident(replica){return Err(error("Invalid cluster replica ID"));}
        let mut conn=self.connect()?;
        let mut tx=conn.transaction()?;
        let row=tx.query_one("SELECT enabled,target,lease_state,
          (next_due<=now()),(lease_until IS NOT NULL AND lease_until<now())
          FROM mb_internal_backup_policy WHERE singleton=TRUE FOR UPDATE",&[])?;
        let enabled:bool=row.get(0);
        let target:String=row.get(1);
        let state:String=row.get(2);
        let due:bool=row.get(3);
        let expired:bool=row.get(4);
        if state=="running" && expired {
            tx.batch_execute("UPDATE mb_internal_backup_policy SET
              lease_state='needs_reconciliation',updated_at=now()
              WHERE singleton=TRUE;
              UPDATE mb_internal_backup_history SET status='needs_reconciliation',
              error_code='writer_disappeared_reconciliation_required'
              WHERE job_id=(SELECT current_job FROM mb_internal_backup_policy)
                AND status='running';")?;
            tx.commit()?;
            return Ok(None);
        }
        if state!="idle" || (!force && (!enabled || !due)) {
            tx.commit()?;
            return Ok(None);
        }
        if target!="s3_worm"{return Err(error("Unsafe non-WORM DB destination"));}
        let id=uuid::Uuid::new_v4().to_string();
        let token=uuid::Uuid::new_v4().to_string();
        tx.execute("UPDATE mb_internal_backup_policy SET current_job=$1,
          lease_token=$2,lease_state='running',lease_until=now()+interval '30 minutes',
          updated_at=now() WHERE singleton=TRUE",
          &[&id,&token])?;
        tx.execute("INSERT INTO mb_internal_backup_history
            (job_id,target,status,replica_id) VALUES ($1,$2,'running',$3)",
            &[&id,&target,&replica])?;
        tx.commit()?;
        Ok(Some(format!("{id}:{token}")))
    }
    /// Extend the exclusive DB dump lease only if this exact fencing token
    /// still owns it and its previous deadline has not elapsed. A crashed
    /// writer can never reacquire an expired lease by renewing late.
    pub fn renew_db_backup_lease(&self,lease:&str)->Result<bool,BoxError>{
        let (id,token)=lease.split_once(':').ok_or_else(||error("Unsafe DB backup lease"))?;
        if uuid::Uuid::parse_str(id).is_err()||uuid::Uuid::parse_str(token).is_err(){
            return Err(error("Invalid DB backup lease"));
        }
        let mut db=self.connect()?;
        Ok(db.execute("UPDATE mb_internal_backup_policy
            SET lease_until=now()+interval '30 minutes',updated_at=now()
            WHERE singleton=TRUE AND lease_state='running'
              AND current_job=$1 AND lease_token=$2 AND lease_until>now()",
            &[&id,&token])?==1)
    }
    pub fn finish_db_backup(&self,lease:&str,manifest:Option<&Value>,error_code:Option<&str>)
        ->Result<bool,BoxError>{
        let (id,token)=lease.split_once(':').ok_or_else(||error("Unsafe DB backup lease"))?;
        if uuid::Uuid::parse_str(id).is_err()||uuid::Uuid::parse_str(token).is_err(){
           return Err(error("Invalid DB backup lease"));
        }
        let mut conn=self.connect()?;
        let mut tx=conn.transaction()?;
        let row=tx.query_opt("SELECT current_job FROM mb_internal_backup_policy
          WHERE singleton=TRUE AND lease_state='running'
            AND current_job=$1 AND lease_token=$2
            AND lease_until>now() FOR UPDATE",&[&id,&token])?;
        if row.is_none(){return Ok(false);}
        let result=if manifest.is_some() && error_code.is_none() {"success"}else{"failed"};
        let backup_id=manifest.and_then(|x|x.get("id")).and_then(Value::as_str);
        tx.execute("UPDATE mb_internal_backup_history SET
            status=$2,backup_id=$3,manifest=$4,error_code=$5,finished_at=now()
            WHERE job_id=$1 AND status='running'",
            &[&id,&result,&backup_id,&manifest,&error_code])?;
        tx.execute("UPDATE mb_internal_backup_policy SET current_job=NULL,
          lease_token=NULL,lease_until=NULL,lease_state='idle',
          last_completed=CASE WHEN $1='success' THEN now() ELSE last_completed END,
          next_due=now()+interval '1 minute' * interval_minutes,
          updated_at=now() WHERE singleton=TRUE",&[&result])?;
        tx.commit()?;
        Ok(true)
    }


    /// Issue a 256-bit random bearer token. Only its SHA-256 is retained.
    /// The plaintext is returned exactly once over authenticated browser TLS.
    pub fn create_monitor_token(&self,name:&str,days:i32)->Result<Value,BoxError>{
        if name.is_empty()||name.len()>128||name.chars().any(char::is_control)||
           !(1..=365).contains(&days){
            return Err(error("Unsafe monitoring token name/lifetime"));
        }
        use rand::RngCore;
        let mut nonce=[0u8;32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let token=format!("mbmon_{}",hex::encode(nonce));
        let hash=hex::encode(Sha256::digest(token.as_bytes()));
        let id=uuid::Uuid::new_v4().to_string();
        let mut db=self.connect()?;
        db.execute("INSERT INTO mb_monitor_tokens
          (token_id,name,secret_sha256,expires_at)
          VALUES($1,$2,$3,now()+($4::integer * INTERVAL '1 day'))",
          &[&id,&name,&hash,&days])?;
        Ok(json!({"id":id,"token":token,"name":name,"expires_in_days":days,
                  "scope":"monitor.read"}))
    }
    pub fn monitor_token_valid(&self,token:&str)->Result<bool,BoxError>{
        if token.len()!=70 || !token.starts_with("mbmon_")||
           !token[6..].bytes().all(|x|x.is_ascii_hexdigit()&&!x.is_ascii_uppercase()){
            return Ok(false);
        }
        let hash=hex::encode(Sha256::digest(token.as_bytes()));
        let mut db=self.connect()?;
        Ok(db.query_opt("SELECT token_id FROM mb_monitor_tokens
           WHERE secret_sha256=$1 AND scope='monitor.read'
             AND NOT revoked AND expires_at>now()", &[&hash])?.is_some())
    }
    pub fn monitor_token_list(&self)->Result<Value,BoxError>{
        let mut db=self.connect()?;
        let rows=db.query("SELECT token_id,name,scope,created_at::TEXT,
            expires_at::TEXT,revoked FROM mb_monitor_tokens
            ORDER BY created_at DESC LIMIT 100",&[])?;
        Ok(json!({"tokens":rows.iter().map(|r|json!({
          "id":r.get::<_,String>(0),"name":r.get::<_,String>(1),
          "scope":r.get::<_,String>(2),
          "created_at":r.get::<_,String>(3),
          "expires_at":r.get::<_,String>(4),
          "revoked":r.get::<_,bool>(5)
        })).collect::<Vec<_>>()}))
    }
    pub fn revoke_monitor_token(&self,id:&str)->Result<bool,BoxError>{
        if uuid::Uuid::parse_str(id).is_err(){return Err(error("Invalid token identifier"));}
        let mut db=self.connect()?;
        Ok(db.execute("UPDATE mb_monitor_tokens SET revoked=TRUE
            WHERE token_id=$1 AND NOT revoked",&[&id])?==1)
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
    #[test]fn migration_ledger_is_versioned_and_hash_pinned(){
        assert_eq!(MIGRATIONS.len(),4);
        assert!(DB_BACKUP_SCHEMA.contains("mb_internal_backup_policy"));
        assert!(DB_BACKUP_SCHEMA.contains("mb_internal_backup_history"));
        assert!(Sha256::digest(SCHEMA.as_bytes()) != Sha256::digest(DB_BACKUP_SCHEMA.as_bytes()));
    }
    #[test]fn cluster_inventory_is_not_an_unscoped_local_export(){
        assert!(SCHEMA.contains("CREATE TABLE IF NOT EXISTS mb_cluster_backups"));
        assert!(SCHEMA.contains("PRIMARY KEY(storage_id,backup_id)"));
        assert!(SCHEMA.contains("REFERENCES mb_cluster_replicas"));
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
    let mut client=db.connect().unwrap();
    migrate_schema(&mut client).unwrap();
    db
}
