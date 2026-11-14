//! Single-user web preview for the compiled Rust backup core.
//! Fail closed: local Argon2id login is operational; LDAP/OIDC MUST NOT be
//! advertised or enabled until implemented and externally verified.
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
    sessions: Mutex<HashMap<String, Session>>,
}
#[derive(Clone)]
struct Session { csrf: String, until: Instant }
#[derive(Deserialize)]
struct Login { username: String, password: String }
impl Drop for Login { fn drop(&mut self) { self.password.zeroize(); } }
#[derive(Deserialize)]
struct Export { backup_id: String, selection: Option<String>, selections: Option<Vec<String>> }

fn random_token() -> String {
    let mut bytes=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut bytes);hex::encode(bytes)
}
fn audit(state:&AppConfig,event:&str) {
    if let Some(parent)=state.audit.parent(){let _=fs::create_dir_all(parent);}
    if let Ok(mut log)=OpenOptions::new().create(true).append(true).open(&state.audit) {
        let line=json!({"time":chrono::Utc::now().to_rfc3339(),"event":event,"user":state.name});
        let _=writeln!(log,"{}",line);
        let _=log.sync_data();
    }
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
async fn login_page()->Response {
    secure_headers(Html(include_str!("../public/login.html")))
}
async fn do_login(State(state):State<AppState>,headers:HeaderMap,
                  Form(login):Form<Login>)->Response {
    if !same_origin(&headers,&state.0){return secure_headers(StatusCode::FORBIDDEN);}
    let verified=PasswordHash::new(&state.0.password_hash).ok()
        .map(|hash| Argon2::default().verify_password(login.password.as_bytes(),&hash).is_ok())
        .unwrap_or(false);
    if login.username!=state.0.name || !verified {
        audit(&state.0,"login_failed");
        return secure_headers((StatusCode::UNAUTHORIZED,Html("Invalid credentials")));
    }
    let token=random_token();
    let csrf=random_token();
    let mut sessions=match state.0.sessions.lock() {
        Ok(value)=>value,Err(_)=>return secure_headers(StatusCode::INTERNAL_SERVER_ERROR)
    };
    if sessions.len()>=100 {return secure_headers(StatusCode::TOO_MANY_REQUESTS);}
    sessions.insert(token.clone(),Session {csrf,until:Instant::now()+Duration::from_secs(8*3600)});
    drop(sessions);
    audit(&state.0,"login_success");
    let secure=if state.0.origin.starts_with("https://"){"; Secure"}else{""};
    let cookie=format!("mb_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{secure}");
    let mut response=Redirect::to("/").into_response();
    if let Ok(value)=cookie.parse(){response.headers_mut().insert(header::SET_COOKIE,value);}
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
    audit(&state.0,"selective_export_completed");
    Ok(Json(json!({"files":result,"server_directory":dir.to_string_lossy()})))
}
async fn logout(State(state):State<AppState>,headers:HeaderMap)->Response {
    if let Some((token,s))=session(&headers,&state.0) {
        if same_origin(&headers,&state.0) &&
            headers.get("x-csrf-token").and_then(|x|x.to_str().ok())==Some(s.csrf.as_str()) {
            if let Ok(mut sessions)=state.0.sessions.lock(){sessions.remove(&token);}
            audit(&state.0,"logout");
            let mut result=Redirect::to("/login").into_response();
            result.headers_mut().insert(header::SET_COOKIE,
                "mb_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0".parse().unwrap());
            return secure_headers(result);
        }
    }
    secure_headers(StatusCode::FORBIDDEN)
}
fn read_hash(path:&str)->Result<String,Box<dyn std::error::Error>> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len()>512 {
        return Err("Unsafe password hash file".into());
    }
    let hash=fs::read_to_string(path)?.trim().to_string();
    PasswordHash::new(&hash).map_err(|_|"Invalid Argon2 PHC hash")?;
    Ok(hash)
}
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>> {
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
    let state=AppState(Arc::new(AppConfig {
        name,password_hash:hash,origin,root,exports,identity,audit,
        sessions:Mutex::new(HashMap::new()),
    }));
    let app=Router::new().route("/healthz",get(health))
        .route("/login",get(login_page).post(do_login))
        .route("/",get(dashboard))
        .route("/app.css",get(css)).route("/app.js",get(js))
        .route("/api/backups",get(backups))
        .route("/api/export",post(export))
        .route("/logout",post(logout))
        .with_state(state);
    let bind=std::env::var("MULTI_BACKUP_BIND").unwrap_or_else(|_|"0.0.0.0:8080".into());
    let listener=tokio::net::TcpListener::bind(bind.parse::<SocketAddr>()?).await?;
    axum::serve(listener,app).await?;
    Ok(())
}
