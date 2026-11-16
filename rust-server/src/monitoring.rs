//! Optional independent monitoring listeners. PostgreSQL is never exposed
//! through this module. SNMP is delegated to the read-only Net-SNMP daemon.
use crate::{AppState,session,same_origin};
use axum::{extract::{Json,State},http::{header,HeaderMap,StatusCode},
    response::{IntoResponse,Response},routing::get,Router};
use serde::Deserialize;
use serde_json::{json,Value};
use std::{env,error::Error,fs,net::SocketAddr,path::PathBuf,sync::Arc};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
type ErrBox=Box<dyn Error+Send+Sync>;

fn flag(key:&str)->Result<bool,ErrBox>{
    match env::var(key).as_deref(){
        Ok("true"|"1")=>Ok(true),
        Ok("false"|"0")|Err(env::VarError::NotPresent)=>Ok(false),
        _=>Err(format!("Invalid monitoring boolean {key}").into())
    }
}
fn binding(key:&str,default:u16)->Result<SocketAddr,ErrBox>{
    let socket=env::var(key).unwrap_or_else(|_|format!("127.0.0.1:{default}"))
       .parse::<SocketAddr>()?;
    if socket.ip().is_unspecified()||socket.ip().is_multicast(){
        return Err("Monitoring must bind to a specific trusted IP, not 0.0.0.0".into());
    }
    Ok(socket)
}
fn bearer(headers:&HeaderMap)->Option<&str>{
    headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")
}
fn prometheus_key()->Result<Option<Arc<Zeroizing<Vec<u8>>>>,ErrBox>{
    let Some(path)=env::var_os("MULTI_BACKUP_PROMETHEUS_TOKEN_FILE") else{
        return Ok(None);
    };
    let m=fs::symlink_metadata(&path)?;
    if !m.file_type().is_file()||m.len()>256{return Err("Invalid Prometheus secret".into());}
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if m.permissions().mode()&0o077 !=0{
            return Err("Prometheus token must be mode 0600".into());
        }
    }
    let value=Zeroizing::new(fs::read(path)?);
    if !(32..=256).contains(&value.len())||value.contains(&b'\n')||value.contains(&b'\r'){
        return Err("Prometheus bearer must be >=32 opaque bytes".into());
    }
    Ok(Some(Arc::new(value)))
}
async fn state_snapshot(state:&AppState)->Result<Value,ErrBox>{
    if let Some(db)=state.0.cluster.clone(){
        tokio::task::spawn_blocking(move||->Result<Value,ErrBox>{
            Ok(json!({"cluster":db.status()?,"database":db.db_backup_dashboard()?}))
        }).await?
    }else{
        Ok(json!({"cluster":{"replicas":[{"healthy":true}],"jobs":[],"storage":[]},
           "database":{"enabled":false,"history":[],"database":{"role":"standalone","standbys":[]}}}))
    }
}
fn list<'a>(value:&'a Value)->&'a [Value]{
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn gauges(data:&Value)->Vec<(&'static str,i64)>{
    let cluster=&data["cluster"];let db=&data["database"];
    let nodes=list(&cluster["replicas"]);
    let jobs=list(&cluster["jobs"]);
    let history=list(&db["history"]);
    vec![
        ("multi_backup_up",1),
        ("multi_backup_cluster_nodes_total",nodes.len() as i64),
        ("multi_backup_cluster_nodes_healthy",
         nodes.iter().filter(|r|r["healthy"]==true).count() as i64),
        ("multi_backup_cluster_jobs_queued",
         jobs.iter().filter(|r|r["status"]=="queued").count() as i64),
        ("multi_backup_cluster_jobs_running",
         jobs.iter().filter(|r|r["status"]=="running").count() as i64),
        ("multi_backup_cluster_jobs_failed",
         jobs.iter().filter(|r|r["status"]=="failed").count() as i64),
        ("multi_backup_database_primary",
         i64::from(db["database"]["role"]=="primary")),
        ("multi_backup_database_sync_replicas",list(&db["database"]["standbys"])
           .iter().filter(|r|r["sync_state"]=="sync"||r["sync_state"]=="quorum")
           .count() as i64),
        ("multi_backup_database_backup_enabled",i64::from(db["enabled"]==true)),
        ("multi_backup_database_backup_failed",history.iter()
           .filter(|r|r["status"]=="failed").count() as i64),
        ("multi_backup_database_reconciliation_required",
           i64::from(db["lease_state"]=="needs_reconciliation")),
    ]
}
fn exposition(data:&Value)->String{
    let mut out=String::new();
    for (key,value) in gauges(data){
        out.push_str(&format!("# HELP {key} Multi Backup server and cluster status\n"));
        out.push_str(&format!("# TYPE {key} gauge\n{key} {value}\n"));
    }
    out
}
#[derive(Clone)]
struct PromState{state:AppState,key:Option<Arc<Zeroizing<Vec<u8>>>>}
async fn metrics(State(ctx):State<PromState>,headers:HeaderMap)->Response{
    if let Some(ref key)=ctx.key{
        let authenticated=bearer(&headers).is_some_and(|value|
            value.len()==key.len()&&
            value.as_bytes().ct_eq(key.as_slice()).unwrap_u8()==1);
        if !authenticated {return StatusCode::UNAUTHORIZED.into_response();}
    }
    match state_snapshot(&ctx.state).await {
        Ok(value)=>([(header::CONTENT_TYPE,"text/plain; version=0.0.4; charset=utf-8"),
                    (header::CACHE_CONTROL,"no-store")],exposition(&value)).into_response(),
        Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
async fn verify_api(state:&AppState,headers:&HeaderMap)->Result<(),Response>{
    let Some(token)=bearer(headers) else{return Err(StatusCode::UNAUTHORIZED.into_response())};
    let Some(db)=state.0.cluster.clone() else{return Err(StatusCode::SERVICE_UNAVAILABLE.into_response())};
    let token=token.to_string();
    if matches!(tokio::task::spawn_blocking(move||db.monitor_token_valid(&token)).await,
        Ok(Ok(true))){Ok(())}else{Err(StatusCode::UNAUTHORIZED.into_response())}
}
async fn api_status(State(state):State<AppState>,headers:HeaderMap)->Response{
    if let Err(e)=verify_api(&state,&headers).await{return e;}
    match state_snapshot(&state).await{
        Ok(data)=>([(header::CACHE_CONTROL,"no-store")],
            Json(json!({"schema":1,"read_only":true,"cluster":data["cluster"],
                "database":data["database"]}))).into_response(),
        Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
async fn api_cluster(State(state):State<AppState>,headers:HeaderMap)->Response{
    if let Err(e)=verify_api(&state,&headers).await{return e;}
    match state_snapshot(&state).await{
        Ok(data)=>([(header::CACHE_CONTROL,"no-store")],
            Json(data["cluster"].clone())).into_response(),
        Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
async fn api_db(State(state):State<AppState>,headers:HeaderMap)->Response{
    if let Err(e)=verify_api(&state,&headers).await{return e;}
    match state_snapshot(&state).await{
        Ok(data)=>([(header::CACHE_CONTROL,"no-store")],
            Json(data["database"].clone())).into_response(),
        Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
async fn admin(state:&AppState,headers:&HeaderMap,write:bool)
    ->Result<crate::cluster::Database,Response>{
    let auth=session(headers,&state.0).await
        .ok_or_else(||StatusCode::UNAUTHORIZED.into_response())?.1;
    if auth.user!=state.0.name{return Err(StatusCode::FORBIDDEN.into_response());}
    if write&&(!same_origin(headers,&state.0)||
        headers.get("x-csrf-token").and_then(|v|v.to_str().ok())!=Some(auth.csrf.as_str())||
        !state.0.security.recovery_ok(headers)){
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    state.0.cluster.clone().ok_or_else(||StatusCode::SERVICE_UNAVAILABLE.into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issue{pub name:String,pub lifetime_days:i32}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revoke{pub id:String}
pub async fn issue(State(state):State<AppState>,headers:HeaderMap,Json(req):Json<Issue>)->Response{
    let db=match admin(&state,&headers,true).await{Ok(x)=>x,Err(e)=>return e};
    match tokio::task::spawn_blocking(move||db.create_monitor_token(&req.name,req.lifetime_days)).await{
        Ok(Ok(data))=>(StatusCode::CREATED,[(header::CACHE_CONTROL,"no-store")],Json(data)).into_response(),
        _=>StatusCode::BAD_REQUEST.into_response()
    }
}
pub async fn tokens(State(state):State<AppState>,headers:HeaderMap)->Response{
    let db=match admin(&state,&headers,false).await{Ok(x)=>x,Err(e)=>return e};
    match tokio::task::spawn_blocking(move||db.monitor_token_list()).await{
        Ok(Ok(data))=>([(header::CACHE_CONTROL,"no-store")],Json(data)).into_response(),
        _=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
pub async fn revoke(State(state):State<AppState>,headers:HeaderMap,Json(req):Json<Revoke>)->Response{
    let db=match admin(&state,&headers,true).await{Ok(x)=>x,Err(e)=>return e};
    match tokio::task::spawn_blocking(move||db.revoke_monitor_token(&req.id)).await{
        Ok(Ok(true))=>StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false))=>StatusCode::NOT_FOUND.into_response(),
        _=>StatusCode::BAD_REQUEST.into_response()
    }
}
pub async fn start(state:AppState)->Result<(),ErrBox>{
    if flag("MULTI_BACKUP_PROMETHEUS_ENABLED")?{
        let ip=binding("MULTI_BACKUP_PROMETHEUS_BIND",9502)?;
        let key=prometheus_key()?;
        if !ip.ip().is_loopback()&&key.is_none(){
            return Err("Non-loopback Prometheus requires a bearer token".into());
        }
        let listener=tokio::net::TcpListener::bind(ip).await?;
        let app=Router::new().route("/metrics",get(metrics))
            .with_state(PromState{state:state.clone(),key});
        tokio::spawn(async move{
            if let Err(e)=axum::serve(listener,app).await{
                eprintln!("Prometheus listener terminated: {e}");
            }
        });
    }
    if flag("MULTI_BACKUP_REST_API_ENABLED")?{
        if state.0.cluster.is_none(){
            return Err("Secure REST monitoring requires PostgreSQL token registry".into());
        }
        let ip=binding("MULTI_BACKUP_REST_API_BIND",8443)?;
        let cert=env::var("MULTI_BACKUP_REST_TLS_CERT_FILE")?;
        let key=env::var("MULTI_BACKUP_REST_TLS_KEY_FILE")?;
        for p in [&cert,&key]{
            let md=fs::symlink_metadata(p)?;
            if !md.file_type().is_file()||md.len()>1_048_576{
                return Err("REST TLS PEM must be a regular bounded file".into());
            }
        }
        let tls=axum_server::tls_rustls::RustlsConfig::from_pem_file(cert,key).await?;
        let router=Router::new()
            .route("/v1/status",get(api_status))
            .route("/v1/cluster",get(api_cluster))
            .route("/v1/database",get(api_db))
            .with_state(state);
        tokio::spawn(async move{
            if let Err(e)=axum_server::bind_rustls(ip,tls)
                .serve(router.into_make_service()).await{
                eprintln!("REST HTTPS monitoring listener terminated: {e}");
            }
        });
    }
    Ok(())
}
/// Native read-only output, consumed by an optional Net-SNMP extend entry.
pub fn snmp_status()->Result<(),ErrBox>{
    let db=crate::cluster::Database::configured()?
        .ok_or("SNMP exporter requires configured PostgreSQL cluster")?;
    let data=json!({"cluster":db.status()?,"database":db.db_backup_dashboard()?});
    for (name,value) in gauges(&data){println!("{name}={value}");}
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn prometheus_format_no_source_paths_or_secrets(){
        let v=json!({"cluster":{"replicas":[{"healthy":true}],"jobs":[{"status":"queued"}]},
           "database":{"enabled":false,"history":[],"database":{"role":"primary","standbys":[]}}});
        let output=exposition(&v);
        assert!(output.contains("multi_backup_cluster_jobs_queued 1"));
        assert!(output.contains("multi_backup_database_primary 1"));
        assert!(!output.contains("token"));
    }
}
