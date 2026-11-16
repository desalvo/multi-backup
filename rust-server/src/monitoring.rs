//! Opt-in Rust monitoring: Prometheus 9502, token-only HTTPS REST 8443,
//! and read-only Net-SNMP on an operator-controlled UDP port.
//! Exposes derived application counters, NEVER PostgreSQL network sockets.
use crate::{AppState, cluster::Database};
use axum::{extract::{Json,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response},routing::{get,post},Router};
use axum_server::tls_rustls::RustlsConfig;
use rand::RngCore;
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::HashMap,env,fs,fs::OpenOptions,io::Write,net::SocketAddr,
    path::{Path,PathBuf},process::Stdio,sync::Mutex,time::Instant};
use subtle::ConstantTimeEq;
type E=Box<dyn std::error::Error+Send+Sync>;
type Result<T>=std::result::Result<T,E>;

#[derive(Clone,Debug,Serialize,Deserialize)]
struct TokenRow {id:String,sha256:String,expires_unix:i64,issued_unix:i64}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalStore {version:u32,tokens:HashMap<String,TokenRow>}
#[derive(Clone,Debug)]
pub struct SnmpConfig {
    bind:SocketAddr,versions:Vec<String>,community:String,
    user:Option<String>,auth_file:Option<PathBuf>,priv_file:Option<PathBuf>,
    root:PathBuf,
}
pub struct Monitoring {
    boot:Instant,
    prom:Option<SocketAddr>,
    prom_key:Option<Vec<u8>>,
    rest:Option<SocketAddr>,
    tls_cert:Option<PathBuf>,
    tls_key:Option<PathBuf>,
    snmp:Option<SnmpConfig>,
    store_path:Option<PathBuf>,
    local:Mutex<LocalStore>,
}
fn enabled(key:&str)->Result<bool>{
    match env::var(key).as_deref(){
        Ok("true"|"1")=>Ok(true),
        Ok("false"|"0")|Err(env::VarError::NotPresent)=>Ok(false),
        _=>Err(format!("Invalid {key}: expected true/false").into())
    }
}
fn address(key:&str,default:&str)->Result<SocketAddr>{
    Ok(env::var(key).unwrap_or_else(|_|default.into()).parse()?)
}
fn read_secret(path:&Path)->Result<Vec<u8>>{
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()||meta.len()>2048{return Err("Invalid monitoring credential file".into());}
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode()&0o077!=0 {return Err("Monitoring credentials must be mode 0600".into());}
    }
    let data=fs::read(path)?;
    let data=data.strip_suffix(b"\n").unwrap_or(&data).to_vec();
    if data.len()<16 || data.len()>512 || data.iter().any(|b|!b.is_ascii_graphic()){
        return Err("Monitoring credential requires 16..512 printable characters".into());
    }
    Ok(data)
}
fn token_hash(token:&str)->String{hex::encode(Sha256::digest(token.as_bytes()))}
fn bearer(headers:&HeaderMap)->Option<&str>{
    let value=headers.get("authorization")?.to_str().ok()?;
    let value=value.strip_prefix("Bearer ")?;
    if value.len()==69 && value.starts_with("mbm1_") &&
        value[5..].bytes().all(|b|b.is_ascii_hexdigit()) {Some(value)}else{None}
}
fn write_private(path:&Path,body:&[u8])->Result<()>{
    let parent=path.parent().ok_or("Missing monitoring file parent")?;
    fs::create_dir_all(parent)?;
    let md=fs::symlink_metadata(parent)?;
    if !md.file_type().is_dir(){return Err("Monitoring directory must not be a symlink".into());}
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent,fs::Permissions::from_mode(0o700))?;
    }
    let temp=parent.join(format!(".mb-{}",uuid::Uuid::new_v4().simple()));
    let mut opts=OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]{
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file=opts.open(&temp)?;
    let result=(||->Result<()>{
        file.write_all(body)?;file.sync_all()?;
        // Refuse to replace an attacker-created symlink or foreign file.
        if let Ok(metadata)=fs::symlink_metadata(path){
            if !metadata.file_type().is_file(){return Err("Unsafe monitoring destination".into());}
        }
        fs::rename(&temp,path)?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(temp);}
    result
}
fn save_store(path:&Path,store:&LocalStore)->Result<()>{
    write_private(path,&serde_json::to_vec(store)?)
}
impl Monitoring{
    pub fn from_env()->Result<Self>{
        let prom=if enabled("MULTI_BACKUP_PROMETHEUS_ENABLED")?{
            Some(address("MULTI_BACKUP_PROMETHEUS_BIND","127.0.0.1:9502")?)
        }else{None};
        let rest=if enabled("MULTI_BACKUP_MONITOR_REST_ENABLED")?{
            Some(address("MULTI_BACKUP_MONITOR_REST_BIND","127.0.0.1:8443")?)
        }else{None};
        let prom_key=env::var_os("MULTI_BACKUP_PROMETHEUS_TOKEN_FILE")
            .map(|p|read_secret(Path::new(&p))).transpose()?;
        if prom.is_some_and(|bind|!bind.ip().is_loopback()) && prom_key.is_none(){
            return Err("Prometheus outside localhost requires a dedicated bearer token file".into());
        }
        let (tls_cert,tls_key)=if rest.is_some(){
            let cert=PathBuf::from(env::var_os("MULTI_BACKUP_MONITOR_TLS_CERT_FILE")
                .ok_or("REST monitor HTTPS requires a certificate")?);
            let key=PathBuf::from(env::var_os("MULTI_BACKUP_MONITOR_TLS_KEY_FILE")
                .ok_or("REST monitor HTTPS requires a private key")?);
            let cm=fs::symlink_metadata(&cert)?;
            let km=fs::symlink_metadata(&key)?;
            if !cm.file_type().is_file()||!km.file_type().is_file(){
                return Err("Monitoring HTTPS cert/key must be regular files".into());
            }
            #[cfg(unix)]{
                use std::os::unix::fs::PermissionsExt;
                if km.permissions().mode()&0o077!=0 {
                    return Err("Monitoring HTTPS private key must be mode 0600".into());
                }
            }
            (Some(cert),Some(key))
        }else{(None,None)};
        let snmp=if enabled("MULTI_BACKUP_SNMP_ENABLED")?{
            let bind=address("MULTI_BACKUP_SNMP_BIND","127.0.0.1:9161")?;
            let raw=env::var("MULTI_BACKUP_SNMP_VERSIONS").unwrap_or_else(|_|"v2c".into());
            let mut versions=raw.split(',').map(str::trim).map(str::to_owned).collect::<Vec<_>>();
            versions.sort();versions.dedup();
            if versions.is_empty()||versions.iter().any(|v|!matches!(v.as_str(),"v1"|"v2c"|"v3")){
                return Err("SNMP versions must be v1,v2c,v3".into());
            }
            if !bind.ip().is_loopback() && versions.iter().any(|v|v!="v3"){
                return Err("SNMPv1/v2c are permitted on loopback only; use SNMPv3 authPriv for network access".into());
            }
            let community=env::var("MULTI_BACKUP_SNMP_COMMUNITY").unwrap_or_else(|_|"public".into());
            if community.len()>128||community.is_empty()||
                !community.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)){
                return Err("Unsafe SNMP community".into());
            }
            let v3=versions.iter().any(|v|v=="v3");
            let user=if v3{Some(env::var("MULTI_BACKUP_SNMP_V3_USER")?)}else{None};
            if let Some(value)=&user {
                if value.len()>64||value.len()<1||!value.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b)){
                    return Err("Invalid SNMPv3 user".into());
                }
            }
            let auth_file=if v3{Some(PathBuf::from(env::var("MULTI_BACKUP_SNMP_V3_AUTH_FILE")?))}else{None};
            let priv_file=if v3{Some(PathBuf::from(env::var("MULTI_BACKUP_SNMP_V3_PRIV_FILE")?))}else{None};
            if let (Some(a),Some(p))=(&auth_file,&priv_file){
                if read_secret(a)?==read_secret(p)?{return Err("SNMPv3 auth and privacy secrets must differ".into());}
            }
            let root=PathBuf::from(env::var("MULTI_BACKUP_SNMP_STATE_ROOT")
                .unwrap_or_else(|_|"/data/monitor/snmp".into()));
            if !root.is_absolute(){return Err("SNMP state root must be absolute".into());}
            Some(SnmpConfig{bind,versions,community,user,auth_file,priv_file,root})
        }else{None};
        let store_path=if rest.is_some(){
            let p=PathBuf::from(env::var("MULTI_BACKUP_MONITOR_TOKEN_STORE")
                .unwrap_or_else(|_|"/data/monitor/tokens.json".into()));
            if !p.is_absolute(){return Err("Monitor token store must be absolute".into());}
            Some(p)
        }else{None};
        let mut local=LocalStore{version:1,tokens:HashMap::new()};
        if let Some(path)=&store_path{
            if let Ok(meta)=fs::symlink_metadata(path){
                if !meta.file_type().is_file()||meta.len()>1024*1024 {
                    return Err("Unsafe monitoring token file".into());
                }
                #[cfg(unix)]{
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode()&0o077!=0{return Err("Monitoring token file must be mode 0600".into());}
                }
                local=serde_json::from_slice(&fs::read(path)?)?;
                if local.version!=1{return Err("Unsupported monitoring token-store version".into());}
            }
        }
        Ok(Self{boot:Instant::now(),prom,prom_key,rest,tls_cert,tls_key,snmp,store_path,
            local:Mutex::new(local)})
    }
    pub fn enabled(&self)->bool{self.prom.is_some()||self.rest.is_some()||self.snmp.is_some()}
    pub fn settings(&self)->Value{
        json!({"prometheus":{"enabled":self.prom.is_some(),"bind":self.prom.map(|x|x.to_string())},
            "rest":{"enabled":self.rest.is_some(),"bind":self.rest.map(|x|x.to_string()),"https":true,"auth":"application_issued_bearer"},
            "snmp":{"enabled":self.snmp.is_some(),"bind":self.snmp.as_ref().map(|x|x.bind.to_string()),
                "versions":self.snmp.as_ref().map(|x|x.versions.clone()),
                "community_default":"public","read_only":true},
            "postgres_direct_public_endpoint":false})
    }
    pub fn issue_token(&self,db:Option<&Database>,ttl_days:i64)->Result<Value>{
        if self.rest.is_none(){return Err("Monitoring REST is disabled".into());}
        if !(1..=90).contains(&ttl_days){return Err("Token lifetime must be 1..90 days".into());}
        let mut bytes=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token=format!("mbm1_{}",hex::encode(bytes));
        let id=uuid::Uuid::new_v4().simple().to_string();
        let now=chrono::Utc::now().timestamp();
        let expires=now+ttl_days*86400;
        let row=TokenRow{id:id.clone(),sha256:token_hash(&token),expires_unix:expires,issued_unix:now};
        if let Some(db)=db {
            db.monitor_token_create(&id,&row.sha256,expires)?;
        }else{
            let mut store=self.local.lock().map_err(|_|"Token registry unavailable")?;
            store.tokens.insert(id.clone(),row);
            if let Some(path)=&self.store_path{save_store(path,&store)?;}
        }
        Ok(json!({"id":id,"token_once":token,"expires_unix":expires,"scope":"monitor:read",
            "warning":"This token is returned once; store it securely"}))
    }
    pub fn token_list(&self,db:Option<&Database>)->Result<Value>{
        if let Some(db)=db{return db.monitor_token_list();}
        let store=self.local.lock().map_err(|_|"Token registry unavailable")?;
        Ok(json!({"tokens":store.tokens.values().map(|v|json!({
            "id":v.id,"issued_unix":v.issued_unix,
            "expires_unix":v.expires_unix
        })).collect::<Vec<_>>()}))
    }
    pub fn revoke_token(&self,db:Option<&Database>,id:&str)->Result<bool>{
        if id.len()!=32||!id.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Invalid monitor token ID".into());}
        if let Some(db)=db{return db.monitor_token_revoke(id);}
        let mut store=self.local.lock().map_err(|_|"Token registry unavailable")?;
        let deleted=store.tokens.remove(id).is_some();
        if deleted{if let Some(path)=&self.store_path{save_store(path,&store)?;}}
        Ok(deleted)
    }
    fn token_valid(&self,db:Option<&Database>,token:&str)->bool{
        let hash=token_hash(token);
        if let Some(db)=db{return db.monitor_token_valid(&hash).unwrap_or(false);}
        let Ok(store)=self.local.lock() else{return false};
        let now=chrono::Utc::now().timestamp();
        store.tokens.values().any(|row|row.expires_unix>now &&
            hash.as_bytes().ct_eq(row.sha256.as_bytes()).unwrap_u8()==1)
    }
    fn prom_authorized(&self,headers:&HeaderMap)->bool{
        let Some(secret)=&self.prom_key else{return true};
        let Some(v)=headers.get("authorization").and_then(|x|x.to_str().ok())
            .and_then(|v|v.strip_prefix("Bearer ")) else{return false};
        secret.as_slice().ct_eq(v.as_bytes()).unwrap_u8()==1
    }
}
pub async fn collect(state:&AppState)->Value{
    let elapsed=state.0.monitor.boot.elapsed().as_secs();
    let cluster=state.0.cluster.clone();
    let db_status=if let Some(db)=cluster{
        tokio::task::spawn_blocking(move||{
            let cluster=db.status()?;
            let ha=db.ha_status()?;
            Ok::<_,Box<dyn std::error::Error+Send+Sync>>((cluster,ha))
        }).await.ok().and_then(Result::ok)
    }else{None};
    let (data,ha,available)=if let Some((data,ha))=db_status {
        (data,ha,true)
    }else{(json!({"jobs":[],"replicas":[],"storage":[]}),
        json!({"role":"unavailable","writer_ready":false,"replication_peers":[]}),false)};
    let jobs=data.get("jobs").and_then(Value::as_array).cloned().unwrap_or_default();
    let replicas=data.get("replicas").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut counters=HashMap::<String,u64>::new();
    for job in &jobs{
        if let Some(s)=job.get("status").and_then(Value::as_str){*counters.entry(s.into()).or_default()+=1;}
    }
    json!({
        "version":env!("CARGO_PKG_VERSION"),
        "uptime_seconds":elapsed,
        "mode":if state.0.cluster.is_some(){"cluster"}else{"standalone"},
        "database":{"configured":state.0.cluster.is_some(),"available":available,"ha":ha},
        "jobs":{"sample_total":jobs.len(),"by_status":counters},
        "replicas":{"total":replicas.len(),"healthy":replicas.iter()
            .filter(|x|x.get("healthy").and_then(Value::as_bool)==Some(true)).count()},
        "storage":{"registered":data.get("storage").and_then(Value::as_array).map_or(0,Vec::len),
            "server_data_root_available":state.0.root.is_dir(),
            "s3_worm_configured":state.0.s3_vault_config.is_some(),
            "local_vault_configured":state.0.immutable_vault_root.is_some()},
        "cluster":data
    })
}
fn value_u64(data:&Value,path:&[&str])->u64{
    let mut current=data;
    for key in path{current=current.get(*key).unwrap_or(&Value::Null);}
    current.as_u64().unwrap_or(0)
}
fn metric(name:&str,val:u64,out:&mut String){
    out.push_str(&format!("# TYPE {name} gauge\n{name} {val}\n"));
}
pub fn prometheus(data:&Value)->String{
    let mut text=String::from("# HELP multi_backup_up Multi Backup monitoring endpoint health\n");
    metric("multi_backup_up",1,&mut text);
    metric("multi_backup_uptime_seconds",value_u64(data,&["uptime_seconds"]),&mut text);
    metric("multi_backup_database_available",u64::from(
        data["database"]["available"]==true),&mut text);
    metric("multi_backup_database_writer_ready",u64::from(
        data["database"]["ha"]["writer_ready"]==true),&mut text);
    metric("multi_backup_database_schema_version",value_u64(data,
        &["database","ha","schema_version"]),&mut text);
    metric("multi_backup_replica_count",value_u64(data,&["replicas","total"]),&mut text);
    metric("multi_backup_replica_healthy",value_u64(data,&["replicas","healthy"]),&mut text);
    metric("multi_backup_storage_count",value_u64(data,&["storage","registered"]),&mut text);
    metric("multi_backup_storage_s3_worm_configured",u64::from(
        data["storage"]["s3_worm_configured"]==true),&mut text);
    for status in ["queued","running","success","failed","cancelled","needs_reconciliation"]{
        metric(&format!("multi_backup_jobs_{status}"),
            value_u64(data,&["jobs","by_status",status]),&mut text);
    }
    metric("multi_backup_database_replication_peers",data["database"]["ha"]["replication_peers"]
        .as_array().map_or(0,|a|a.len()) as u64,&mut text);
    metric("multi_backup_database_synchronous_peers",data["database"]["ha"]["replication_peers"]
        .as_array().map_or(0,|a|a.iter().filter(|v|
            matches!(v["sync_state"].as_str(),Some("sync"|"quorum"))).count()) as u64,&mut text);
    text
}
async fn prometheus_handler(State(state):State<AppState>,headers:HeaderMap)->Response{
    if !state.0.monitor.prom_authorized(&headers){return StatusCode::UNAUTHORIZED.into_response();}
    let data=collect(&state).await;
    ([(axum::http::header::CONTENT_TYPE,"text/plain; version=0.0.4; charset=utf-8")],
        prometheus(&data)).into_response()
}
async fn rest_status(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some(token)=bearer(&headers) else{return StatusCode::UNAUTHORIZED.into_response()};
    if !state.0.monitor.token_valid(state.0.cluster.as_ref(),token){
        return StatusCode::UNAUTHORIZED.into_response();
    }
    crate::secure_headers(Json(collect(&state).await))
}
async fn rest_database(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some(token)=bearer(&headers) else{return StatusCode::UNAUTHORIZED.into_response()};
    if !state.0.monitor.token_valid(state.0.cluster.as_ref(),token){
        return StatusCode::UNAUTHORIZED.into_response();
    }
    crate::secure_headers(Json(collect(&state).await["database"].clone()))
}
async fn rest_metrics(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some(token)=bearer(&headers) else{return StatusCode::UNAUTHORIZED.into_response()};
    if !state.0.monitor.token_valid(state.0.cluster.as_ref(),token){
        return StatusCode::UNAUTHORIZED.into_response();
    }
    crate::secure_headers(([(axum::http::header::CONTENT_TYPE,"text/plain; version=0.0.4")],
        prometheus(&collect(&state).await)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequest {ttl_days:Option<i64>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {id:String}
fn managed(state:&AppState,headers:&HeaderMap,user:&str,csrf:&str)->bool{
    user==state.0.name && crate::same_origin(headers,&state.0) &&
        headers.get("x-csrf-token").and_then(|v|v.to_str().ok())==Some(csrf)
}
pub async fn issue(State(state):State<AppState>,headers:HeaderMap,
    Json(request):Json<TokenRequest>)->Response{
    let Some((_,actor))=crate::session(&headers,&state.0).await else{
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !managed(&state,&headers,&actor.user,&actor.csrf){return StatusCode::FORBIDDEN.into_response();}
    match state.0.monitor.issue_token(state.0.cluster.as_ref(),
        request.ttl_days.unwrap_or(30)){
        Ok(value)=>{
            crate::audit_actor(&state.0,"monitor_token_issued",&actor.user);
            crate::secure_headers(Json(value))
        },
        Err(_)=>StatusCode::BAD_REQUEST.into_response()
    }
}
pub async fn list_tokens(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some((_,actor))=crate::session(&headers,&state.0).await else{
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if actor.user!=state.0.name{return StatusCode::FORBIDDEN.into_response();}
    match state.0.monitor.token_list(state.0.cluster.as_ref()){
        Ok(value)=>crate::secure_headers(Json(value)),
        Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
pub async fn revoke(State(state):State<AppState>,headers:HeaderMap,
    Json(request):Json<RevokeRequest>)->Response{
    let Some((_,actor))=crate::session(&headers,&state.0).await else{
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !managed(&state,&headers,&actor.user,&actor.csrf){return StatusCode::FORBIDDEN.into_response();}
    match state.0.monitor.revoke_token(state.0.cluster.as_ref(),&request.id){
        Ok(deleted)=>{
            crate::audit_actor(&state.0,"monitor_token_revoked",&actor.user);
            crate::secure_headers(Json(json!({"revoked":deleted})))
        },
        Err(_)=>StatusCode::BAD_REQUEST.into_response()
    }
}
pub async fn settings(State(state):State<AppState>,headers:HeaderMap)->Response{
    let Some((_,actor))=crate::session(&headers,&state.0).await else{
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if actor.user!=state.0.name{return StatusCode::FORBIDDEN.into_response();}
    crate::secure_headers(Json(state.0.monitor.settings()))
}
fn snmp_conf(snmp:&SnmpConfig,snapshot:&Path)->Result<String>{
    let mut text=format!("agentAddress udp:{}\n",snmp.bind);
    for (version,key) in [("v1","disableSNMPv1"),("v2c","disableSNMPv2c"),("v3","disableSNMPv3")]{
        if !snmp.versions.iter().any(|v|v==version){
            text.push_str(&format!("{key} yes\n"));
        }
    }
    // No registered private enterprise OID is invented. Net-SNMP's standard
    // nsExtendOutput1Line tables carry the read-only Multi Backup summary.
    text.push_str("view mbview included .1.3.6.1.4.1.8072.1.3.2\n");
    if snmp.versions.iter().any(|v|v=="v1"||v=="v2c"){
        text.push_str(&format!("rocommunity {} 127.0.0.1 -V mbview\n",snmp.community));
    }
    if let Some(user)=&snmp.user{
        text.push_str(&format!("rouser {user} priv -V mbview\n"));
    }
    if snapshot.to_string_lossy().bytes().any(|x|!x.is_ascii_alphanumeric()&&
        !b"/._-".contains(&x)) {
        return Err("Unsafe SNMP snapshot path".into());
    }
    text.push_str(&format!("extend multiBackup /bin/cat {}\n",snapshot.display()));
    Ok(text)
}
fn snmp_prepare(snmp:&SnmpConfig)->Result<(PathBuf,PathBuf)>{
    if !snmp.root.is_absolute(){return Err("SNMP state root must be absolute".into());}
    let persist=snmp.root.join("persist");
    fs::create_dir_all(&persist)?;
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&snmp.root,fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&persist,fs::Permissions::from_mode(0o700))?;
    }
    let snapshot=snmp.root.join("status.txt");
    write_private(&snapshot,b"multi_backup_up=0\n")?;
    let conf=snmp.root.join("snmpd.runtime.conf");
    write_private(&conf,snmp_conf(snmp,&snapshot)?.as_bytes())?;
    if let (Some(user),Some(auth),Some(priv_file))=(&snmp.user,&snmp.auth_file,&snmp.priv_file){
        let persistent= persist.join("snmpd.conf");
        if !persistent.exists(){
            let auth=read_secret(auth)?;
            let privacy=read_secret(priv_file)?;
            let auth=std::str::from_utf8(&auth)?;
            let privacy=std::str::from_utf8(&privacy)?;
            if auth.contains('"')||privacy.contains('"')||auth.contains('\\')||privacy.contains('\\'){
                return Err("SNMPv3 credentials cannot contain quotes or backslashes".into());
            }
            let directive=format!("createUser {user} SHA \"{auth}\" AES \"{privacy}\"\n");
            write_private(&persistent,directive.as_bytes())?;
        }
    }
    Ok((conf,snapshot))
}
async fn start_snmp(snmp:SnmpConfig,state:AppState)->Result<()>{
    let (config,snapshot)=snmp_prepare(&snmp)?;
    let status_state=state.clone();
    tokio::spawn(async move{
        let mut interval=tokio::time::interval(std::time::Duration::from_secs(15));
        loop{
            interval.tick().await;
            let data=collect(&status_state).await;
            let line=format!("up=1 uptime={} db={} writer={} queued={} running={} failed={} replicas={}\n",
                value_u64(&data,&["uptime_seconds"]),
                u64::from(data["database"]["available"]==true),
                u64::from(data["database"]["ha"]["writer_ready"]==true),
                value_u64(&data,&["jobs","by_status","queued"]),
                value_u64(&data,&["jobs","by_status","running"]),
                value_u64(&data,&["jobs","by_status","failed"]),
                value_u64(&data,&["replicas","healthy"]));
            if let Err(e)=write_private(&snapshot,line.as_bytes()){
                eprintln!("SNMP summary update failed: {e}");
            }
        }
    });
    let mut cmd=tokio::process::Command::new("snmpd");
    cmd.args(["-f","-C","-c"]).arg(config).arg("-Lo");
    // Never leak PG, vault or S3 credentials into snmpd child environment.
    cmd.env_clear().env("SNMP_PERSISTENT_DIR",snmp.root.join("persist"))
        .env("PATH","/usr/sbin:/usr/bin:/bin")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child=cmd.spawn()?;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    if let Some(status)=child.try_wait()?{
        return Err(format!("Enabled Net-SNMP service exited at startup: {status}").into());
    }
    tokio::spawn(async move {
        if let Err(e)=child.wait().await{
            eprintln!("Net-SNMP agent failed: {e}");
        }else{
            eprintln!("Net-SNMP agent stopped; server monitoring degraded");
        }
    });
    Ok(())
}
pub async fn start(state:AppState)->Result<()>{
    let monitor=&state.0.monitor;
    if let Some(bind)=monitor.prom {
        let listener=tokio::net::TcpListener::bind(bind).await?;
        let app=Router::new().route("/metrics",get(prometheus_handler)).with_state(state.clone());
        tokio::spawn(async move{
            if let Err(e)=axum::serve(listener,app).await{eprintln!("Prometheus monitor stopped: {e}");}
        });
    }
    if let Some(bind)=monitor.rest {
        let cert=monitor.tls_cert.as_ref().ok_or("Monitor TLS certificate missing")?;
        let key=monitor.tls_key.as_ref().ok_or("Monitor TLS key missing")?;
        let tls=RustlsConfig::from_pem_file(cert,key).await?;
        let listener=std::net::TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let app=Router::new().route("/v1/status",get(rest_status))
            .route("/v1/database",get(rest_database))
            .route("/v1/metrics",get(rest_metrics)).with_state(state.clone());
        let server=axum_server::from_tcp_rustls(listener,tls)?;
        tokio::spawn(async move{
            if let Err(e)=server.serve(app.into_make_service()).await{
                eprintln!("TLS monitoring REST stopped: {e}");
            }
        });
    }
    if let Some(snmp)=monitor.snmp.clone(){start_snmp(snmp,state).await?;}
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn bearer_credentials_reject_cookie_and_invalid_token(){
        let mut headers=HeaderMap::new();
        assert!(bearer(&headers).is_none());
        headers.insert("authorization","Bearer foo".parse().unwrap());
        assert!(bearer(&headers).is_none());
        let t=format!("Bearer mbm1_{}","a".repeat(64));
        headers.insert("authorization",t.parse().unwrap());
        assert_eq!(bearer(&headers).unwrap().len(),69);
    }
    #[test]fn snmp_legacy_is_read_only_and_version_restricted(){
        let config=SnmpConfig{bind:"127.0.0.1:9161".parse().unwrap(),
            versions:vec!["v2c".into()],community:"public".into(),
            user:None,auth_file:None,priv_file:None,root:"/data/monitor/snmp".into()};
        let text=snmp_conf(&config,Path::new("/data/monitor/snmp/status.txt")).unwrap();
        assert!(text.contains("disableSNMPv1 yes"));
        assert!(text.contains("disableSNMPv3 yes"));
        assert!(text.contains("rocommunity public 127.0.0.1 -V mbview"));
        assert!(!text.contains("rwcommunity"));
    }
    #[test]fn only_aggregrate_metrics_without_user_or_data_paths(){
        let data=json!({"uptime_seconds":42,"database":{"available":true,
            "ha":{"writer_ready":true,"schema_version":3,"replication_peers":[]}},
            "jobs":{"by_status":{"running":2,"failed":1}},"replicas":{"total":3,"healthy":2},
            "storage":{"registered":1,"s3_worm_configured":true}});
        let lines=prometheus(&data);
        assert!(lines.contains("multi_backup_database_writer_ready 1"));
        assert!(lines.contains("multi_backup_jobs_running 2"));
        assert!(!lines.contains("password"));
        assert!(!lines.contains("storage_id"));
    }
}
