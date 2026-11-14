//! Interoperable encrypted identity vault operations in native Rust.
//! V1 matches Python: scrypt N=32768,r=8,p=1 + AES-256-GCM + recipient AAD.
//! No plaintext private keys are written to disk or printed. Never overwrite
//! an existing vault without an explicit authenticated change-password call.
use aes_gcm::{aead::{Aead, Payload}, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

type VaultResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_BYTES: u64 = 65_536;

#[derive(Debug, Deserialize)]
struct Envelope {
    schema: u64, kdf: String, n: u64, r: u64, p: u64,
    cipher: String, recipient: String, salt: String, nonce: String,
    ciphertext: String,
}

fn allowed_recipient(s: &str) -> bool {
    s.starts_with("age1") && s.len() >= 24 &&
        s[4..].bytes().all(|b| b"023456789acdefghjklmnpqrstuvwxyz".contains(&b))
}

fn valid_private_key(clear: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(clear) else { return false };
    text.lines().any(|line| {
        let candidate = line.trim();
        let Some(suffix) = candidate.strip_prefix("AGE-SECRET-KEY-") else {
            return false
        };
        !suffix.is_empty() && suffix.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    })
}

fn validate_password(password: &str, creating: bool) -> VaultResult<()> {
    if password.is_empty() || password.len() > 1024 ||
       (creating && password.chars().count() < 12) {
        return Err("Vault password must be at least 12 characters and at most 1024 bytes".into());
    }
    Ok(())
}

fn open(path: &Path, password: &str) -> VaultResult<(Zeroizing<Vec<u8>>, String)> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > MAX_BYTES {
        return Err("Invalid or unsafe encrypted identity vault".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("Encrypted vault permissions must be mode 0600".into());
        }
    }
    validate_password(password, false)?;
    let mut raw = Vec::with_capacity(meta.len() as usize);
    File::open(path)?.take(MAX_BYTES + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_BYTES {
        return Err("Vault exceeds permitted size".into());
    }
    let envelope: Envelope = serde_json::from_slice(&raw)?;
    if envelope.schema != 1 || envelope.kdf != "scrypt" ||
       envelope.n != 32_768 || envelope.r != 8 || envelope.p != 1 ||
       envelope.cipher != "AES-256-GCM" ||
       !allowed_recipient(&envelope.recipient) {
        return Err("Unsupported or malformed encrypted vault format".into());
    }
    let salt = STANDARD.decode(&envelope.salt)?;
    let nonce = STANDARD.decode(&envelope.nonce)?;
    let ciphertext = STANDARD.decode(&envelope.ciphertext)?;
    if salt.len() != 16 || nonce.len() != 12 ||
       ciphertext.len() <= 16 || ciphertext.len() >= 32_768 {
        return Err("Malformed encrypted vault field lengths".into());
    }
    let params = scrypt::Params::new(15, 8, 1, 32)
        .map_err(|_| "Unsupported scrypt vault parameters")?;
    let mut key = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut *key)
        .map_err(|_| "Could not derive vault encryption key")?;
    let cipher = Aes256Gcm::new_from_slice(&*key)
        .map_err(|_| "Invalid AES-GCM key")?;
    let aad = format!("multi-backup/identity-v1/{}", envelope.recipient);
    let clear = cipher.decrypt(Nonce::from_slice(&nonce), Payload {
        msg: &ciphertext, aad: aad.as_bytes(),
    }).map_err(|_| "Wrong password or invalid authenticated vault")?;
    let identity = Zeroizing::new(clear);
    if identity.len() > 16_384 || !valid_private_key(&identity) {
        return Err("Recovered age identity failed validation".into());
    }
    Ok((identity, envelope.recipient))
}

pub fn verify(path: &Path, password: &str) -> VaultResult<String> {
    let (_, recipient) = open(path, password)?;
    Ok(recipient)
}

fn seal(clear: &[u8], recipient: &str, password: &str) -> VaultResult<Value> {
    validate_password(password, true)?;
    if !allowed_recipient(recipient) || !valid_private_key(clear) || clear.len() > 16_384 {
        return Err("Invalid public recipient or private identity".into());
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let params = scrypt::Params::new(15, 8, 1, 32)
        .map_err(|_| "Unsupported scrypt vault parameters")?;
    let mut key = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut *key)
        .map_err(|_| "Could not derive vault encryption key")?;
    let cipher = Aes256Gcm::new_from_slice(&*key)
        .map_err(|_| "Invalid AES-GCM key")?;
    let aad = format!("multi-backup/identity-v1/{recipient}");
    let encrypted = cipher.encrypt(Nonce::from_slice(&nonce),
        Payload { msg: clear, aad: aad.as_bytes() })
        .map_err(|_| "Vault encryption failed")?;
    Ok(json!({
        "schema": 1, "kdf": "scrypt", "n": 32768, "r": 8, "p": 1,
        "cipher": "AES-256-GCM", "recipient": recipient,
        "salt": STANDARD.encode(salt), "nonce": STANDARD.encode(nonce),
        "ciphertext": STANDARD.encode(encrypted),
    }))
}

