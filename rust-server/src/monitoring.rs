//! Optional independent monitoring listeners. PostgreSQL is never exposed
//! through this module. SNMP is delegated to the read-only Net-SNMP daemon.
use crate::{AppState,session,same_origin,agent_registry};
use axum::{extract::{Json,State},http::{header,HeaderMap,StatusCode},
    response::{IntoResponse,Response},routing::get,Router};
use serde::Deserialize;
use serde_json::{json,Value};
use std::{env,error::Error,fs,net::SocketAddr,path::PathBuf,sync::Arc};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
type ErrBox=Box<dyn Error+Send+Sync>;

fn initialize_tls_crypto_provider()->Result<(),ErrBox>{
    // axum-server and other dependencies may enable both aws-lc-rs and
    // ring features transitively. rustls then panics when it cannot infer
    // one default. Install a deliberate process-wide provider before
    // parsing any HTTPS PEM/serving any monitoring request.
    if rustls::crypto::CryptoProvider::get_default().is_none(){
        let installed=rustls::crypto::aws_lc_rs::default_provider().install_default();
        if installed.is_err() && rustls::crypto::CryptoProvider::get_default().is_none(){
            return Err("Could not install safe rustls cryptographic provider".into());
        }
    }
    Ok(())
}
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
    let allowed=match socket.ip(){
        std::net::IpAddr::V4(ip)=>ip.is_loopback()||ip.is_private()||ip.is_link_local(),
        std::net::IpAddr::V6(ip)=>ip.is_loopback()||ip.is_unique_local()||
            ip.is_unicast_link_local(),
    };
    if !allowed {
        return Err("Monitoring listeners must bind only to loopback or a trusted private address".into());
    }
    Ok(socket)
}
fn checked_tls_pem(cert:&str,key:&str)->Result<(),ErrBox>{
    // Reject symlinked, empty or unexpectedly large PEMs; the private
    // key MUST be accessible only to the daemon owner on Unix.
    for (path,private) in [(cert,false),(key,true)]{
        let md=fs::symlink_metadata(path)?;
        if !md.file_type().is_file()||md.len()==0||md.len()>1_048_576{
            return Err("Monitoring TLS PEM must be a nonempty bounded regular file".into());
        }
        #[cfg(unix)]{
            if private{
                use std::os::unix::fs::PermissionsExt;
                if md.permissions().mode()&0o077 !=0{
                    return Err("Monitoring TLS private key must be mode 0600".into());
                }
            }
        }
        #[cfg(not(unix))]let _=private;
    }
    Ok(())
}
fn remote_prometheus_safe(address:SocketAddr,has_token:bool,has_tls:bool)
    ->Result<(),ErrBox>{
    if !address.ip().is_loopback()&&(!has_token||!has_tls){
        return Err("Remote Prometheus requires BOTH bearer authentication and TLS".into());
    }
    Ok(())
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
fn remove_sensitive_manifests(database:&mut Value){
    // A scoped monitor.read token may inspect health and outcome counts,
    // but must never become a way to list recovery manifests or paths.
    if let Some(history)=database.get_mut("history").and_then(Value::as_array_mut){
        for record in history {
            if let Some(object)=record.as_object_mut(){
                object.remove("manifest");
            }
        }
    }
}
fn standalone_snapshot()->Value{
    json!({"cluster":{"replicas":[{"healthy":true}],"jobs":[],"storage":[]},
        "database":{"enabled":false,"history":[],
            "database":{"role":"standalone","standbys":[]}}})
}
async fn state_snapshot(state:&AppState)->Result<Value,ErrBox>{
    if let Some(db)=state.0.cluster.clone(){
        tokio::task::spawn_blocking(move||->Result<Value,ErrBox>{
            let mut database=db.db_backup_dashboard()?;
            remove_sensitive_manifests(&mut database);
            Ok(json!({"cluster":db.status()?,"database":database}))
        }).await?
    }else{
        Ok(standalone_snapshot())
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
/// Expose counts only: a monitor.read bearer never permits listing the
/// agents' user identities, approved profile hashes or enrollment secrets.
fn safe_agent_counts(raw:&Value)->Value{
    let agents=list(&raw["agents"]);
    json!({
        "total":agents.len(),
        "online":agents.iter().filter(|x|x["online"]==true).count(),
        "approved":agents.iter().filter(|x|x["approved"]==true && x["revoked"]!=true).count(),
        "revoked":agents.iter().filter(|x|x["revoked"]==true).count()
    })
}
async fn api_agents(State(state):State<AppState>,headers:HeaderMap)->Response{
    if let Err(e)=verify_api(&state,&headers).await{return e;}
    let Some(db)=state.0.cluster.clone() else{
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tokio::task::spawn_blocking(move||
        agent_registry::list(&db)).await{
        Ok(Ok(data))=>([(header::CACHE_CONTROL,"no-store")],
            Json(json!({"schema":1,"read_only":true,
                        "agents":safe_agent_counts(&data)}))).into_response(),
        _=>StatusCode::SERVICE_UNAVAILABLE.into_response()
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
    if flag("MULTI_BACKUP_PROMETHEUS_ENABLED")?||
       flag("MULTI_BACKUP_REST_API_ENABLED")?{
        initialize_tls_crypto_provider()?;
    }
    if flag("MULTI_BACKUP_PROMETHEUS_ENABLED")?{
        let ip=binding("MULTI_BACKUP_PROMETHEUS_BIND",9502)?;
        let key=prometheus_key()?;
        let cert=env::var("MULTI_BACKUP_PROMETHEUS_TLS_CERT_FILE").ok();
        let tls_key=env::var("MULTI_BACKUP_PROMETHEUS_TLS_KEY_FILE").ok();
        if cert.is_some()!=tls_key.is_some(){
            return Err("Prometheus TLS certificate and key must be configured together".into());
        }
        remote_prometheus_safe(ip,key.is_some(),cert.is_some())?;
        let app=Router::new().route("/metrics",get(metrics))
            .with_state(PromState{state:state.clone(),key});
        if let (Some(cert),Some(tls_key))=(cert,tls_key){
            checked_tls_pem(&cert,&tls_key)?;
            let tls=axum_server::tls_rustls::RustlsConfig::from_pem_file(cert,tls_key).await?;
            tokio::spawn(async move{
                if let Err(e)=axum_server::bind_rustls(ip,tls)
                    .serve(app.into_make_service()).await{
                    eprintln!("Prometheus HTTPS listener terminated: {e}");
                }
            });
        }else{
            // Plain HTTP is permitted exclusively on loopback, where a
            // local TLS reverse proxy or Prometheus agent can scrape it.
            let listener=tokio::net::TcpListener::bind(ip).await?;
            tokio::spawn(async move{
                if let Err(e)=axum::serve(listener,app).await{
                    eprintln!("Prometheus loopback listener terminated: {e}");
                }
            });
        }
    }
    if flag("MULTI_BACKUP_REST_API_ENABLED")?{
        if state.0.cluster.is_none(){
            return Err("Secure REST monitoring requires PostgreSQL token registry".into());
        }
        let ip=binding("MULTI_BACKUP_REST_API_BIND",8443)?;
        let cert=env::var("MULTI_BACKUP_REST_TLS_CERT_FILE")?;
        let key=env::var("MULTI_BACKUP_REST_TLS_KEY_FILE")?;
        checked_tls_pem(&cert,&key)?;
        let tls=axum_server::tls_rustls::RustlsConfig::from_pem_file(cert,key).await?;
        let router=Router::new()
            .route("/v1/status",get(api_status))
            .route("/v1/cluster",get(api_cluster))
            .route("/v1/database",get(api_db))
            .route("/v1/agents",get(api_agents))
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
    // A monitoring poll must never apply schema migrations. Standalone
    // bare-metal servers still expose their own metrics without PostgreSQL.
    let data=if let Some(db)=crate::cluster::Database::readonly_configured()?{
        let mut database=db.db_backup_dashboard()?;
        remove_sensitive_manifests(&mut database);
        json!({"cluster":db.status()?,"database":database})
    }else{
        standalone_snapshot()
    };
    for (name,value) in gauges(&data){println!("{name}={value}");}
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn multi_feature_rustls_default_is_unambiguously_initialized(){
        initialize_tls_crypto_provider().unwrap();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
    #[test]fn remote_prometheus_refuses_cleartext_or_no_bearer(){
        let private:std::net::SocketAddr = "10.70.0.11:9502".parse().unwrap();
        let localhost:std::net::SocketAddr = "127.0.0.1:9502".parse().unwrap();
        assert!(remote_prometheus_safe(private,false,false).is_err());
        assert!(remote_prometheus_safe(private,true,false).is_err());
        assert!(remote_prometheus_safe(private,false,true).is_err());
        assert!(remote_prometheus_safe(private,true,true).is_ok());
        assert!(remote_prometheus_safe(localhost,false,false).is_ok());
    }
    #[cfg(unix)]
    #[test]fn tls_private_key_must_be_regular_and_owner_only(){
        use std::os::unix::fs::PermissionsExt;
        let root=std::env::temp_dir().join(format!("mb-monitor-tls-{}",uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let cert=root.join("cert.pem");
        let key=root.join("key.pem");
        fs::write(&cert,b"public certificate placeholder").unwrap();
        fs::write(&key,b"test private key placeholder").unwrap();
        fs::set_permissions(&key,fs::Permissions::from_mode(0o644)).unwrap();
        assert!(checked_tls_pem(cert.to_str().unwrap(),key.to_str().unwrap()).is_err());
        fs::set_permissions(&key,fs::Permissions::from_mode(0o600)).unwrap();
        assert!(checked_tls_pem(cert.to_str().unwrap(),key.to_str().unwrap()).is_ok());
        let link=root.join("symlink.key");
        std::os::unix::fs::symlink(&key,&link).unwrap();
        assert!(checked_tls_pem(cert.to_str().unwrap(),link.to_str().unwrap()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]fn standalone_snmp_exposes_only_safe_metrics_without_database(){
        let snapshot=standalone_snapshot();
        assert_eq!(snapshot["database"]["database"]["role"],"standalone");
        let output=exposition(&snapshot);
        assert!(output.contains("multi_backup_cluster_nodes_total 1"));
        assert!(output.contains("multi_backup_database_primary 0"));
        assert!(!output.contains("manifest"));
    }
    #[test]fn readonly_monitor_cannot_list_database_recovery_manifests(){
        let mut db=json!({"history":[{"job_id":"opaque","status":"success",
            "manifest":{"private_path":"/secret/restore/archive","id":"safe-id"}}]});
        remove_sensitive_manifests(&mut db);
        assert_eq!(db["history"][0]["job_id"],"opaque");
        assert!(db["history"][0].get("manifest").is_none());
        assert!(!db.to_string().contains("/secret/"));
    }
    #[test]fn monitor_read_agents_never_leaks_account_or_profile_identity(){
        let raw=json!({"agents":[
            {"id":"first","user":"secret-user@example.test","online":true,
             "approved":true,"revoked":false,"profile_sha256":"secret"},
            {"id":"second","user":"other-secret","online":false,
             "approved":false,"revoked":true}]});
        let safe=safe_agent_counts(&raw);
        assert_eq!(safe["total"],2);
        assert_eq!(safe["online"],1);
        assert_eq!(safe["approved"],1);
        assert_eq!(safe["revoked"],1);
        assert!(!safe.to_string().contains("secret"));
        assert!(!safe.to_string().contains("first"));
    }
    #[test]fn prometheus_format_no_source_paths_or_secrets(){
        let v=json!({"cluster":{"replicas":[{"healthy":true}],"jobs":[{"status":"queued"}]},
           "database":{"enabled":false,"history":[],"database":{"role":"primary","standbys":[]}}});
        let output=exposition(&v);
        assert!(output.contains("multi_backup_cluster_jobs_queued 1"));
        assert!(output.contains("multi_backup_database_primary 1"));
        assert!(!output.contains("token"));
    }
}
