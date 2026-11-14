//! Authenticated Rust web server for the compiled backup core.
//! Fail closed: local Argon2id login is operational; LDAP/OIDC MUST NOT be
//! advertised or enabled until implemented and externally verified.
mod smtp;
mod ldap;
mod oidc;
mod dav;
use dav::DavConfig;
use smtp::SmtpConfig;
use ldap::LdapConfig;
use oidc::{OidcConfig,Pending};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString};
use axum::{extract::{Form, Json, State}, http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post}, Router};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, fs, fs::OpenOptions, io::Write,
    net::SocketAddr, path::PathBuf, sync::{Arc, Mutex},
    time::{Duration, Instant}};
use zeroize::Zeroize;

#[derive(Clone)]
struct AppState(Arc<AppConfig>);
struct AppConfig {
    name: String,
    password_hash: String,
    origin: String,
    root: PathBuf,
    identity: Option<PathBuf>,
    exports: PathBuf,
    audit: PathBuf,
    smtp: Option<SmtpConfig>,
    dav: Option<DavConfig>,
    ldap: Option<LdapConfig>,
    oidc: Option<OidcConfig>,
    pending_oidc: Mutex<HashMap<String,Pending>>,
    sessions: Mutex<HashMap<String, Session>>,
    failed_logins: Mutex<HashMap<String, (Instant,u32)>>,
    last_smtp_test: Mutex<Option<Instant>>,
}
#[derive(Clone)]
struct Session { csrf: String, until: Instant, user: String }
#[derive(Deserialize)]
struct Login { username: String, password: String }
impl Drop for Login { fn drop(&mut self) { self.password.zeroize(); } }
#[derive(Deserialize)]
struct Export { backup_id: String, selection: Option<String>, selections: Option<Vec<String>> }

fn random_token() -> String {
    let mut bytes=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut bytes);hex::encode(bytes)
}
fn audit_actor(state:&AppConfig,event:&str,actor:&str) {
    if let Some(parent)=state.audit.parent(){let _=fs::create_dir_all(parent);}
    // Refuse nonregular/symlinked audit files; never append through a symlink.
    if let Ok(meta)=fs::symlink_metadata(&state.audit) {
        if !meta.file_type().is_file(){return;}
    }
    if let Ok(mut log)=OpenOptions::new().create(true).append(true).open(&state.audit) {
        let line=json!({"time":chrono::Utc::now().to_rfc3339(),
            "event":event,"user":actor});
        let _=writeln!(log,"{}",line);
        let _=log.sync_data();
    }
}
fn audit(state:&AppConfig,event:&str){audit_actor(state,event,"system");}
fn oidc_cookie_state(headers:&HeaderMap)->Option<String>{
    let cookie=headers.get(header::COOKIE)?.to_str().ok()?;
    let state=cookie.split(';').find_map(|item|
        item.trim().strip_prefix("mb_oidc_state="))?;
    if (16..=512).contains(&state.len()) &&
       state.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){
        Some(state.to_string())
    }else{None}
}