fn save_vault(path: &Path, value: &Value, overwrite: bool) -> VaultResult<()> {
    if path.extension().and_then(|x| x.to_str()) != Some("agevault") {
        return Err("Protected vault path must end in .agevault".into());
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Ok(meta) = fs::symlink_metadata(parent) {
        if meta.file_type().is_symlink() {
            return Err("Refusing symlinked vault directory".into());
        }
    }
    fs::create_dir_all(parent)?;
    if fs::symlink_metadata(parent)?.file_type().is_symlink() {
        return Err("Refusing symlinked vault directory".into());
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !overwrite || !meta.file_type().is_file() {
            return Err("Existing vault cannot be overwritten".into());
        }
    } else if overwrite {
        return Err("Password change requires an existing vault".into());
    }
    let mut temporary = NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut serialized = serde_json::to_vec(value)?;
    serialized.push(b'\n');
    if serialized.len() as u64 > MAX_BYTES {
        return Err("Encrypted vault size limit exceeded".into());
    }
    temporary.write_all(&serialized)?;
    temporary.as_file().sync_all()?;
    if overwrite {
        temporary.persist(path).map_err(|e| e.error)?;
    } else {
        temporary.persist_noclobber(path).map_err(|e| e.error)?;
    }
    Ok(())
}

pub fn create(path: &Path, password: &str) -> VaultResult<String> {
    validate_password(password, true)?;
    if fs::symlink_metadata(path).is_ok() {
        return Err("Protected vault already exists".into());
    }
    if path.file_name().and_then(|n| n.to_str()) == Some("default.agevault") {
        let legacy = path.parent().unwrap_or_else(|| Path::new(".")).join("default.txt");
        if fs::symlink_metadata(legacy).is_ok() {
            return Err("Legacy plaintext identity exists; migrate it instead".into());
        }
    }
    let generated = Command::new("age-keygen")
        .stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
    if !generated.status.success() || !valid_private_key(&generated.stdout) {
        return Err("age-keygen failed to generate a private identity".into());
    }
    let clear = Zeroizing::new(generated.stdout);
    let mut derive = Command::new("age-keygen").arg("-y")
        .stdin(Stdio::piped()).stdout(Stdio::piped())
        .stderr(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = derive.stdin.take() {
        stdin.write_all(&clear)?;
    } else {
        let _ = derive.kill();
        return Err("Cannot pass age private identity to age-keygen".into());
    }
    let result = derive.wait_with_output()?;
    let recipient = String::from_utf8(result.stdout)?.trim().to_string();
    if !result.status.success() || !allowed_recipient(&recipient) {
        return Err("Could not derive age public recipient".into());
    }
    save_vault(path, &seal(&clear, &recipient, password)?, false)?;
    if verify(path, password)? != recipient {
        return Err("Created identity failed authenticated verification".into());
    }
    Ok(recipient)
}

pub fn export_recovery(source: &Path, destination: &Path,
    password: &str, recovery_password: &str) -> VaultResult<String>
{
    let (identity, recipient) = open(source, password)?;
    save_vault(destination, &seal(&identity, &recipient, recovery_password)?, false)?;
    if verify(destination, recovery_password)? != recipient {
        return Err("Recovery copy did not authenticate".into());
    }
    Ok(recipient)
}

pub fn import_recovery(source: &Path, destination: &Path,
    recovery_password: &str, new_password: &str) -> VaultResult<String>
{
    export_recovery(source, destination, recovery_password, new_password)
}

pub fn change_password(path: &Path, password: &str, new_password: &str)
    -> VaultResult<String>
{
    let (identity, recipient) = open(path, password)?;
    let original = fs::read(path)?; // only encrypted bytes; retained for rollback
    let replacement = seal(&identity, &recipient, new_password)?;
    save_vault(path, &replacement, true)?;
    match verify(path, new_password) {
        Ok(value) if value == recipient => Ok(recipient),
        _ => {
            // Restore the original authenticated encrypted vault on failure.
            let old: Value = serde_json::from_slice(&original)?;
            save_vault(path, &old, true)?;
            Err("Password change did not verify; original vault restored".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_age_recipient_encoding() {
        assert!(!allowed_recipient("https://example.com"));
        assert!(!allowed_recipient("age1bad!"));
        assert!(!allowed_recipient("age1"));
        assert!(allowed_recipient("age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"));
    }
    #[test]
    fn rejects_plaintext_and_oversized_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-vault.agevault");
        fs::write(&file, b"AGE-SECRET-KEY-SHOULD-NOT-BE-ACCEPTED").unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(verify(&file, "anything").is_err());
    }
}
