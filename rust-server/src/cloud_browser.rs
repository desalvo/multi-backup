//! Opt-in authenticated cloud filesystem browsing via a strictly scoped rclone CLI.
//! rclone RC/config endpoints are NEVER exposed. Each remote is fixed by admin
//! config, and every operation stays below one pre-approved cloud prefix.
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, env, fs, path::{Path, PathBuf}, time::Duration};
use tokio::{process::Command, time::timeout};
use crate::{AppState, session, same_origin, secure_headers};

const MAX_UPLOAD: usize = 8*1024*1024;
const MAX_OUTPUT: usize = 10*1024*1024;
type ApiResult<T> = Result<T, Response>;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    id: String,
    provider: String,
    remote: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    allow_upload: bool,
    #[serde(default)]
    allow_delete: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    rclone_config: PathBuf,
    profiles: Vec<Profile>,
}
#[derive(Serialize)]
struct PublicProfile<'a> {
    id: &'a str,
    provider: &'a str,
    read: bool,
    upload: bool,
    delete: bool,
    mount_supported: bool,
    mount_read_only: bool,
}
#[derive(Deserialize)]
pub struct BrowseQuery { pub profile: String, #[serde(default)] pub path: String }
#[derive(Deserialize)]
pub struct FileQuery { pub profile: String, pub path: String }
#[derive(Deserialize)]
pub struct DeleteRequest {
    pub profile: String, pub path: String, pub confirm: String,
}

fn response(code: StatusCode, value: Value) -> Response {
    secure_headers((code, Json(value)).into_response())
}
fn deny(code: StatusCode, reason: &str) -> Response {
    response(code, json!({"error":reason}))
}
fn validate_path(path: &str, may_be_empty: bool) -> Result<(), String> {
    if path.is_empty() && may_be_empty { return Ok(()); }
    if path.is_empty() || path.len()>1024 || path.starts_with('/') ||
       path.contains('\\') || path.contains(':') ||
       path.chars().any(char::is_control) {
        return Err("invalid_relative_path".into());
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len()>40 || segments.iter().any(|x|
        x.is_empty() || *x=="." || *x==".." || x.starts_with('~')) {
        return Err("invalid_relative_path".into());
    }
    Ok(())
}
fn allowed_provider(s:&str)->bool {
    matches!(s,"dropbox"|"google-drive"|"onedrive"|"owncloud"|"nextcloud"|
        "icloud"|"s3"|"webdav"|"other")
}
fn valid_remote_name(name:&str)->bool {
    !name.is_empty() && name.len()<=64 &&
    name.bytes().all(|c|c.is_ascii_alphanumeric() || c==b'_' || c==b'-')
}
fn profile_remote(p:&Profile, path:&str) -> Result<String,String> {
    validate_path(path,true)?;
    let base=if p.prefix.is_empty() {String::new()} else {p.prefix.clone()};
    let joined=if base.is_empty(){path.to_string()} else if path.is_empty(){base}
               else {format!("{base}/{path}")};
    Ok(format!("{}:{}",p.remote,joined))
}
fn load_registry()->Result<Option<Registry>,String> {
    let Some(path)=env::var_os("MULTI_BACKUP_CLOUD_PROFILES_FILE") else {
        return Ok(None);
    };
    let path=PathBuf::from(path);
    let meta=fs::symlink_metadata(&path).map_err(|_|"missing_cloud_registry")?;
    if !meta.file_type().is_file() || meta.len()>64*1024 {
        return Err("invalid_cloud_registry_file".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode()&0o077!=0 {
            return Err("cloud_registry_must_be_private_0600".into());
        }
    }
    let data=fs::read(&path).map_err(|_|"cloud_registry_unreadable")?;
    let registry:Registry=serde_json::from_slice(&data)
        .map_err(|_|"invalid_cloud_registry_json")?;
    let cfg_meta=fs::symlink_metadata(&registry.rclone_config)
        .map_err(|_|"rclone_config_missing")?;
    if !cfg_meta.file_type().is_file() {
        return Err("rclone_config_must_be_regular_file".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if cfg_meta.permissions().mode()&0o077!=0 {
            return Err("rclone_config_must_be_private_0600".into());
        }
    }
    let mut seen=HashSet::new();
    if registry.profiles.len()>32 {return Err("too_many_cloud_profiles".into());}
    for p in &registry.profiles {
        if !valid_remote_name(&p.id) || !valid_remote_name(&p.remote) ||
           !allowed_provider(&p.provider) || !seen.insert(&p.id) ||
           validate_path(&p.prefix,true).is_err() {
            return Err("invalid_or_duplicate_cloud_profile".into());
        }
        // rclone iCloud Drive cannot upload/move/delete remotely. On an
        // authorized Mac, its local File Provider helper is a separate path.
        if p.provider=="icloud" && (p.allow_upload || p.allow_delete) {
            return Err("icloud_remote_must_remain_read_only".into());
        }
    }
    Ok(Some(registry))
}
pub fn validate_config()->Result<(),Box<dyn std::error::Error+Send+Sync>> {
    load_registry().map_err(|e|e.into()).map(|_|())
}
fn pick<'a>(reg:&'a Registry, id:&str)->ApiResult<&'a Profile> {
    reg.profiles.iter().find(|p|p.id==id)
        .ok_or_else(||deny(StatusCode::NOT_FOUND,"cloud_profile_not_found"))
}
async fn administrator(state:&AppState, headers:&HeaderMap,write:bool)->ApiResult<()> {
    let actor=session(headers,&state.0).await
        .ok_or_else(||deny(StatusCode::UNAUTHORIZED,"authentication_required"))?.1;
    if actor.user!=state.0.name {
        return Err(deny(StatusCode::FORBIDDEN,"admin_required"));
    }
    if write && (!same_origin(headers,&state.0) ||
        headers.get("x-csrf-token").and_then(|x|x.to_str().ok())!=Some(actor.csrf.as_str()) ||
        !state.0.security.recovery_ok(headers)) {
        return Err(deny(StatusCode::FORBIDDEN,"cloud_write_requires_csrf_and_approval"));
    }
    Ok(())
}
fn registry()->ApiResult<Registry> {
    load_registry().map_err(|_|deny(StatusCode::SERVICE_UNAVAILABLE,"cloud_registry_invalid"))?
        .ok_or_else(||deny(StatusCode::SERVICE_UNAVAILABLE,"cloud_profiles_not_configured"))
}
async fn run_rclone(reg:&Registry, args:&[&str])->Result<Vec<u8>,String> {
    // Explicit binary path, no shell interpolation, no RC listener, no config
    // management command or user-supplied remote.
    let bin=env::var("MULTI_BACKUP_RCLONE_BIN").unwrap_or_else(|_|"/usr/bin/rclone".into());
    if !Path::new(&bin).is_absolute() {
        return Err("rclone_executable_must_be_absolute".into());
    }
    let mut cmd=Command::new(bin);
    cmd.arg("--config").arg(&reg.rclone_config).arg("--ask-password=false");
    cmd.args(args);
    cmd.kill_on_drop(true);
    let out=timeout(Duration::from_secs(65),cmd.output()).await
        .map_err(|_|"cloud_timeout".to_string())
        .and_then(|r|r.map_err(|_|"cloud_command_unavailable".to_string()))?;
    if !out.status.success() {return Err("cloud_provider_operation_failed".into());}
    if out.stdout.len()>MAX_OUTPUT {return Err("cloud_response_limit_exceeded".into());}
    Ok(out.stdout)
}
pub async fn profiles(State(state):State<AppState>,headers:HeaderMap)->Response {
    if let Err(r)=administrator(&state,&headers,false).await {return r;}
    let reg=match registry(){Ok(r)=>r,Err(e)=>return e};
    let profiles:Vec<_>=reg.profiles.iter().map(|p|PublicProfile{
        id:&p.id, provider:&p.provider,read:true,
        upload:p.allow_upload,delete:p.allow_delete,
        mount_supported:true,mount_read_only:!p.allow_upload || p.provider=="icloud",
    }).collect();
    response(StatusCode::OK,json!({"profiles":profiles,"mode":"server_rclone_scoped","remote_credentials_visible":false}))
}
pub async fn list(State(state):State<AppState>,headers:HeaderMap,Query(query):Query<BrowseQuery>)->Response {
    if let Err(e)=administrator(&state,&headers,false).await{return e;}
    let reg=match registry(){Ok(r)=>r,Err(e)=>return e};
    let p=match pick(&reg,&query.profile){Ok(p)=>p,Err(e)=>return e};
    let remote=match profile_remote(p,&query.path){
        Ok(r)=>r,Err(_)=>return deny(StatusCode::BAD_REQUEST,"invalid_path")
    };
    let raw=match run_rclone(&reg,&["lsjson","--max-depth","1","--",remote.as_str()]).await{
        Ok(v)=>v,Err(_)=>return deny(StatusCode::BAD_GATEWAY,"cloud_list_failed")
    };
    let files:Value=match serde_json::from_slice(&raw){
        Ok(v)=>v,Err(_)=>return deny(StatusCode::BAD_GATEWAY,"invalid_provider_listing")
    };
    // Strip provider-specific metadata, remote IDs and potential credentials.
    let Some(items)=files.as_array() else {return deny(StatusCode::BAD_GATEWAY,"invalid_cloud_listing");};
    if items.len()>2000 {return deny(StatusCode::PAYLOAD_TOO_LARGE,"cloud_directory_too_large");}
    let clean:Vec<_>=items.iter().filter_map(|v|{
        let name=v.get("Name")?.as_str()?;
        if validate_path(name,false).is_err() || name.contains('/') {return None;}
        Some(json!({"name":name,"directory":v.get("IsDir").and_then(Value::as_bool).unwrap_or(false),
            "size":v.get("Size").and_then(Value::as_u64).unwrap_or(0)}))
    }).collect();
    response(StatusCode::OK,json!({"profile":p.id,"path":query.path,"items":clean}))
}
pub async fn download(State(state):State<AppState>,headers:HeaderMap,Query(query):Query<FileQuery>)->Response {
    if let Err(e)=administrator(&state,&headers,false).await{return e;}
    if validate_path(&query.path,false).is_err() {
        return deny(StatusCode::BAD_REQUEST,"invalid_path");
    }
    let reg=match registry(){Ok(r)=>r,Err(e)=>return e};
    let p=match pick(&reg,&query.profile){Ok(p)=>p,Err(e)=>return e};
    let remote=match profile_remote(p,&query.path){Ok(s)=>s,Err(_)=>return deny(StatusCode::BAD_REQUEST,"invalid_path")};
    // rclone cat --head ensures even a changed provider object cannot
    // exhaust server RAM. This PREVIEW endpoint serves files <=8 MiB.
    let bytes=match run_rclone(&reg,&["cat","--head","8388609","--",remote.as_str()]).await{
        Ok(v)=>v,Err(_)=>return deny(StatusCode::BAD_GATEWAY,"cloud_download_failed")
    };
    if bytes.len()>MAX_UPLOAD {return deny(StatusCode::PAYLOAD_TOO_LARGE,"cloud_download_limit_8mib");}
    let name=query.path.rsplit('/').next().unwrap_or("cloud.bin");
    let disposition=format!("attachment; filename=\"{}\"",name.replace('"',"_"));
    secure_headers((
        [(axum::http::header::CONTENT_TYPE,"application/octet-stream"),
         (axum::http::header::CACHE_CONTROL,"no-store"),
         (axum::http::header::CONTENT_DISPOSITION,disposition.as_str())],
        bytes
    ).into_response())
}
pub async fn upload(State(state):State<AppState>,headers:HeaderMap,Query(query):Query<FileQuery>,body:Bytes)->Response {
    if let Err(e)=administrator(&state,&headers,true).await{return e;}
    if body.is_empty() || body.len()>MAX_UPLOAD || validate_path(&query.path,false).is_err(){
        return deny(StatusCode::BAD_REQUEST,"cloud_upload_requires_new_file_under_8mib");
    }
    let reg=match registry(){Ok(r)=>r,Err(e)=>return e};
    let p=match pick(&reg,&query.profile){Ok(p)=>p,Err(e)=>return e};
    if !p.allow_upload {return deny(StatusCode::FORBIDDEN,"cloud_upload_disabled");}
    let remote=match profile_remote(p,&query.path){Ok(s)=>s,Err(_)=>return deny(StatusCode::BAD_REQUEST,"invalid_path")};
    // No overwrite by design. This preliminary lsjson and rclone --immutable
    // prevent ordinary overwrites but DO NOT provide atomic provider-side
    // creation under concurrency: the operation stays PREVIEW.
    if run_rclone(&reg,&["lsjson","--",remote.as_str()]).await.is_ok(){
        return deny(StatusCode::CONFLICT,"cloud_destination_exists");
    }
    let file=match tempfile::NamedTempFile::new(){
        Ok(f)=>f,Err(_)=>return deny(StatusCode::SERVICE_UNAVAILABLE,"cloud_stage_unavailable")
    };
    let path=file.path().to_path_buf();
    if fs::write(&path,&body).is_err(){return deny(StatusCode::SERVICE_UNAVAILABLE,"cloud_stage_write_failed");}
    let local=path.to_string_lossy().to_string();
    let result=run_rclone(&reg,&["copyto","--immutable","--",local.as_str(),remote.as_str()]).await;
    if result.is_err(){return deny(StatusCode::BAD_GATEWAY,"cloud_upload_failed");}
    response(StatusCode::CREATED,json!({"uploaded":true,"remote_atomic_create":false,
        "note":"Preview: provider-side concurrent create-only guarantee not certified"}))
}
pub async fn delete(State(state):State<AppState>,headers:HeaderMap,Json(req):Json<DeleteRequest>)->Response {
    if let Err(e)=administrator(&state,&headers,true).await{return e;}
    if validate_path(&req.path,false).is_err(){
        return deny(StatusCode::BAD_REQUEST,"invalid_path");
    }
    let expected=format!("DELETE:{}:{}",req.profile,req.path);
    if req.confirm!=expected {
        return deny(StatusCode::FORBIDDEN,"exact_cloud_delete_confirmation_required");
    }
    let reg=match registry(){Ok(r)=>r,Err(e)=>return e};
    let p=match pick(&reg,&req.profile){Ok(p)=>p,Err(e)=>return e};
    if !p.allow_delete{return deny(StatusCode::FORBIDDEN,"cloud_delete_disabled");}
    // Protect Multi Backup encrypted archives against deletion via cloud
    // browsing. Retention/vault services manage those objects independently.
    if req.path.ends_with(".age") || req.path.ends_with(".age.sha256") {
        return deny(StatusCode::FORBIDDEN,"encrypted_backup_deletion_requires_retention_workflow");
    }
    let remote=match profile_remote(p,&req.path){Ok(s)=>s,Err(_)=>return deny(StatusCode::BAD_REQUEST,"invalid_path")};
    match run_rclone(&reg,&["deletefile","--",remote.as_str()]).await{
        Ok(_)=>response(StatusCode::OK,json!({"deleted":true,"path":req.path})),
        Err(_)=>deny(StatusCode::BAD_GATEWAY,"cloud_delete_failed"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile()->Profile { Profile{id:"cloud".into(),provider:"dropbox".into(),
        remote:"db1".into(),prefix:"team/backups".into(),
        allow_upload:false,allow_delete:false} }
    #[test] fn no_traversal_remote_injection_or_empty_file() {
        for bad in ["../x","a/../b","a//b","/etc/passwd","a\\b","a:b","a/\nX","a/./b","~root"]{
            assert!(validate_path(bad,false).is_err(),"{bad}");
        }
        assert!(validate_path("",false).is_err());
        assert!(validate_path("",true).is_ok());
        let p=profile();
        assert_eq!(profile_remote(&p,"a/b").unwrap(),"db1:team/backups/a/b");
        assert!(profile_remote(&p,"../escape").is_err());
        assert_eq!(profile_remote(&p,"").unwrap(),"db1:team/backups");
    }
    #[test] fn provider_whitelist_and_icloud_read_only_policy(){
        assert!(allowed_provider("nextcloud"));
        assert!(allowed_provider("owncloud"));
        assert!(allowed_provider("icloud"));
        assert!(!valid_remote_name(":local:"));
        assert!(!valid_remote_name("--config=/tmp/evil"));
        assert!(!allowed_provider("local"));
        let x=PublicProfile{id:"i",provider:"icloud",read:true,
            upload:false,delete:false,mount_supported:true,mount_read_only:true};
        assert!(x.mount_read_only && !x.delete);
    }
}