fn session(headers:&HeaderMap,state:&AppConfig)->Option<(String,Session)> {
    let cookie=headers.get(header::COOKIE)?.to_str().ok()?;
    let token=cookie.split(';').find_map(|part|
        part.trim().strip_prefix("mb_session="))?;
    if token.len()!=64 || !token.bytes().all(|v|v.is_ascii_hexdigit()) {
        return None;
    }
    let mut sessions=state.sessions.lock().ok()?;
    sessions.retain(|_,session|session.until>Instant::now());
    sessions.get(token).cloned().map(|s|(token.to_string(),s))
}
fn same_origin(headers:&HeaderMap,state:&AppConfig)->bool {
    headers.get(header::ORIGIN).and_then(|x|x.to_str().ok())==Some(state.origin.as_str())
}
fn secure_headers<R:IntoResponse>(response:R)->Response {
    let mut result=response.into_response();
    let h=result.headers_mut();
    h.insert(header::CACHE_CONTROL,"no-store".parse().unwrap());
    h.insert("x-frame-options","DENY".parse().unwrap());
    h.insert("x-content-type-options","nosniff".parse().unwrap());
    h.insert(header::CONTENT_SECURITY_POLICY,
       "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'".parse().unwrap());
    result
}
async fn health()->impl IntoResponse {(StatusCode::OK,"ok")}
async fn css()->Response {
    ([(header::CONTENT_TYPE,"text/css; charset=utf-8")],
        include_str!("../public/app.css")).into_response()
}
async fn js()->Response {
    ([(header::CONTENT_TYPE,"application/javascript; charset=utf-8")],
        include_str!("../public/app.js")).into_response()
}
async fn login_page(State(state):State<AppState>)->Response {
    let link=if state.0.oidc.is_some(){
        r#"<p><a href="/oidc/start">Accedi con OpenID Connect / SSO</a></p>"#
    }else{""};
    secure_headers(Html(include_str!("../public/login.html")
        .replace("<!--MULTI_BACKUP_SSO-->",link)))
}
async fn do_login(State(state):State<AppState>,headers:HeaderMap,
                  Form(login):Form<Login>)->Response{
    if !same_origin(&headers,&state.0){return secure_headers(StatusCode::FORBIDDEN);}
    if login.username.is_empty()||login.username.len()>128||login.password.len()>1024 {
        return secure_headers(StatusCode::BAD_REQUEST);
    }
    {
        let Ok(mut failed)=state.0.failed_logins.lock() else {
            return secure_headers(StatusCode::INTERNAL_SERVER_ERROR);
        };
        failed.retain(|_,(then,_)|then.elapsed()<Duration::from_secs(900));
        if failed.get(&login.username).is_some_and(|(_,count)|*count>=5) {
            return secure_headers(StatusCode::TOO_MANY_REQUESTS);
        }
        if failed.len()>1024 {return secure_headers(StatusCode::TOO_MANY_REQUESTS);}
    }
    let local=PasswordHash::new(&state.0.password_hash).ok()
        .map(|hash|Argon2::default().verify_password(
            login.password.as_bytes(),&hash).is_ok()).unwrap_or(false)
        &&login.username==state.0.name;
    // The local admin name is always reserved; LDAP can never impersonate it.
    let ldap_valid=if !local && login.username!=state.0.name {
        if let Some(cfg)=state.0.ldap.clone(){
            let user=login.username.clone();
            let password=zeroize::Zeroizing::new(login.password.clone());
            tokio::task::spawn_blocking(move||cfg.authenticate(&user,&password))
                .await.ok().and_then(Result::ok).unwrap_or(false)
        }else{false}
    }else{false};
    if !local&&!ldap_valid {
        if let Ok(mut failed)=state.0.failed_logins.lock() {
            let item=failed.entry(login.username.clone()).or_insert((Instant::now(),0));
            item.1=item.1.saturating_add(1);
        }
        audit_actor(&state.0,"login_failed",&login.username);
        return secure_headers((StatusCode::UNAUTHORIZED,Html("Invalid credentials")));
    }
    if let Ok(mut failures)=state.0.failed_logins.lock(){
        failures.remove(&login.username);
    }
    let token=random_token();
    let csrf=random_token();
    let mut sessions=match state.0.sessions.lock(){
        Ok(value)=>value,Err(_)=>return secure_headers(StatusCode::INTERNAL_SERVER_ERROR)
    };
    if sessions.len()>=100{return secure_headers(StatusCode::TOO_MANY_REQUESTS);}
    sessions.insert(token.clone(),Session{
        csrf,until:Instant::now()+Duration::from_secs(8*3600),
        user:login.username.clone()
    });
    drop(sessions);
    audit_actor(&state.0,if local{"login_local_success"}else{"login_ldaps_success"},&login.username);
    let secure=if state.0.origin.starts_with("https://"){"; Secure"}else{""};
    let cookie=format!("mb_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{secure}");
    let mut response=Redirect::to("/").into_response();
    if let Ok(value)=cookie.parse(){response.headers_mut().insert(header::SET_COOKIE,value);}
    secure_headers(response)
}
#[derive(Deserialize)]
struct OidcReturn {code:Option<String>,state:Option<String>,error:Option<String>}
async fn oidc_start(State(state):State<AppState>)->Response{
    let Some(config)=state.0.oidc.clone() else {
        return secure_headers(StatusCode::NOT_FOUND);
    };
    let Ok((url,csrf,pending))=config.start().await else {
        audit(&state.0,"oidc_start_failed");
        return secure_headers(StatusCode::BAD_GATEWAY);
    };
    {
        let Ok(mut entries)=state.0.pending_oidc.lock() else {
            return secure_headers(StatusCode::INTERNAL_SERVER_ERROR);
        };
        entries.retain(|_,p|p.created.elapsed()<Duration::from_secs(600));
        if entries.len()>=100{return secure_headers(StatusCode::TOO_MANY_REQUESTS);}
        entries.insert(csrf.clone(),pending);
    }
    // Bind this OIDC state to the initiating browser, preventing login CSRF.
    let cookie=format!("mb_oidc_state={csrf}; Path=/oidc; Secure; HttpOnly; SameSite=Lax; Max-Age=600");
    let mut response=Redirect::temporary(&url).into_response();
    if let Ok(value)=cookie.parse(){response.headers_mut().insert(header::SET_COOKIE,value);}
    secure_headers(response)
}
async fn oidc_callback(State(state):State<AppState>,headers:HeaderMap,
    axum::extract::Query(params):axum::extract::Query<OidcReturn>)->Response{
    let Some(config)=state.0.oidc.clone() else {
        return secure_headers(StatusCode::NOT_FOUND);
    };
    if params.error.is_some(){return secure_headers(StatusCode::UNAUTHORIZED);}
    let Some(state_param)=params.state.filter(|s|s.len()>=16&&s.len()<=512) else {
        return secure_headers(StatusCode::UNAUTHORIZED);
    };
    if oidc_cookie_state(&headers).as_deref()!=Some(state_param.as_str()) {
        audit(&state.0,"oidc_browser_state_mismatch");
        return secure_headers(StatusCode::UNAUTHORIZED);
    }
    let Some(code)=params.code.filter(|s|!s.is_empty()&&s.len()<=8192) else {
        return secure_headers(StatusCode::UNAUTHORIZED);
    };
    let pending={
        let Ok(mut entries)=state.0.pending_oidc.lock() else {
            return secure_headers(StatusCode::INTERNAL_SERVER_ERROR);
        };
        entries.remove(&state_param)
    };
    let Some(pending)=pending else {
        audit(&state.0,"oidc_state_replay_rejected");
        return secure_headers(StatusCode::UNAUTHORIZED);
    };
    let subject=match config.complete(pending,code).await {
        Ok(subject)=>subject,
        Err(_)=>{
            audit(&state.0,"oidc_token_validation_failed");
            return secure_headers(StatusCode::UNAUTHORIZED);
        }
    };
    let cookie_id=random_token();
    let csrf=random_token();
    {
        let Ok(mut sessions)=state.0.sessions.lock() else {
            return secure_headers(StatusCode::INTERNAL_SERVER_ERROR);
        };
        if sessions.len()>=100{return secure_headers(StatusCode::TOO_MANY_REQUESTS);}
        sessions.insert(cookie_id.clone(),Session{
            csrf,until:Instant::now()+Duration::from_secs(8*3600),
            user:format!("oidc:{subject}")
        });
    }
    audit_actor(&state.0,"login_oidc_success",&format!("oidc:{subject}"));
    let session_cookie=format!("mb_session={cookie_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age=28800; Secure");
    let mut response=Redirect::to("/").into_response();
    if let Ok(value)=session_cookie.parse(){
        response.headers_mut().append(header::SET_COOKIE,value);
    }
    response.headers_mut().append(header::SET_COOKIE,
        "mb_oidc_state=; Path=/oidc; HttpOnly; Secure; SameSite=Lax; Max-Age=0".parse().unwrap());
    secure_headers(response)
}

