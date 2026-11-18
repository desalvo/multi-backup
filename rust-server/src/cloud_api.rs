//! Optional authenticated cloud drive browser gateway.
//! All provider names and config live on server; no remote secrets in browser.
//! Default READ ONLY; writes require CSRF+admin+offline recovery approval.
//! Delete is separately disabled unless explicitly enabled by operator.
use super::{AppState, audit_actor, same_origin, session, secure_headers, unauthorized};
use axum::{
    extract::{Query, State, DefaultBodyLimit},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json,Value};
use std::path::PathBuf;
use multi_backup_core::cloud_drive;
use base64::Engine;
type ApiError=(StatusCode,Json<Value>);
fn denied(code:StatusCode,message:&'static str)->ApiError{
    (code,Json(json!({"error":message})))
}
#[derive(Deserialize)]
pub struct Selection {
    provider:String,
    #[serde(default)]
    path:String,
}
#[derive(Deserialize)]
pub struct Upload {
    provider:String,
    #[serde(default)]
    directory:String,
    filename:String,
    content_base64:String,
}
#[derive(Deserialize)]
pub struct Deletion {
    provider:String,
    path:String,
    expected_sha256:String,
    confirmation:String,
}
fn config(state:&AppState)->Result<PathBuf,ApiError>{
    state.0.cloud_config.clone()
       .ok_or_else(||denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_browser_not_configured"))
}
async fn can_read(state:&AppState,headers:&HeaderMap)->Result<String,ApiError>{
    let user=session(headers,&state.0).await.ok_or_else(unauthorized)?.1.user;
    if !state.0.security.can_browse(&user,&state.0.name){
        return Err(denied(StatusCode::FORBIDDEN,"cloud_browse_denied"))
    }
    Ok(user)
}
async fn can_write(state:&AppState,headers:&HeaderMap,delete:bool)
    ->Result<String,ApiError>{
    let auth=session(headers,&state.0).await.ok_or_else(unauthorized)?.1;
    if auth.user!=state.0.name ||
       !same_origin(headers,&state.0) ||
       headers.get("x-csrf-token").and_then(|v|v.to_str().ok())!=Some(auth.csrf.as_str()) ||
       !state.0.security.recovery_ok(headers) ||
       !state.0.security.can_restore(&auth.user,&state.0.name) ||
       !state.0.security.hardened() ||
       !state.0.cloud_write_enabled ||
       (delete&&!state.0.cloud_delete_enabled) {
        audit_actor(&state.0,if delete{"cloud_delete_denied"}else{"cloud_upload_denied"},&auth.user);
        return Err(denied(StatusCode::FORBIDDEN,"cloud_write_requires_admin_https_csrf_offline_approval_and_opt_in"));
    }
    Ok(auth.user)
}
pub async fn remotes(State(state):State<AppState>,headers:HeaderMap)
    ->Result<Json<Value>,ApiError>{
    can_read(&state,&headers).await?;
    let cfg=config(&state)?;
    let providers=tokio::task::spawn_blocking(move||cloud_drive::configured(&cfg))
        .await.map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_worker_failed"))?
        .map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_config_invalid"))?;
    Ok(Json(json!({"providers":providers,
        "write_enabled":state.0.cloud_write_enabled && state.0.security.hardened(),
        "delete_enabled":state.0.cloud_delete_enabled && state.0.cloud_write_enabled &&
          state.0.security.hardened(),"mount_is_local_desktop_only":true})))
}
pub async fn list(State(state):State<AppState>,headers:HeaderMap,
                  Query(req):Query<Selection>)->Result<Json<Value>,ApiError>{
    can_read(&state,&headers).await?;
    let cfg=config(&state)?;
    let list=tokio::task::spawn_blocking(move||
            cloud_drive::list(&cfg,&req.provider,&req.path))
        .await.map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_worker_failed"))?
        .map_err(|_|denied(StatusCode::BAD_GATEWAY,"cloud_provider_listing_failed"))?;
    Ok(Json(list))
}
pub async fn download(State(state):State<AppState>,headers:HeaderMap,
              Query(req):Query<Selection>)->Result<Response,ApiError>{
    can_read(&state,&headers).await?;
    let cfg=config(&state)?;
    let bytes=tokio::task::spawn_blocking(move||
            cloud_drive::read(&cfg,&req.provider,&req.path,64*1024*1024))
        .await.map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_worker_failed"))?
        .map_err(|_|denied(StatusCode::BAD_GATEWAY,"cloud_download_failed_or_exceeds_64mib"))?;
    let mut response=(StatusCode::OK,bytes).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"));
    response.headers_mut().insert(header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"cloud-download.bin\""));
    Ok(secure_headers(response))
}
pub async fn upload(State(state):State<AppState>,headers:HeaderMap,
                    Json(req):Json<Upload>)->Result<Json<Value>,ApiError>{
    let actor=can_write(&state,&headers,false).await?;
    if req.content_base64.len()>12*1024*1024 {
        return Err(denied(StatusCode::PAYLOAD_TOO_LARGE,"cloud_upload_exceeds_8mib"))
    }
    let decoded=base64::engine::general_purpose::STANDARD
        .decode(&req.content_base64).map_err(|_|
            denied(StatusCode::BAD_REQUEST,"invalid_upload_encoding"))?;
    if decoded.len()>8*1024*1024 {
        return Err(denied(StatusCode::PAYLOAD_TOO_LARGE,"cloud_upload_exceeds_8mib"))
    }
    let cfg=config(&state)?;
    let result=tokio::task::spawn_blocking(move||
        cloud_drive::write_new(&cfg,&req.provider,&req.directory,&req.filename,&decoded))
        .await.map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_worker_failed"))?
        .map_err(|_|denied(StatusCode::BAD_GATEWAY,"cloud_create_new_or_verification_failed"))?;
    audit_actor(&state.0,"cloud_create_new_uploaded",&actor);
    Ok(Json(result))
}
pub async fn delete(State(state):State<AppState>,headers:HeaderMap,
                    Json(req):Json<Deletion>)->Result<Json<Value>,ApiError>{
    let actor=can_write(&state,&headers,true).await?;
    let cfg=config(&state)?;
    let result=tokio::task::spawn_blocking(move||
        cloud_drive::delete_file(&cfg,&req.provider,&req.path,
                                 &req.expected_sha256,&req.confirmation))
        .await.map_err(|_|denied(StatusCode::SERVICE_UNAVAILABLE,"cloud_worker_failed"))?
        .map_err(|_|denied(StatusCode::BAD_GATEWAY,"cloud_delete_denied_changed_or_failed"))?;
    audit_actor(&state.0,"cloud_exact_file_deleted",&actor);
    Ok(Json(result))
}
pub fn upload_body_limit()->DefaultBodyLimit{
    DefaultBodyLimit::max(12*1024*1024)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn requests_require_explicit_upload_and_delete_data(){
        assert!(serde_json::from_str::<Deletion>(r#"{"provider":"icloud","path":"x"}"#).is_err());
        assert!(serde_json::from_str::<Upload>(r#"{"provider":"icloud","filename":"x"}"#).is_err());
    }
}
