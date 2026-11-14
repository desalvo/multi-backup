//! Public security primitives; no vault topology, key or trust material here.
//! Protected deployments require a separate mutual-TLS gateway and offline
//! recovery authorization. A reverse proxy MUST strip all client-supplied
//! X-Multi-Backup-Private-Gate headers before injecting its own credential.
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{collections::HashSet, env, fs, path::Path, sync::Arc};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
use super::AppState;

#[derive(Clone)]
pub struct SecurityPolicy {
    hardened: bool,
    gate: Option<Arc<Zeroizing<Vec<u8>>>>,
    recovery: Option<Arc<Zeroizing<Vec<u8>>>>,
    readers: HashSet<String>,
}

type ErrBox = Box<dyn std::error::Error + Send + Sync>;
fn read_token(path: &Path) -> Result<Arc<Zeroizing<Vec<u8>>>, ErrBox> {
    // Kubernetes Secret projections use symlinks within the same Secret
    // volume. Resolve only to a regular file beneath the same trusted root.
    let parent = fs::canonicalize(path.parent().ok_or("Secret path has no parent")?)?;
    let resolved = fs::canonicalize(path)?;
    if !resolved.starts_with(&parent) {
        return Err("Secret symlink escapes its directory".into());
    }
    let metadata = fs::metadata(&resolved)?;
    if !metadata.is_file() || metadata.len() > 256 {
        return Err("Invalid secret file".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o007 != 0 {
            return Err("Secret must not be readable by other users".into());
        }
    }
    let mut data = Zeroizing::new(fs::read(&resolved)?);
    while data.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {data.pop();}
    if data.len() != 64 || !data.iter().all(|b| b.is_ascii_hexdigit()) {
        return Err("Expected a 64-hex-character random secret".into());
    }
    Ok(Arc::new(data))
}
fn env_enabled(key: &str) -> Result<bool, ErrBox> {
    match env::var(key).as_deref() {
        Ok("true"|"1") => Ok(true),
        Ok("false"|"0") | Err(env::VarError::NotPresent) => Ok(false),
        _ => Err(format!("Invalid boolean setting {key}").into())
    }
}
impl SecurityPolicy {
    pub fn from_env(origin: &str) -> Result<Self, ErrBox> {
        let hardened = env_enabled("MULTI_BACKUP_HARDENED_MODE")?;
        if hardened && !origin.starts_with("https://") {
            return Err("Hardened mode requires a verified HTTPS public origin".into());
        }
        let gate = env::var_os("MULTI_BACKUP_PRIVATE_GATE_SECRET_FILE")
            .map(|p|read_token(Path::new(&p))).transpose()?;
        let recovery = env::var_os("MULTI_BACKUP_RECOVERY_SECRET_FILE")
            .map(|p|read_token(Path::new(&p))).transpose()?;
        if hardened && (gate.is_none() || recovery.is_none()) {
            return Err("Hardened mode requires separate private-gate and recovery secrets".into());
        }
        let raw = env::var("MULTI_BACKUP_AUTHORIZED_READERS").unwrap_or_default();
        if raw.len() > 16*1024 {return Err("Reader list is oversized".into());}
        let readers: HashSet<_> = raw.split(',').map(str::trim)
            .filter(|v|!v.is_empty()).map(str::to_owned).collect();
        if readers.iter().any(|v| v.len()>256 || v.chars().any(char::is_control) ||
            !(v.starts_with("oidc:")||v.starts_with("ldap:"))) {
            return Err("Reader identities must be explicit oidc:/ldap: subjects".into());
        }
        Ok(Self{hardened,gate,recovery,readers})
    }
    fn match_header(value: Option<&axum::http::HeaderValue>,
                    secret: &Option<Arc<Zeroizing<Vec<u8>>>>, default:bool) -> bool {
        let Some(key)=secret else {return default;};
        let Some(v)=value else {return false;};
        let bytes=v.as_bytes();
        if bytes.len()!=key.len(){return false;}
        bytes.ct_eq(key.as_slice()).unwrap_u8()==1
    }
    pub fn gateway_ok(&self, headers:&HeaderMap)->bool {
        Self::match_header(headers.get("x-multi-backup-private-gate"),
                           &self.gate,!self.hardened)
    }
    pub fn recovery_ok(&self, headers:&HeaderMap)->bool {
        Self::match_header(headers.get("x-multi-backup-recovery-authorization"),
                           &self.recovery,!self.hardened)
    }
    pub fn can_browse(&self, user:&str, administrator:&str)->bool {
        user==administrator || self.readers.contains(user)
    }
    pub fn can_restore(&self, user:&str, administrator:&str)->bool {
        user==administrator
    }
    pub fn hardened(&self)->bool {self.hardened}
    pub fn private_gate_enabled(&self)->bool {self.gate.is_some()}
    pub fn approval_enabled(&self)->bool {self.recovery.is_some()}
}
pub async fn enforce_gateway(State(state):State<AppState>,request:Request,next:Next)->Response {
    // Only a liveness endpoint is exempt; ingress MUST NOT publish it.
    if request.uri().path()=="/healthz" {
        return next.run(request).await;
    }
    if !state.0.security.gateway_ok(request.headers()) {
        // Do not reveal whether this is an authentication or a vault endpoint.
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}
#[cfg(test)]
mod tests {
    use super::*;
    fn policy(hardened:bool)->SecurityPolicy {
        SecurityPolicy{
            hardened,
            gate:Some(Arc::new(Zeroizing::new(vec![b'a';64]))),
            recovery:Some(Arc::new(Zeroizing::new(vec![b'b';64]))),
            readers:HashSet::from(["oidc:reader-123".into()])
        }
    }
    #[test]
    fn rejects_missing_wrong_or_short_gateway_token() {
        let secured=policy(true);
        let mut h=HeaderMap::new();
        assert!(!secured.gateway_ok(&h));
        h.insert("x-multi-backup-private-gate","wrong".parse().unwrap());
        assert!(!secured.gateway_ok(&h));
        h.insert("x-multi-backup-private-gate","a".repeat(64).parse().unwrap());
        assert!(secured.gateway_ok(&h));
        h.insert("x-multi-backup-private-gate","a".repeat(63).parse().unwrap());
        assert!(!secured.gateway_ok(&h));
        assert!(!secured.recovery_ok(&h));
    }
    #[test]
    fn recovery_requires_separate_permission() {
        let p=policy(true);
        let mut h=HeaderMap::new();
        h.insert("x-multi-backup-private-gate","a".repeat(64).parse().unwrap());
        assert!(p.gateway_ok(&h));
        assert!(!p.recovery_ok(&h));
        h.insert("x-multi-backup-recovery-authorization","b".repeat(64).parse().unwrap());
        assert!(p.recovery_ok(&h));
    }
    #[test]
    fn user_privileges_are_deny_by_default() {
        let p=policy(true);
        assert!(p.can_restore("admin","admin"));
        assert!(!p.can_restore("oidc:reader-123","admin"));
        assert!(p.can_browse("oidc:reader-123","admin"));
        assert!(!p.can_browse("ldap:unexpected","admin"));
    }
}
