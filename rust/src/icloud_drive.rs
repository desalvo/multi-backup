//! iCloud Drive B-In/B-Out preview via an operator-configured external rclone
//! iclouddrive remote. No Apple passwords/tokens enter Multi Backup, no
//! plaintext is written locally by the B-In data path. NOT a live certified
//! Apple API integration or a general backup/restore engine.
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::Builder;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn denied(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    message.into()
}
fn config_guard(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len() > 65536 {
        return Err(denied("iCloud rclone config must be an ordinary private file <=64KiB"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(denied("iCloud rclone config requires owner-only 0600"));
        }
    }
    // Check ONLY our dedicated [icloud] stanza, not any unrelated remote.
    let text = fs::read_to_string(path)?;
    let mut section = String::new();
    let mut remote_type = String::new();
    let mut service = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len()-1].to_owned();
        } else if section == "icloud" {
            if let Some((key,value)) = line.split_once('=') {
                match key.trim() {
                    "type" => remote_type = value.trim().to_owned(),
                    "service" => service = value.trim().to_owned(),
                    _ => {}
                }
            }
        }
    }
    if remote_type != "iclouddrive" || !(service.is_empty() || service == "drive") {
        return Err(denied("Only an [icloud] type=iclouddrive service=drive remote is allowed"));
    }
    Ok(())
}
fn remote_path(value: &str, directory: bool) -> Result<()> {
    let rest = value.strip_prefix("icloud:")
        .ok_or_else(|| denied("Remote must use the explicitly configured icloud: remote"))?;
    if rest.is_empty() || rest.len()>768 || rest.starts_with('/') || rest.ends_with('/')
        || rest.contains('\\') || rest.chars().any(char::is_control) {
        return Err(denied("Unsafe iCloud Drive remote path"));
    }
    let segments: Vec<_> = rest.split('/').collect();
    if segments.iter().any(|s| s.is_empty() || *s=="." || *s==".." || s.contains(':')) {
        return Err(denied("Invalid or traversing remote path"));
    }
    if !directory && segments.last().map_or(true, |part| part.is_empty()) {
        return Err(denied("iCloud source must name an exact file"));
    }
    Ok(())
}
fn sha_arg(value: &str) -> Result<String> {
    if value.len()!=64 || !value.bytes().all(|x| x.is_ascii_hexdigit()) {
        return Err(denied("Expected exact 64-hex SHA256"));
    }
    Ok(value.to_ascii_lowercase())
}
fn exact_regular_file(path: &Path) -> Result<u64> {
    let meta=fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len()==0 {
        return Err(denied("Expected an existing nonempty ordinary file"));
    }
    Ok(meta.len())
}
fn private_parent(destination: &Path) -> Result<()> {
    let parent=destination.parent().ok_or_else(||denied("No destination parent"))?;
    let meta=fs::symlink_metadata(parent)?;
    if !meta.is_dir() || destination.exists() {
        return Err(denied("Destination parent must exist; target must not exist"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(denied("Local staging parent must have owner-only 0700 permissions"));
        }
    }
    Ok(())
}
fn run_rclone(config: &Path, args: &[&str]) -> Result<std::process::Child> {
    // No shell, no HTTP/TLS insecure flags, no secrets in argv. The rclone
    // config was explicitly created outside this application using 2FA.
    let mut command=Command::new("rclone");
    command.arg("--config").arg(config)
        .arg("--disable-http2") // provider compatibility; TLS remains mandatory
        .args(args)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    Ok(command.spawn()?)
}
fn hash_stream(mut source: impl Read, max_bytes: u64, mut write: impl Write)
    -> Result<(String,u64)> {
    let mut digest=Sha256::new();
    let mut bytes=0u64;
    let mut buffer=[0u8;65536];
    loop {
        let count=source.read(&mut buffer)?;
        if count==0 {break}
        bytes=bytes.checked_add(count as u64).ok_or_else(||denied("Size overflow"))?;
        if bytes>max_bytes {return Err(denied("iCloud file exceeds selected transfer limit"))}
        write.write_all(&buffer[..count])?;
        digest.update(&buffer[..count]);
    }
    write.flush()?;
    Ok((hex::encode(digest.finalize()),bytes))
}
fn verified_hash(path: &Path) -> Result<(String,u64)> {
    let len=exact_regular_file(path)?;
    let (hash,read)=hash_stream(File::open(path)?,len,std::io::sink())?;
    if read != len {return Err(denied("Archive changed during hashing"))}
    Ok((hash,len))
}
fn pin_remote_digest(config: &Path, remote: &str, expected: &str, max_bytes: u64)
    -> Result<u64> {
    let mut child=run_rclone(config,&["cat",remote])?;
    let output=child.stdout.take().ok_or_else(||denied("No rclone stdout"))?;
    let read=hash_stream(output,max_bytes,std::io::sink());
    if read.is_err() { let _=child.kill(); }
    let result=child.wait()?;
    if !result.success() {return Err(denied("iCloud remote readback failed"))}
    let (hash,count)=read?;
    if hash!=expected {return Err(denied("Remote iCloud SHA256 readback mismatch"))}
    Ok(count)
}
fn age_recipient(value: &str) -> Result<()> {
    if !value.starts_with("age1") || value.len()<30 || value.len()>160
        || !value.bytes().all(|b|b.is_ascii_alphanumeric()) {
        return Err(denied("A valid public age1 recipient is mandatory"));
    }
    Ok(())
}

/// B-In: exact object whose independent digest was supplied by the operator,
/// then authenticated age ciphertext published create-only in the private
/// destination. No plaintext tempfile is ever created.
pub fn import_file(config:&Path, remote:&str, recipient:&str,
                   expected_sha:&str, dest:&Path, limit:u64) -> Result<Value> {
    config_guard(config)?;
    remote_path(remote,false)?;
    age_recipient(recipient)?;
    let expected=sha_arg(expected_sha)?;
    if limit==0 || limit>128*1024*1024*1024 {return Err(denied("Transfer cap must be 1..128GiB"))}
    if dest.extension().and_then(|s|s.to_str())!=Some("age"){
        return Err(denied("iCloud B-In must produce encrypted .age output"));
    }
    private_parent(dest)?;
    let parent=dest.parent().ok_or_else(||denied("Missing parent"))?;
    let tmp=Builder::new().prefix(".mb-icloud-").tempdir_in(parent)?;
    let cipher=tmp.path().join("sealed.age");
    let mut rclone=run_rclone(config,&["cat",remote])?;
    let input=rclone.stdout.take().ok_or_else(||denied("Missing rclone pipe"))?;
    let mut age=Command::new("age")
        .args(["-r",recipient,"-o"]).arg(&cipher)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let pipe=age.stdin.take().ok_or_else(||denied("Missing age pipe"))?;
    let result=hash_stream(input,limit,pipe);
    if result.is_err(){let _=rclone.kill();let _=age.kill();}
    let source_ok=rclone.wait()?.success();
    let sealed_ok=age.wait()?.success();
    if !source_ok || !sealed_ok{return Err(denied("iCloud import or age encryption failed"))}
    let (source_hash,source_size)=result?;
    if source_hash!=expected{return Err(denied("iCloud imported data did not match pinned SHA256"))}
    let (cipher_hash,cipher_size)=verified_hash(&cipher)?;
    // hard_link is an atomic O_EXCL-style create-only publish within same FS.
    fs::hard_link(&cipher,dest)?;
    Ok(json!({"plugin":"icloud","direction":"b-in","stage":"preview",
      "source_remote":remote,"source_sha256":source_hash,"source_bytes":source_size,
      "ciphertext":dest.display().to_string(),"ciphertext_sha256":cipher_hash,"ciphertext_bytes":cipher_size}))
}

/// B-Out: store an already age-encrypted, independently authenticated archive
/// under a random new iCloud Drive object key. No deletion, rename or sync.
/// rclone ignore-existing isn't a provider-side conditional PUT: race-free
/// remote create-only is NOT certified; randomly named objects minimize risk.
pub fn upload_ciphertext(config:&Path, remote_parent:&str, archive:&Path,
                         identity:&Path, expected_sha:&str, limit:u64) -> Result<Value> {
    config_guard(config)?;
    remote_path(remote_parent,true)?;
    if archive.extension().and_then(|s|s.to_str())!=Some("age"){
        return Err(denied("B-Out accepts only .age ciphertext"));
    }
    let expected=sha_arg(expected_sha)?;
    if limit==0 || limit>128*1024*1024*1024 {return Err(denied("Transfer cap must be 1..128GiB"))}
    exact_regular_file(identity)?;
    let (hash,len)=verified_hash(archive)?;
    if hash!=expected || len>limit {return Err(denied("Archive digest/size mismatch"))}
    let decrypted=Command::new("age").args(["-d","-i"]).arg(identity)
        .arg(archive).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    if !decrypted.success(){return Err(denied("Ciphertext is not authenticated age data for supplied identity"))}
    let mut entropy=[0u8;16];
    OsRng.fill_bytes(&mut entropy);
    let remote=format!("{remote_parent}/mb-{}.age",hex::encode(entropy));
    remote_path(&remote,false)?;
    let archive_arg=archive.to_str().ok_or_else(||denied("Non-UTF8 local archive path"))?;
    let mut child=run_rclone(config,&["copyto","--ignore-existing",
                                    "--retries","1",archive_arg,&remote])?;
    let status=child.wait()?;
    if !status.success(){return Err(denied("iCloud ciphertext upload failed"))}
    let actual=pin_remote_digest(config,&remote,&hash,len)?;
    if actual!=len {return Err(denied("Uploaded iCloud ciphertext size differs"))}
    Ok(json!({"plugin":"icloud","direction":"b-out","stage":"preview",
         "remote":remote,"ciphertext_sha256":hash,"ciphertext_bytes":len,
         "verified_remote_readback":true,
         "provider_atomic_if_none_match":false}))
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn remote_must_be_explicit_icloud_drive_not_photos_or_traversal(){
        for bad in ["other:a","icloud:","icloud:/abs","icloud:../secret",
                    "icloud:a//b","icloud:a/./x","icloud:photo:\\x","icloud:a\\b",
                    "icloud:foo\nbar"] {
            assert!(remote_path(bad,false).is_err(),"{bad}");
        }
        assert!(remote_path("icloud:Multi Backup/Archivio.age",false).is_ok());
    }
    #[test]
    fn config_rejects_untrusted_other_backend_and_world_readable(){
        let tmp=tempfile::tempdir().unwrap();
        let path=tmp.path().join("rclone.conf");
        fs::write(&path,"[icloud]\ntype = local\nservice = drive\n").unwrap();
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path,fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(config_guard(&path).is_err());
        fs::write(&path,"[fake]\ntype = iclouddrive\n[icloud]\ntype = iclouddrive\nservice = photos\n").unwrap();
        assert!(config_guard(&path).is_err());
        fs::write(&path,"[icloud]\ntype = iclouddrive\nservice = drive\n").unwrap();
        assert!(config_guard(&path).is_ok());
    }
    #[test]
    fn digest_guards_are_fail_closed(){
        assert!(sha_arg(&"a".repeat(64)).is_ok());
        assert!(sha_arg("bad").is_err());
        assert!(age_recipient("not-an-age-key").is_err());
    }
}