async fn dashboard(State(state):State<AppState>,headers:HeaderMap)->Response {
    if let Some((_,auth))=session(&headers,&state.0) {
        let page=include_str!("../public/index.html").replace("CSRF_TOKEN", &auth.csrf);
        secure_headers(Html(page))
    } else {secure_headers(Redirect::to("/login"))}
}
fn unauthorized()->(StatusCode,Json<Value>) {
    (StatusCode::UNAUTHORIZED,Json(json!({"error":"authentication_required"})))
}
async fn backups(State(state):State<AppState>,headers:HeaderMap)
    ->Result<Json<Value>,(StatusCode,Json<Value>)> {
    if session(&headers,&state.0).is_none(){return Err(unauthorized());}
    let identity=state.0.identity.as_ref().ok_or_else(||(
        StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"vault_not_configured"}))))?;
    let store=multi_backup_core::chunk_store::Store::open(&state.0.root,identity)
        .map_err(|_|(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"storage_unavailable"}))))?;
    let list=store.list(identity).map_err(|_|(
        StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"error":"manifest_verification_failed"}))))?;
    Ok(Json(json!({"backups":list.into_iter().map(|s|json!({
        "id":s.id,"label":s.label,
        "files":s.entries.into_iter().map(|f|json!({"path":f.path,"size":f.bytes})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()})))
}
async fn export(State(state):State<AppState>,headers:HeaderMap,Json(req):Json<Export>)
    ->Result<Json<Value>,(StatusCode,Json<Value>)> {
    let auth=session(&headers,&state.0).ok_or_else(unauthorized)?.1;
    if !same_origin(&headers,&state.0) ||
       headers.get("x-csrf-token").and_then(|x|x.to_str().ok())!=Some(auth.csrf.as_str()) {
        return Err((StatusCode::FORBIDDEN,Json(json!({"error":"csrf_rejected"}))));
    }
    let identity=state.0.identity.as_ref().ok_or_else(||(
        StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"vault_not_configured"}))))?;
    fs::create_dir_all(&state.0.exports).map_err(|_|(
        StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"error":"export_root_unavailable"}))))?;
    let dir=state.0.exports.join(format!("export-{}",&random_token()[..24]));
    let store=multi_backup_core::chunk_store::Store::open(&state.0.root,identity)
        .map_err(|_|(StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"storage_unavailable"}))))?;
    if req.selection.is_some() && req.selections.is_some() {
        return Err((StatusCode::BAD_REQUEST,Json(json!({"error":"ambiguous_selection"}))));
    }
    let selections=req.selections.unwrap_or_else(||req.selection.into_iter().collect());
    let result=store.restore_many(identity,&req.backup_id,&selections,&dir)
        .map_err(|_|(StatusCode::BAD_REQUEST,Json(json!({"error":"restore_failed_integrity_or_selection"}))))?;
    audit_actor(&state.0,"selective_export_completed",&auth.user);
    Ok(Json(json!({"files":result,"server_directory":dir.to_string_lossy()})))
}
#[derive(Deserialize)]
struct AuditFilter {
    offset:Option<usize>,
    limit:Option<usize>,
    query:Option<String>,
}
fn paginate_audit(rows:Vec<Value>,filter:&AuditFilter)->(usize,Vec<Value>){
    let query=filter.query.as_deref().unwrap_or("").to_lowercase();
    let matched:Vec<_>=rows.into_iter().filter(|row|{
        let event=row["event"].as_str().unwrap_or("").to_lowercase();
        let user=row["user"].as_str().unwrap_or("").to_lowercase();
        query.is_empty()||event.contains(&query)||user.contains(&query)
    }).collect();
    let count=matched.len();
    let page=matched.into_iter().rev()
        .skip(filter.offset.unwrap_or(0).min(100000))
        .take(filter.limit.unwrap_or(10).clamp(1,100)).collect();
    (count,page)
}
async fn audit_list(State(state):State<AppState>,headers:HeaderMap,
    axum::extract::Query(filter):axum::extract::Query<AuditFilter>)
    ->Result<Json<Value>,(StatusCode,Json<Value>)>{
    let auth=session(&headers,&state.0).ok_or_else(unauthorized)?.1;
    if auth.user!=state.0.name{
        return Err((StatusCode::FORBIDDEN,Json(json!({"error":"admin_required"}))));
    }
    if filter.query.as_deref().unwrap_or("").len()>256 ||
       filter.offset.unwrap_or(0)>100000 {
        return Err((StatusCode::BAD_REQUEST,Json(json!({"error":"audit_filter_invalid"}))));
    }
    let mut rows=Vec::new();
    if let Ok(meta)=fs::symlink_metadata(&state.0.audit){
        if !meta.file_type().is_file()||meta.len()>32*1024*1024 {
            return Err((StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"audit_store_unavailable"}))));
        }
        use std::io::{BufRead,BufReader};
        let log=fs::File::open(&state.0.audit).map_err(|_|(
            StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"audit_read_failed"}))))?;
        for record in BufReader::new(log).lines(){
            let record=record.map_err(|_|(StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"audit_read_failed"}))))?;
            if record.len()>8192 {
                return Err((StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error":"audit_record_oversized"}))));
            }
            let row:Value=serde_json::from_str(&record).map_err(|_|(
                StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"audit_record_corrupt"}))))?;
            rows.push(row);
        }
    }
    let (total,events)=paginate_audit(rows,&filter);
    Ok(Json(json!({"total":total,"offset":filter.offset.unwrap_or(0),
        "limit":filter.limit.unwrap_or(10).clamp(1,100),"events":events})))
}

