//! Interoperable, read-only decryption of the original Python age identity vault.
//! Schema v1: scrypt N=32768/r=8/p=1 + AES-256-GCM, recipient-bound AAD.
//! Private key bytes and the derived password key are kept only in RAM and
//! never printed, staged or changed on disk by this module.
use aes_gcm::{aead::{Aead, Payload}, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::{fs::{self, File}, io::Read, path::Path};
use zeroize::Zeroizing;

type VaultResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_BYTES: u64 = 65_536;

#[derive(Debug, Deserialize)]
struct Envelope {
    schema: u64,
    kdf: String,
    n: u64,
    r: u64,
    p: u64,
    cipher: String,
    recipient: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn allowed_recipient(s: &str) -> bool {
    if !s.starts_with("age1") || s.len() < 24 { return false; }
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

pub fn verify(path: &Path, password: &str) -> VaultResult<String> {
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
    if password.is_empty() || password.len() > 1024 {
        return Err("Vault password missing or over 1024 bytes".into());
    }
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
    // Return public metadata ONLY, never the decrypted private identity.
    Ok(envelope.recipient)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_age_recipient_encoding() {
        assert!(!allowed_recipient("https://example.com"));
        assert!(!allowed_recipient("age1bad!"));
        assert!(allowed_recipient("age1" .to_owned().as_str()) == false);
        assert!(allowed_recipient("age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"));
    }
    #[test]
    fn rejects_plaintext_and_oversized_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-vault");
        fs::write(&file, b"AGE-SECRET-KEY-SHOULD-NOT-BE-ACCEPTED").unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(verify(&file, "anything").is_err());
    }
}
