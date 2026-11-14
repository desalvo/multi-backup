//! Per-device registration, explicit approval, revocation and Ed25519
//! anti-spoof/replay checks backed by shared PostgreSQL, not in-process maps.
//! This code never issues a certificate bypass and never authorizes devices
//! merely because a valid token is presented before administrator approval.
use crate::cluster::Database;
use multi_backup_core::{agent_protocol::{self as wire,AgentProof},execution_policy::ExecutionLimits};
use base64::{engine::general_purpose::STANDARD as B64,Engine as _};
use serde_json::{json,Value};
use subtle::ConstantTimeEq;
use axum::http::HeaderMap;
use rand::RngCore;
use std::error::Error;

type Res<T>=Result<T,Box<dyn Error+Send+Sync>>;
fn fail(s:&str)->Box<dyn Error+Send+Sync>{s.to_owned().into()}
#[derive(Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    pub agent_id:String,
    pub owner_user:String,
    pub device_public_key:String,
}
#[derive(Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentChange {pub agent_id:String}
#[derive(Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullJob{
    pub agent_id:String,pub profile_id:String,pub profile_sha256:String
}
#[derive(Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadManifest{
    pub backup_id:String,pub profile_id:String,pub provider:String,
    pub cell:String,pub volume:String,pub encrypted_bytes:u64,
    pub recipient:String,
    #[serde(default)]pub pull_job_id:Option<String>,
}
fn sha256(s:&str)->String{wire::token_sha(s)}
fn valid_hash(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())}
fn validate_public_key(s:&str)->Res<()>{
    let key=B64.decode(s)?;
    if key.len()!=32{return Err(fail("Device public key must be 32 bytes"));}
    Ok(())
}
pub fn register(db:&Database,form:&Enrollment)->Res<Value>{
    if !wire::valid_id(&form.agent_id)||!wire::valid_user(&form.owner_user){
        return Err(fail("Invalid device/account registration"));
    }
    validate_public_key(&form.device_public_key)?;
    let mut bytes=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token=hex::encode(bytes);
    let mut conn=db.connect()?;
    conn.execute("INSERT INTO mb_agent_registry
        (agent_id,owner_user,device_public_key,token_sha256)
        VALUES ($1,$2,$3,$4)",
        &[&form.agent_id,&form.owner_user,&form.device_public_key,&sha256(&token)])?;
    // The token is presented ONCE over an authenticated admin session. It is
    // never stored in plaintext, returned by status or written into an audit.
    Ok(json!({"agent_id":form.agent_id,"owner_user":form.owner_user,
        "status":"pending_admin_approval","token_once":token,
        "device_key_sha256":sha256(&form.device_public_key)}))
}
pub fn set_approval(db:&Database,id:&str,approved:bool,revoke:bool)->Res<bool>{
    if !wire::valid_id(id){return Err(fail("Invalid registered device ID"));}
    let mut conn=db.connect()?;
    let n=conn.execute("UPDATE mb_agent_registry
        SET approved=$2,revoked=$3
        WHERE agent_id=$1 AND (NOT revoked OR $3)",
        &[&id,&approved,&revoke])?;
    Ok(n==1)
}
pub fn list(db:&Database)->Res<Value>{
    let mut conn=db.connect()?;
    let rows=conn.query("SELECT agent_id,owner_user,approved,revoked,
          last_sequence,last_seen IS NOT NULL AND
          last_seen>now()-interval '120 seconds',
          created_at::text FROM mb_agent_registry ORDER BY agent_id",&[])?;
    Ok(json!({"agents":rows.into_iter().map(|r|json!({
        "id":r.get::<_,String>(0),"user":r.get::<_,String>(1),
        "approved":r.get::<_,bool>(2),"revoked":r.get::<_,bool>(3),
        "last_sequence":r.get::<_,i64>(4),"online":r.get::<_,bool>(5),
        "registered":r.get::<_,String>(6)
    })).collect::<Vec<_>>()}))
}
#[derive(Debug,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeLimits {pub agent_id:String,pub limits:ExecutionLimits}
pub fn update_limits(db:&Database,update:&ChangeLimits)->Res<bool>{
    if !wire::valid_id(&update.agent_id){return Err(fail("Invalid agent ID"));}
    update.limits.validate().map_err(fail)?;
    let limits=serde_json::to_value(&update.limits)?;
    let mut conn=db.connect()?;
    Ok(conn.execute("UPDATE mb_agent_registry SET execution_limits=$2
        WHERE agent_id=$1 AND NOT revoked",&[&update.agent_id,&limits])?==1)
}
pub fn policy(db:&Database,agent:&Verified)->Res<Value>{
    if !record_sequence(db,&agent.proof)?{return Err(fail("Agent policy replay denied"));}
    Ok(json!({"approved":true,"limits":agent.limits}))
}
pub fn enqueue_pull(db:&Database,job:&PullJob)->Res<String>{
    if !wire::valid_id(&job.agent_id)||!wire::valid_id(&job.profile_id)||
       !valid_hash(&job.profile_sha256){return Err(fail("Invalid approved profile hash"));}
    let mut conn=db.connect()?;
    let id=hex::encode(rand::random::<[u8;32]>());
    let n=conn.execute("INSERT INTO mb_agent_pull_jobs
        (job_id,agent_id,profile_id,profile_sha256)
        SELECT $1,agent_id,$3,$4 FROM mb_agent_registry
        WHERE agent_id=$2 AND approved AND NOT revoked",
        &[&id,&job.agent_id,&job.profile_id,&job.profile_sha256])?;
    if n==0{return Err(fail("Agent is unapproved, unknown or revoked"));}
    Ok(id)
}
pub struct Verified {pub agent_id:String,pub owner_user:String,pub proof:AgentProof,
                     pub metadata:Vec<u8>,pub limits:ExecutionLimits}
pub fn authenticate(db:&Database,headers:&HeaderMap,method:&str,path:&str)
    ->Res<Verified>{
    let authorization=headers.get("authorization").and_then(|v|v.to_str().ok())
        .ok_or("Missing agent bearer token")?;
    let token=authorization.strip_prefix("Bearer ").ok_or("Invalid agent authorization type")?;
    if !(43..=256).contains(&token.len())||
       !token.bytes().all(|v|v.is_ascii_alphanumeric()){
        return Err(fail("Malformed agent bearer token"));
    }
    let encoded=headers.get("x-mb-agent-proof").and_then(|v|v.to_str().ok())
        .ok_or("Missing signed agent proof")?;
    if encoded.len()>8192{return Err(fail("Agent proof oversized"));}
    let proof:AgentProof=serde_json::from_slice(&B64.decode(encoded)?)?;
    let extra=headers.get("x-mb-agent-metadata").and_then(|v|v.to_str().ok())
        .ok_or("Missing agent metadata")?;
    if extra.len()>wire::MAX_METADATA_BYTES*2{
        return Err(fail("Agent metadata oversized"));
    }
    let metadata=B64.decode(extra)?;
    if !wire::valid_id(&proof.agent_id)||!wire::valid_user(&proof.user)||
       proof.sequence>i64::MAX as u64{
        return Err(fail("Invalid agent identity or sequence"));
    }
    let mut conn=db.connect()?;
    let row=conn.query_opt("SELECT owner_user,device_public_key,token_sha256,
            last_sequence,approved,revoked,execution_limits
            FROM mb_agent_registry WHERE agent_id=$1",&[&proof.agent_id])?
        .ok_or("Unknown agent")?;
    let owner:String=row.get(0);
    let key:String=row.get(1);
    let token_hash:String=row.get(2);
    let last:i64=row.get(3);
    let approved:bool=row.get(4);
    let revoked:bool=row.get(5);
    let limits:ExecutionLimits=serde_json::from_value(row.get::<_,Value>(6))?;
    limits.validate().map_err(fail)?;
    if !approved||revoked||owner!=proof.user{
        return Err(fail("Agent account not approved or revoked"));
    }
    let hashed=sha256(token);
    if hashed.as_bytes().ct_eq(token_hash.as_bytes()).unwrap_u8()!=1{
        return Err(fail("Agent credential rejected"));
    }
    // Permit bounded out-of-order completion of independently signed
    // parallel uploads; nonce uniqueness is enforced in PostgreSQL.
    // A stale sequence older than the 1024-wide replay window fails closed.
    wire::verify(&key,&proof,method,path,&metadata,
        (last as u64).saturating_sub(1024),wire::now())
        .map_err(|_|fail("Device signature or replay check failed"))?;
    Ok(Verified{agent_id:proof.agent_id.clone(),owner_user:owner,proof,metadata,limits})
}
/// Finish each signed request under an atomic monotonic sequence predicate.
/// Concurrent replays may pass the preflight but at most one can commit.
pub fn record_sequence(db:&Database,proof:&AgentProof)->Res<bool>{
    let mut conn=db.connect()?;
    let mut tx=conn.transaction()?;
    // Token/proof checks have already passed. Prevent any concurrent replay
    // via the unique(agent_id,sequence) key, regardless of arrival order.
    let n=tx.execute("INSERT INTO mb_agent_request_nonces (agent_id,sequence)
         SELECT agent_id,$2 FROM mb_agent_registry
         WHERE agent_id=$1 AND approved AND NOT revoked
           AND last_sequence - 1024 < $2
         ON CONFLICT DO NOTHING",
        &[&proof.agent_id,&(proof.sequence as i64)])?;
    if n!=1 {return Ok(false);}
    let changed=tx.execute("UPDATE mb_agent_registry
         SET last_sequence=GREATEST(last_sequence,$2),last_seen=now()
         WHERE agent_id=$1 AND approved AND NOT revoked",
        &[&proof.agent_id,&(proof.sequence as i64)])?;
    if changed!=1{return Ok(false);}
    tx.commit()?;
    Ok(true)
}
pub fn poll(db:&Database,identity:&Verified)->Res<Value>{
    if !record_sequence(db,&identity.proof)?{return Err(fail("Replayed agent poll"));}
    let mut conn=db.connect()?;
    let rows=conn.query("UPDATE mb_agent_pull_jobs SET
        state='leased',lease_until=now()+interval '15 minutes'
        WHERE job_id IN (
           SELECT job_id FROM mb_agent_pull_jobs
           WHERE agent_id=$1 AND (state='queued' OR
               (state='leased' AND lease_until<now()))
           ORDER BY created_at LIMIT 4 FOR UPDATE SKIP LOCKED
        )
        RETURNING job_id,profile_id,profile_sha256",&[&identity.agent_id])?;
    Ok(json!({"approved":true,"limits":identity.limits,"jobs":rows.into_iter().map(|r|json!({
        "job_id":r.get::<_,String>(0),
        "profile_id":r.get::<_,String>(1),
        "profile_sha256":r.get::<_,String>(2)
    })).collect::<Vec<_>>()}))
}
pub fn validate_upload(meta:&[u8])->Res<UploadManifest>{
    if meta.len()>wire::MAX_METADATA_BYTES{return Err(fail("Upload metadata oversized"));}
    let form:UploadManifest=serde_json::from_slice(meta)?;
    if !valid_hash(&form.backup_id)||!wire::valid_id(&form.profile_id)||
        form.provider!="afs"||!wire::valid_id(&form.cell)||
        !wire::valid_id(&form.volume)||form.encrypted_bytes==0||
        !form.recipient.starts_with("age1")||form.recipient.len()>120||
        form.pull_job_id.as_deref().is_some_and(|v|!valid_hash(v)){
        return Err(fail("Invalid agent backup metadata or identity"));
    }
    Ok(form)
}
pub fn commit_upload(db:&Database,identity:&Verified,form:&UploadManifest,
                     path:&str,digest:&str,bytes:u64)->Res<()>{
    if bytes!=form.encrypted_bytes||digest!=identity.proof.body_sha256{
        return Err(fail("Ciphertext digest or size differs from signed upload"));
    }
    if !record_sequence(db,&identity.proof)? {
        return Err(fail("Agent upload sequence was replayed or revoked"));
    }
    let mut conn=db.connect()?;
    let mut tx=conn.transaction()?;
    if let Some(job_id)=&form.pull_job_id{
        let changed=tx.execute("UPDATE mb_agent_pull_jobs SET state='success',
            lease_until=NULL WHERE job_id=$1 AND agent_id=$2 AND profile_id=$3
            AND state='leased' AND lease_until>now()",
            &[&job_id,&identity.agent_id,&form.profile_id])?;
        if changed!=1{return Err(fail("Pull job lease expired or not approved"));}
    }
    tx.execute("INSERT INTO mb_agent_uploads
        (backup_id,agent_id,owner_user,profile_id,cell,volume,
         recipient,blob_path,encrypted_bytes,ciphertext_sha256)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        &[&form.backup_id,&identity.agent_id,&identity.owner_user,
          &form.profile_id,&form.cell,&form.volume,&form.recipient,
          &path,&(bytes as i64),&digest])?;
    tx.commit()?;
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn malformed_upload_is_rejected(){
        assert!(validate_upload(br#"{}"#).is_err());
        assert!(validate_public_key("broken").is_err());
    }
    #[test]fn unknown_agent_identity_is_not_a_valid_enrollment(){
        assert!(!wire::valid_id("../../tmp"));
        assert!(!wire::valid_user("user name"));
    }
}