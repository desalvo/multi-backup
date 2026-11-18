//! Native Rust desktop cloud mounts: explicit profiles, no Python runtime,
//! no shell and no rclone remote-control credential-bearing listener.
use std::{collections::HashSet, fs, path::{Path,PathBuf}, process::{Child,Command,Stdio}};
use serde_json::Value;

#[derive(Clone,Debug)]
pub struct Profile {
    pub id:String,
    pub provider:String,
    pub remote:String,
    pub prefix:String,
    pub allow_upload:bool,
    pub allow_delete:bool,
}
fn name(s:&str)->bool {
    !s.is_empty() && s.len()<=64 &&
        s.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'_' || b==b'-')
}
fn relative(s:&str)->bool {
    s.len()<=1024 && (s.is_empty() || (!s.starts_with('/') &&
        !s.contains('\\') && !s.contains(':') &&
        !s.chars().any(char::is_control) &&
        s.split('/').count()<=40 &&
        s.split('/').all(|x|!x.is_empty() && x!="." && x!=".." && !x.starts_with('~'))))
}
fn is_private_file(p:&Path)->Result<(),String> {
    let metadata=fs::symlink_metadata(p).map_err(|_|"File di configurazione cloud mancante")?;
    if !metadata.file_type().is_file() || metadata.len()>65536 {
        return Err("Configurazione cloud non è un file regolare".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("Impostare permessi 0600 sui segreti cloud".into());
        }
    }
    Ok(())
}
pub fn read_registry(path:&Path)->Result<(PathBuf,Vec<Profile>),String> {
    is_private_file(path)?;
    let value:Value=serde_json::from_slice(&fs::read(path).map_err(|_|"Profilo illeggibile")?)
        .map_err(|_|"Registro cloud JSON non valido")?;
    let config=value.get("rclone_config").and_then(Value::as_str)
        .ok_or("Percorso configurazione rclone assente")?;
    let rclone_config=PathBuf::from(config);
    if !rclone_config.is_absolute() {return Err("Config rclone deve essere assoluta".into());}
    is_private_file(&rclone_config)?;
    let arr=value.get("profiles").and_then(Value::as_array)
        .ok_or("Nessun profilo cloud")?;
    if arr.len()>32 {return Err("Troppi profili cloud".into());}
    let mut used=HashSet::new();
    let mut profiles=Vec::new();
    for v in arr{
        let get=|s:&str|v.get(s).and_then(Value::as_str).unwrap_or("");
        let id=get("id");let remote=get("remote");let provider=get("provider");
        let prefix=get("prefix");
        if !name(id) || !name(remote) || !relative(prefix) ||
           !matches!(provider,"dropbox"|"google-drive"|"onedrive"|"owncloud"|
               "nextcloud"|"icloud"|"s3"|"webdav"|"other") || !used.insert(id.to_owned()) {
            return Err("Profilo non autorizzato o nome duplicato".into());
        }
        let flag=|key:&str|->Result<bool,String>{
            match v.get(key){None=>Ok(false),Some(Value::Bool(b))=>Ok(*b),
                _=>Err("Flag di scrittura cloud non booleano".into())}
        };
        let upload=flag("allow_upload")?;
        let delete=flag("allow_delete")?;
        if provider=="icloud" && (upload || delete) {
            return Err("rclone iCloud remoto deve restare in sola lettura".into());
        }
        profiles.push(Profile{id:id.into(),provider:provider.into(),remote:remote.into(),
            prefix:prefix.into(),allow_upload:upload,allow_delete:delete});
    }
    Ok((rclone_config,profiles))
}
fn mount_target(target:&str)->Result<String,String> {
    #[cfg(windows)]{
        let b=target.as_bytes();
        if b.len()==2 && b[0].is_ascii_alphabetic() && b[1]==b':' {
            return Ok(target.into());
        }
    }
    let path=Path::new(target);
    if !path.is_absolute() || path.is_symlink() || !path.is_dir(){
        return Err("Mountpoint deve esistere come directory reale e assoluta".into());
    }
    if fs::read_dir(path).map_err(|_|"Mountpoint illeggibile")?.next().is_some(){
        return Err("Mountpoint non vuoto".into());
    }
    Ok(path.canonicalize().map_err(|_|"Mountpoint non raggiungibile")?
        .to_string_lossy().into_owned())
}
pub fn mount_command(registry:&Path,id:&str,target:&str,executable:&Path)
    ->Result<Command,String>{
    if !executable.is_absolute() || !executable.is_file() || executable.is_symlink(){
        return Err("Specificare percorso assoluto al binario rclone verificato".into());
    }
    let (cfg,profiles)=read_registry(registry)?;
    let p=profiles.iter().find(|p|p.id==id)
        .ok_or("Profilo cloud non autorizzato")?;
    let mount=mount_target(target)?;
    let mut cmd=Command::new(executable);
    cmd.arg("--config").arg(cfg).arg("--ask-password=false");
    cmd.arg("mount").arg(format!("{}:{}",p.remote,p.prefix)).arg(mount)
        .args(["--vfs-cache-mode","full","--vfs-cache-max-size","2G",
               "--dir-cache-time","30s","--poll-interval","1m"]);
    if p.provider=="icloud" || !p.allow_upload {
        cmd.arg("--read-only");
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    Ok(cmd)
}
pub fn spawn(registry:&Path,id:&str,target:&str,executable:&Path)->Result<Child,String>{
    mount_command(registry,id,target,executable)?.spawn()
        .map_err(|_|"rclone mount non avviato: verificare FUSE/WinFsp e credenziali".into())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn no_implicit_dynamic_remote_or_traversal(){
        for bad in ["../home","a//b","/absolute","a/./b","a\\b","other:remote","~root"]{
            assert!(!relative(bad));
        }
        assert!(relative("Team/Backups/2026"));
        assert!(!name(":local:"));
        assert!(!name("--config=/etc"));
        assert!(name("nextcloud-team"));
    }
    #[test] fn missing_config_denied_before_fuse_launch(){
        assert!(read_registry(Path::new("/unlikely/multi-backup-r218-cloud-missing.json")).is_err());
    }
}