async fn smtp_status(State(state):State<AppState>,headers:HeaderMap)
    ->Result<Json<Value>,(StatusCode,Json<Value>)>{
    if session(&headers,&state.0).is_none(){return Err(unauthorized());}
    Ok(Json(json!({"configured":state.0.smtp.is_some(),
        "tls":state.0.smtp.as_ref().map(|v|v.protocol())})))
}
async fn smtp_test(State(state):State<AppState>,headers:HeaderMap)
    ->Result<Json<Value>,(StatusCode,Json<Value>)>{
    let auth=session(&headers,&state.0).ok_or_else(unauthorized)?.1;
    if !same_origin(&headers,&state.0)||
        headers.get("x-csrf-token").and_then(|s|s.to_str().ok())!=Some(auth.csrf.as_str()){
        return Err((StatusCode::FORBIDDEN,Json(json!({"error":"csrf_rejected"}))));
    }
    if auth.user!=state.0.name {
        return Err((StatusCode::FORBIDDEN,Json(json!({"error":"admin_required"}))));
    }
    {
        let mut last=state.0.last_smtp_test.lock().map_err(|_|(
            StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"error":"state_unavailable"}))))?;
        if last.is_some_and(|then|then.elapsed()<Duration::from_secs(60)) {
            return Err((StatusCode::TOO_MANY_REQUESTS,Json(json!({"error":"smtp_test_rate_limited"}))));
        }
        *last=Some(Instant::now());
    }
    let cfg=state.0.smtp.clone().ok_or_else(||(
        StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"smtp_not_configured"}))))?;
    let outcome=tokio::task::spawn_blocking(move||{
        cfg.send_notice("Multi Backup - test SMTP(S)",
            "Invio di verifica Multi Backup: TLS SMTP abilitato.").map_err(|e|e.to_string())
    }).await;
    match outcome{
        Ok(Ok(()))=>{
            audit(&state.0,"smtp_test_delivered");
            Ok(Json(json!({"sent":true})))
        },
        _=>{
            audit(&state.0,"smtp_test_failed");
            Err((StatusCode::BAD_GATEWAY,Json(json!({"error":"smtp_send_failed"}))))
        }
    }
}

