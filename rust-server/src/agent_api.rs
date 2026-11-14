//! Rust server endpoints for approved agents, remote encrypted spool
//! reception and cluster status. Agent API cannot be accessed through the
//! public browser session; proxy TLS proof, per-user token and a registered
//! Ed25519 device signature are all mandatory.
use crate::{AppState,session,same_origin,secure_headers};
use crate::{agent_registry as agents,cluster};
use axum::{
    body::Body,extract::{Json,State},http::{HeaderMap,StatusCode},
    response::{IntoResponse,Response}
};
use futures_util::StreamExt;
use multi_backup_core::{
    cluster_placement::{StorageLocation,SourceTransport},
    execution_policy::ExecutionLimits
};
use serde::Deserialize;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,path::PathBuf};
use tokio::io::AsyncWriteExt;

fn response(status:StatusCode,data:Value)->Response{
    secure_headers((status,Json(data)).into_response())
}
fn error(status:StatusCode,message:&str)->Response{
    response(status,json!({"error":message}))
}
fn admin(state:&AppState,headers:&HeaderMap,write:bool)->Result<cluster::Database,Response>{
    let auth=session(headers,&state.0)
        .ok_or_else(||error(StatusCode::UNAUTHORIZED,"authentication_required"))?.1;
    if auth.user!=state.0.name {
        return Err(error(StatusCode::FORBIDDEN,"admin_required"));
    }
    if write && (!same_origin(headers,&state.0)||
        headers.get("x-csrf-token").and_then(|x|x.to_str().ok())!=Some(auth.csrf.as_str())){
        return Err(error(StatusCode::FORBIDDEN,"csrf_rejected"));
    }
    state.0.cluster.clone().ok_or_else(||
        error(StatusCode::SERVICE_UNAVAILABLE,"cluster_registry_not_enabled"))
}
pub async fn list_agents(State(state):State<AppState>,headers:HeaderMap)->Response{
    let db=match admin(&state,&headers,false){Ok(v)=>v,Err(r)=>return r};
    match tokio::task::spawn_blocking(move||agents::list(&db)).await{
        Ok(Ok(data))=>response(StatusCode::OK,data),
        _=>error(StatusCode::SERVICE_UNAVAILABLE,"agent_registry_unavailable")
    }
}
pub async fn enroll(State(state):State<AppState>,headers:HeaderMap,
                    Json(body):Json<agents::Enrollment>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    // Enrollment/approval are high-impact administration actions.
    if !state.0.security.recovery_ok(&headers){
        return error(StatusCode::FORBIDDEN,"device_enrollment_requires_recovery_approval");
    }
    match tokio::task::spawn_blocking(move||agents::register(&db,&body)).await{
        Ok(Ok(data))=>response(StatusCode::CREATED,data),
        _=>error(StatusCode::BAD_REQUEST,"device_enrollment_rejected")
    }
}
pub async fn approve(State(state):State<AppState>,headers:HeaderMap,
                    Json(body):Json<agents::AgentChange>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    if !state.0.security.recovery_ok(&headers){
        return error(StatusCode::FORBIDDEN,"device_approval_requires_recovery_authorization");
    }
    match tokio::task::spawn_blocking(move||
        agents::set_approval(&db,&body.agent_id,true,false)).await{
        Ok(Ok(true))=>response(StatusCode::OK,json!({"approved":true})),
        _=>error(StatusCode::BAD_REQUEST,"device_approval_rejected")
    }
}
pub async fn revoke(State(state):State<AppState>,headers:HeaderMap,
                   Json(body):Json<agents::AgentChange>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    if !state.0.security.recovery_ok(&headers){
        return error(StatusCode::FORBIDDEN,"device_revocation_requires_recovery_authorization");
    }
    match tokio::task::spawn_blocking(move||
        agents::set_approval(&db,&body.agent_id,false,true)).await{
        Ok(Ok(true))=>response(StatusCode::OK,json!({"revoked":true})),
        _=>error(StatusCode::BAD_REQUEST,"device_revocation_rejected")
    }
}
pub async fn enqueue_pull(State(state):State<AppState>,headers:HeaderMap,
                    Json(body):Json<agents::PullJob>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    match tokio::task::spawn_blocking(move||agents::enqueue_pull(&db,&body)).await{
        Ok(Ok(id))=>response(StatusCode::CREATED,json!({"job_id":id,"state":"queued"})),
        _=>error(StatusCode::BAD_REQUEST,"agent_job_rejected")
    }
}
pub async fn poll(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some(db)=state.0.cluster.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE,"agent_registry_not_enabled")
    };
    let proof_headers=headers.clone();
    match tokio::task::spawn_blocking(move||{
        let verified=agents::authenticate(&db,&proof_headers,"GET","/api/agents/poll")?;
        if verified.proof.body_sha256!=multi_backup_core::agent_protocol::hex_sha(b""){
            return Err("Unexpected signed body on agent poll".into());
        }
        agents::poll(&db,&verified)
    }).await{
        Ok(Ok(payload))=>response(StatusCode::OK,payload),
        _=>error(StatusCode::UNAUTHORIZED,"agent_authentication_failed")
    }
}
#[derive(Deserialize)]
pub struct CreateStorage{
    pub storage:StorageLocation,
    /// No automatic inference of shared storage merely from a mount path.
    pub confirmed_shared:bool,
}
pub async fn register_storage(State(state):State<AppState>,headers:HeaderMap,
                      Json(body):Json<CreateStorage>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    if body.storage.visibility==multi_backup_core::cluster_placement::StorageVisibility::Shared &&
       !body.confirmed_shared {
        return error(StatusCode::BAD_REQUEST,"shared_storage_requires_explicit_confirmation");
    }
    match tokio::task::spawn_blocking(move||db.register_storage(&body.storage)).await{
        Ok(Ok(()))=>response(StatusCode::CREATED,json!({"registered":true})),
        _=>error(StatusCode::BAD_REQUEST,"storage_registration_rejected")
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnqueueCluster {
    pub job_id:String,pub plugin_id:String,pub storage_id:String,
    pub source_transport:SourceTransport,
    pub execution_limits:ExecutionLimits,
    pub job_spec:Value,
    #[serde(default)]pub priority:i32,
}
pub async fn enqueue_cluster(State(state):State<AppState>,headers:HeaderMap,
                      Json(req):Json<EnqueueCluster>)->Response{
    let db=match admin(&state,&headers,true){Ok(v)=>v,Err(r)=>return r};
    if multi_backup_core::plugins::find(&req.plugin_id).is_none(){
        return error(StatusCode::BAD_REQUEST,"unregistered_input_plugin");
    }
    match tokio::task::spawn_blocking(move||{
        db.enqueue(&req.job_id,&req.plugin_id,&req.storage_id,
                   req.source_transport,&req.execution_limits,
                   &req.job_spec,req.priority)
    }).await{
        Ok(Ok(()))=>response(StatusCode::CREATED,json!({"state":"queued"})),
        _=>error(StatusCode::BAD_REQUEST,"cluster_job_rejected")
    }
}
pub async fn cluster_status(State(state):State<AppState>,headers:HeaderMap)->Response{
    let db=match admin(&state,&headers,false){Ok(v)=>v,Err(r)=>return r};
    match tokio::task::spawn_blocking(move||db.status()).await{
        Ok(Ok(status))=>response(StatusCode::OK,status),
        _=>error(StatusCode::SERVICE_UNAVAILABLE,"shared_cluster_registry_unavailable")
    }
}
fn spool_directory()->Result<PathBuf,String>{
    let root=std::env::var("MULTI_BACKUP_AGENT_UPLOAD_ROOT")
        .map_err(|_|"Agent encrypted spool is not configured".to_string())?;
    let path=PathBuf::from(root);
    if !path.is_absolute(){return Err("Agent upload root must be absolute".into());}
    fs::create_dir_all(&path).map_err(|e|e.to_string())?;
    let md=fs::symlink_metadata(&path).map_err(|e|e.to_string())?;
    if !md.file_type().is_dir(){return Err("Unsafe agent upload root".into());}
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if md.permissions().mode()&0o077!=0{
            return Err("Agent encrypted spool must have 0700 permissions".into());
        }
    }
    Ok(path)
}
pub async fn upload(State(state):State<AppState>,headers:HeaderMap,
                    body:Body)->Response{
    let Some(db)=state.0.cluster.clone() else {
        return error(StatusCode::SERVICE_UNAVAILABLE,"agent_registry_not_enabled")
    };
    let verify_db=db.clone();
    let verify_headers=headers.clone();
    let verified=match tokio::task::spawn_blocking(move||
        agents::authenticate(&verify_db,&verify_headers,"POST","/api/agents/upload")).await{
        Ok(Ok(approved))=>approved,
        _=>return error(StatusCode::UNAUTHORIZED,"agent_authentication_failed")
    };
    let form=match agents::validate_upload(&verified.metadata){
        Ok(v)=>v,Err(_)=>return error(StatusCode::BAD_REQUEST,"invalid_agent_upload_metadata")
    };
    const MAX_UPLOAD_BYTES:u64=200*1024*1024*1024;
    if form.encrypted_bytes>MAX_UPLOAD_BYTES {
        return error(StatusCode::PAYLOAD_TOO_LARGE,"agent_upload_too_large");
    }
    let folder=match spool_directory(){
        Ok(v)=>v,Err(_)=>return error(StatusCode::SERVICE_UNAVAILABLE,"encrypted_spool_unavailable")
    };
    let temporary=folder.join(format!(".incoming-{}.age",uuid::Uuid::new_v4()));
    let mut output=match tokio::fs::OpenOptions::new().write(true).create_new(true)
        .open(&temporary).await{
        Ok(v)=>v,Err(_)=>return error(StatusCode::SERVICE_UNAVAILABLE,"encrypted_spool_unavailable")
    };
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if tokio::fs::set_permissions(&temporary,std::fs::Permissions::from_mode(0o600))
            .await.is_err(){
            let _=tokio::fs::remove_file(&temporary).await;
            return error(StatusCode::SERVICE_UNAVAILABLE,"secure_spool_permissions_failed");
        }
    }
    let mut stream=body.into_data_stream();
    let mut digest=Sha256::new();
    let mut bytes=0u64;
    let mut prefix=Vec::with_capacity(24);
    let mut failed=false;
    while let Some(chunk)=stream.next().await{
        let Ok(data)=chunk else {failed=true;break};
        bytes=bytes.saturating_add(data.len() as u64);
        if bytes>MAX_UPLOAD_BYTES || bytes>form.encrypted_bytes {failed=true;break;}
        digest.update(&data);
        if prefix.len()<24 {
            let n=(24-prefix.len()).min(data.len());
            prefix.extend_from_slice(&data[..n]);
        }
        if output.write_all(&data).await.is_err(){failed=true;break;}
    }
    if output.flush().await.is_err()||output.sync_all().await.is_err(){failed=true;}
    drop(output);
    if failed||bytes!=form.encrypted_bytes||
       !prefix.starts_with(b"age-encryption.org/v1"){
        let _=tokio::fs::remove_file(&temporary).await;
        return error(StatusCode::BAD_REQUEST,"invalid_or_incomplete_encrypted_upload");
    }
    let hash=hex::encode(digest.finalize());
    if hash!=verified.proof.body_sha256{
        let _=tokio::fs::remove_file(&temporary).await;
        return error(StatusCode::UNAUTHORIZED,"signed_ciphertext_integrity_mismatch");
    }
    let destination=folder.join(format!("{}.age",form.backup_id));
    if destination.exists(){
        let _=tokio::fs::remove_file(&temporary).await;
        return error(StatusCode::CONFLICT,"backup_id_already_present");
    }
    if tokio::fs::rename(&temporary,&destination).await.is_err(){
        let _=tokio::fs::remove_file(&temporary).await;
        return error(StatusCode::SERVICE_UNAVAILABLE,"encrypted_upload_commit_failed");
    }
    let final_path=destination.to_string_lossy().into_owned();
    let upload_id=form.backup_id.clone();
    let record=tokio::task::spawn_blocking(move||
        agents::commit_upload(&db,&verified,&form,&final_path,&hash,bytes)).await;
    match record {
        Ok(Ok(()))=>response(StatusCode::CREATED,
            json!({"accepted":true,"backup_id":upload_id,"encrypted_bytes":bytes})),
        _=>{
            // If DB commit failed the age ciphertext must remain available
            // for an operator to reconcile; never claim completed backup.
            error(StatusCode::SERVICE_UNAVAILABLE,"upload_stored_uncommitted_reconcile_required")
        }
    }
}
