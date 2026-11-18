//! One shared rclone-backed cloud-drive filesystem facade.
//! Provider permissions are *separate* from backup plugin qualifications.
//! No rclone RC daemon, shell expansion, arbitrary backend names or raw secrets.
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn error(message: &'static str)->Box<dyn std::error::Error+Send+Sync>{message.into()}
#[derive(Clone, Copy)]
struct Provider { id:&'static str, kind:&'static str, vendor:Option<&'static str> }
const PROVIDERS:&[Provider]=&[
    Provider{id:"dropbox",kind:"dropbox",vendor:None},
    Provider{id:"google-drive",kind:"drive",vendor:None},
    Provider{id:"icloud",kind:"iclouddrive",vendor:None},
    Provider{id:"onedrive",kind:"onedrive",vendor:None},
    Provider{id:"owncloud",kind:"webdav",vendor:Some("owncloud")},
    Provider{id:"nextcloud",kind:"webdav",vendor:Some("nextcloud")},
    Provider{id:"box",kind:"box",vendor:None},
];
fn provider(id:&str)->Result<Provider> {
    PROVIDERS.iter().find(|p|p.id==id).copied().ok_or_else(||error("Unapproved cloud provider"))
}
pub fn valid_path(path:&str,allow_root:bool)->Result<()>{
    if path.is_empty(){return if allow_root{Ok(())}else{Err(error("Expected exact file path"))}}
    if path.len()>768 || path.starts_with('/') || path.ends_with('/') ||
        path.contains('\\') || path.chars().any(char::is_control) {
        return Err(error("Unsafe cloud drive path"));
    }
    if path.split('/').any(|component| component.is_empty() ||
        component=="." || component==".." || component.contains(':') ||
        component.starts_with('-') || component.starts_with('~')) {
        return Err(error("Cloud drive path traversal or ambiguous component"));
    }
    Ok(())
}
fn check_config(config:&Path, id:&str)->Result<()> {
    let p=provider(id)?;
    let meta=fs::symlink_metadata(config)?;
    if !meta.file_type().is_file() || meta.len()>131072 {
        return Err(error("Rclone config must be an ordinary private file <=128KiB"));
    }
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {return Err(error("Rclone config must be chmod 0600"))}
    }
    let text=fs::read_to_string(config)?;
    let mut in_section=false;
    let (mut typ,mut vendor,mut service)=(String::new(),String::new(),String::new());
    for line in text.lines(){
        let entry=line.trim();
        if entry.starts_with('[')&&entry.ends_with(']'){
            in_section=&entry[1..entry.len()-1]==id;
        }else if in_section {
            if let Some((key,value))=entry.split_once('='){
                match key.trim(){
                    "type"=>typ=value.trim().to_owned(),
                    "vendor"=>vendor=value.trim().to_owned(),
                    "service"=>service=value.trim().to_owned(),
                    _=>{}
                }
            }
        }
    }
    if typ!=p.kind || p.vendor.is_some_and(|v|vendor!=v) ||
        (id=="icloud" && service!="" && service!="drive") {
        return Err(error("Rclone remote type/vendor/service does not match approved cloud provider"));
    }
    Ok(())
}
fn remote(id:&str,path:&str)->String{
    if path.is_empty(){format!("{id}:")}else{format!("{id}:{path}")}
}
fn command(config:&Path, args:&[&str])->Command{
    let mut command=Command::new("rclone");
    command.arg("--config").arg(config)
        .args(["--retries","1","--disable-http2"])
        .args(args).stdin(Stdio::null()).stderr(Stdio::null());
    command
}
fn collect_limited(mut stream:impl Read,cap:usize)->Result<Vec<u8>>{
    let mut data=Vec::new();
    // Cap untrusted remote data, not just decoded JSON entries.
    (&mut stream).take((cap as u64)+1).read_to_end(&mut data)?;
    if data.len()>cap {return Err(error("Cloud response exceeds the configured byte limit"))}
    Ok(data)
}
fn run_output(config:&Path,args:&[&str],cap:usize)->Result<Vec<u8>>{
    let mut child=command(config,args).stdout(Stdio::piped()).spawn()?;
    let stdout=child.stdout.take().ok_or_else(||error("Missing cloud transport output"))?;
    let bytes=collect_limited(stdout,cap);
    if bytes.is_err(){let _=child.kill();}
    if !child.wait()?.success(){return Err(error("Cloud remote command failed"))}
    bytes
}
pub fn configured(config:&Path)->Result<Vec<String>> {
    let mut result=Vec::new();
    for p in PROVIDERS {
        if check_config(config,p.id).is_ok(){result.push(p.id.to_string());}
    }
    Ok(result)
}
pub fn list(config:&Path,id:&str,directory:&str)->Result<Value>{
    check_config(config,id)?;
    valid_path(directory,true)?;
    let address=remote(id,directory);
    let result=run_output(config,&["lsjson","--max-depth","1",&address],2*1024*1024)?;
    let data:Value=serde_json::from_slice(&result)?;
    let entries=data.as_array().ok_or_else(||error("Malformed cloud directory listing"))?;
    if entries.len()>2000{return Err(error("Cloud directory has more than 2000 entries"))}
    let mut sanitized=Vec::new();
    for value in entries {
        let name=value["Name"].as_str().ok_or_else(||error("Unnamed cloud object"))?;
        valid_path(name,false)?;
        if name.contains('/') {return Err(error("Unexpected nested cloud listing entry"))}
        sanitized.push(json!({"name":name,"is_dir":value["IsDir"]==true,
          "size":value["Size"].as_u64(),"modified":value["ModTime"].as_str()}));
    }
    Ok(json!({"provider":id,"directory":directory,"entries":sanitized}))
}
pub fn read(config:&Path,id:&str,path:&str,max_bytes:usize)->Result<Vec<u8>>{
    check_config(config,id)?;
    valid_path(path,false)?;
    if max_bytes==0||max_bytes>64*1024*1024{return Err(error("Read limit must be 1..64 MiB"))}
    run_output(config,&["cat",&remote(id,path)],max_bytes)
}
pub fn write_new(config:&Path,id:&str,directory:&str,filename:&str,bytes:&[u8])->Result<Value>{
    check_config(config,id)?;
    valid_path(directory,true)?;
    valid_path(filename,false)?;
    if filename.contains('/') {return Err(error("Upload expects a single filename"))}
    if bytes.is_empty()||bytes.len()>8*1024*1024{
        return Err(error("Cloud GUI upload accepts 1..8 MiB in this preview"));
    }
    let mut entropy=[0u8;16];OsRng.fill_bytes(&mut entropy);
    let new_name=format!("mb-new-{}-{}",hex::encode(entropy),filename);
    let full=if directory.is_empty(){new_name.clone()}
        else{format!("{directory}/{new_name}")};
    let address=remote(id,&full);
    // Global ignore-existing is safer than default overwrite but does not
    // guarantee atomic create-if-absent on every cloud backend.
    let mut child=command(config,&["rcat","--ignore-existing",&address])
        .stdin(Stdio::piped()).stdout(Stdio::null()).spawn()?;
    let mut pipe=child.stdin.take().ok_or_else(||error("Cloud writer stdin missing"))?;
    let written=pipe.write_all(bytes);
    drop(pipe);
    if written.is_err(){let _=child.kill();}
    if !child.wait()?.success(){return Err(error("Cloud upload failed"))}
    written?;
    let remote_bytes=read(config,id,&full,8*1024*1024)?;
    let hash=hex::encode(Sha256::digest(bytes));
    if hex::encode(Sha256::digest(&remote_bytes))!=hash {
        return Err(error("Cloud readback SHA256 verification failed"));
    }
    Ok(json!({"provider":id,"new_path":full,"sha256":hash,"bytes":bytes.len(),
              "verified":true,"atomic_create_guaranteed":false}))
}
pub fn delete_file(config:&Path,id:&str,path:&str,
                   pinned_sha256:&str,confirmation:&str)->Result<Value>{
    check_config(config,id)?;
    valid_path(path,false)?;
    if pinned_sha256.len()!=64 ||
        !pinned_sha256.bytes().all(|x|x.is_ascii_hexdigit()){
        return Err(error("Delete requires independently captured SHA256"));
    }
    if confirmation!=format!("DELETE {id}:{path}"){
        return Err(error("Delete exact-object confirmation mismatch"));
    }
    // Preflight; not an atomic compare-and-delete. The server may enable this
    // only under an additional independent offline approval policy.
    let raw=read(config,id,path,64*1024*1024)?;
    if hex::encode(Sha256::digest(&raw))!=pinned_sha256.to_ascii_lowercase(){
        return Err(error("Remote object changed since delete confirmation"));
    }
    let output=command(config,&["deletefile",&remote(id,path)])
        .stdout(Stdio::null()).status()?;
    if !output.success(){return Err(error("Remote deletefile failed"))}
    Ok(json!({"provider":id,"path":path,"deleted":true,
              "atomic_version_guard":false}))
}

/// Streaming create-only remote read; no complete cloud object in RAM.
pub fn download_to_new(config:&Path,id:&str,path:&str,destination:&Path,
                       max_bytes:u64)->Result<(u64,String)>{
    check_config(config,id)?;
    valid_path(path,false)?;
    if max_bytes==0||max_bytes>32*1024*1024*1024 {
        return Err(error("Remote recovery cap must be 1 byte..32 GiB"));
    }
    let parent=destination.parent().ok_or_else(||error("Missing recovery parent"))?;
    if !fs::symlink_metadata(parent)?.file_type().is_dir()||
       fs::symlink_metadata(destination).is_ok() {
        return Err(error("Recovery destination must be new in an ordinary directory"));
    }
    let mut stage=tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        stage.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let remote_path=remote(id,path);
    let mut child=command(config,&["cat",&remote_path]).stdout(Stdio::piped()).spawn()?;
    let stdout=child.stdout.take().ok_or_else(||error("Cloud stdout missing"))?;
    let copied=std::io::copy(&mut stdout.take(max_bytes+1),stage.as_file_mut());
    let count=match copied {
        Ok(n) if n<=max_bytes=>n,
        _=>{
            let _=child.kill();let _=child.wait();
            return Err(error("Remote recovery failed or exceeded transfer cap"));
        }
    };
    if !child.wait()?.success(){return Err(error("Cloud transport exited nonzero"));}
    stage.as_file().sync_all()?;
    let mut hasher=Sha256::new();
    let mut source=File::open(stage.path())?;
    let mut buffer=[0u8;65536];
    loop {
        let n=source.read(&mut buffer)?;
        if n==0 {break;}
        hasher.update(&buffer[..n]);
    }
    let digest=hex::encode(hasher.finalize());
    stage.persist_noclobber(destination).map_err(|e|e.error)?;
    Ok((count,digest))
}

pub fn mount_foreground(config:&Path,id:&str,mountpoint:&Path,read_write:bool)->Result<()>{
    check_config(config,id)?;
    let meta=fs::symlink_metadata(mountpoint)?;
    if !meta.is_dir() || !mountpoint.read_dir()?.next().is_none(){
        return Err(error("Mountpoint must be an existing empty ordinary directory"));
    }
    let path=mountpoint.canonicalize()?;
    if path.parent().is_none() {return Err(error("Cannot mount over filesystem root"))}
    let local=path.to_str().ok_or_else(||error("Mountpoint path must be UTF-8"))?;
    let base=remote(id,"");
    let mut cmd=command(config,&["mount",&base,local,
                 "--vfs-cache-mode","full","--vfs-cache-max-size","1G",
                 "--dir-cache-time","1m"]);
    if !read_write{cmd.arg("--read-only");}
    let status=cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).status()?;
    if !status.success(){return Err(error("Cloud mount failed; check OS FUSE/WinFsp and credentials"))}
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_unknown_backend_and_unsafe_names(){
        for s in ["/etc","../x","x/../y","x/./z","x//y","x\\test",
                  "x:/drive","-flag","~alice","test\nnext"]{
            assert!(valid_path(s,false).is_err(),"{s}");
        }
        assert!(valid_path("backup/2026/version-1.tar.gz.age",false).is_ok());
        assert!(provider("totally-unknown").is_err());
    }
    #[test]
    fn authentic_rclone_config_backend_is_mandatory(){
        let tmp=tempfile::tempdir().unwrap();
        let file=tmp.path().join("config.conf");
        fs::write(&file,"[dropbox]\ntype = local\n[owncloud]\ntype=webdav\nvendor=nextcloud\n").unwrap();
        #[cfg(unix)]{
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file,fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(check_config(&file,"dropbox").is_err());
        assert!(check_config(&file,"owncloud").is_err());
        fs::write(&file,"[dropbox]\ntype=dropbox\n[owncloud]\ntype=webdav\nvendor=owncloud\n").unwrap();
        assert!(check_config(&file,"dropbox").is_ok());
        assert!(check_config(&file,"owncloud").is_ok());
        assert!(check_config(&file,"nextcloud").is_err());
    }
    #[test]
    fn delete_confirmation_never_accepts_implicit_wildcards(){
        let path="Backup/important.age";
        assert_ne!(format!("DELETE icloud:{path}"),"DELETE icloud:Backup/*");
    }
}