async fn logout(State(state):State<AppState>,headers:HeaderMap)->Response {
    if let Some((token,s))=session(&headers,&state.0) {
        if same_origin(&headers,&state.0) &&
            headers.get("x-csrf-token").and_then(|x|x.to_str().ok())==Some(s.csrf.as_str()) {
            if let Ok(mut sessions)=state.0.sessions.lock(){sessions.remove(&token);}
            audit_actor(&state.0,"logout",&s.user);
            let mut result=Redirect::to("/login").into_response();
            result.headers_mut().insert(header::SET_COOKIE,
                "mb_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0".parse().unwrap());
            return secure_headers(result);
        }
    }
    secure_headers(StatusCode::FORBIDDEN)
}
fn read_hash(path:&str)->Result<String,Box<dyn std::error::Error+Send+Sync>> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len()>512 {
        return Err("Unsafe password hash file".into());
    }
    let hash=fs::read_to_string(path)?.trim().to_string();
    PasswordHash::new(&hash).map_err(|_|"Invalid Argon2 PHC hash")?;
    Ok(hash)
}
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error+Send+Sync>> {
    if std::env::args().any(|x|x=="hash-password") {
        let password=rpassword::prompt_password("Admin password (>=12 chars): ")?;
        if password.len()<12{return Err("Password too short".into());}
        let salt=SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
        println!("{}",Argon2::default().hash_password(password.as_bytes(),&salt)?);
        return Ok(());
    }
    let name=std::env::var("MULTI_BACKUP_ADMIN_USER")?;
    let hash=read_hash(&std::env::var("MULTI_BACKUP_ADMIN_HASH_FILE")?)?;
    let origin=std::env::var("MULTI_BACKUP_PUBLIC_ORIGIN")?;
    if !origin.starts_with("https://") && origin!="http://127.0.0.1:8080" {
        return Err("Production MUST use an HTTPS public origin".into());
    }
    let root=PathBuf::from(std::env::var("MULTI_BACKUP_DATA_ROOT")
        .unwrap_or_else(|_|"/data/backups".into()));
    let exports=PathBuf::from(std::env::var("MULTI_BACKUP_EXPORT_ROOT")
        .unwrap_or_else(|_|"/data/exports".into()));
    let identity=std::env::var_os("MULTI_BACKUP_AGE_IDENTITY_FILE").map(PathBuf::from);
    let audit=PathBuf::from(std::env::var("MULTI_BACKUP_AUDIT_LOG")
        .unwrap_or_else(|_|"/data/audit/events.jsonl".into()));
    let smtp=SmtpConfig::from_env()?;
    let dav=DavConfig::from_env(&origin)?;
    let ldap=LdapConfig::from_env()?;
    let oidc=OidcConfig::from_env(&origin)?;
    let state=AppState(Arc::new(AppConfig {
        name,password_hash:hash,origin,root,exports,identity,audit,smtp,dav,ldap,oidc,
        pending_oidc:Mutex::new(HashMap::new()),
        sessions:Mutex::new(HashMap::new()),
        failed_logins:Mutex::new(HashMap::new()),
        last_smtp_test:Mutex::new(None),
    }));
    let app=Router::new().route("/healthz",get(health))
        .route("/login",get(login_page).post(do_login))
        .route("/oidc/start",get(oidc_start))
        .route("/oidc/callback",get(oidc_callback))
        .route("/",get(dashboard))
        .route("/app.css",get(css)).route("/app.js",get(js))
        .route("/api/backups",get(backups))
        .route("/dav/:id",axum::routing::any(dav::root))
        .route("/dav/:id/",axum::routing::any(dav::root))
        .route("/dav/:id/*path",axum::routing::any(dav::file))
        .route("/api/export",post(export))
        .route("/api/audit",get(audit_list))
        .route("/api/smtp/status",get(smtp_status))
        .route("/api/smtp/test",post(smtp_test))
        .route("/logout",post(logout))
        .with_state(state);
    let bind=std::env::var("MULTI_BACKUP_BIND").unwrap_or_else(|_|"0.0.0.0:8080".into());
    let listener=tokio::net::TcpListener::bind(bind.parse::<SocketAddr>()?).await?;
    axum::serve(listener,app).await?;
    Ok(())
}

#[cfg(test)]
mod security_tests{
    use super::*;
    #[test]
    fn oidc_browser_binding_rejects_missing_or_untrusted_cookie(){
        let mut headers=HeaderMap::new();
        assert!(oidc_cookie_state(&headers).is_none());
        headers.insert(header::COOKIE,
            "extra=a; mb_oidc_state=abcdefgh12345678; more=x".parse().unwrap());
        assert_eq!(oidc_cookie_state(&headers).as_deref(),Some("abcdefgh12345678"));
        headers.insert(header::COOKIE,"mb_oidc_state=../../".parse().unwrap());
        assert!(oidc_cookie_state(&headers).is_none());
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    #[test]
    fn pagination_is_recent_first_and_search_filters_actor_or_event(){
        let rows=vec![
            json!({"event":"login_failed","user":"alice"}),
            json!({"event":"backup_restore","user":"bob"}),
            json!({"event":"login_success","user":"alice"})
        ];
        let (total,page)=paginate_audit(rows.clone(),&AuditFilter{
            offset:0,limit:Some(1),query:Some("alice".into())});
        assert_eq!(total,2);
        assert_eq!(page[0]["event"],"login_success");
        let (total,page)=paginate_audit(rows,&AuditFilter{
            offset:1,limit:Some(1),query:None});
        assert_eq!(total,3);
        assert_eq!(page[0]["event"],"backup_restore");
    }
}
