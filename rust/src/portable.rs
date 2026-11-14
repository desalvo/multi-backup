//! ZIP64 portable transport of *encrypted* objects for one logical backup.
//! All imports are transactional and preserve destination data.
use crate::chunk_store::{age, dir, safe_hash, Store};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::{self,File,OpenOptions},io::{Read,Write},
 path::{Path,PathBuf}};
use zip::{ZipWriter,ZipArchive,CompressionMethod,write::FileOptions};
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;
pub const DEFAULT_VOLUME_MIB:u64=2048;
#[derive(Clone,Serialize,Deserialize)]
struct Member {path:String,size:u64,sha256:String}
#[derive(Clone,Serialize,Deserialize)]
struct Volume {name:String,size:u64,sha256:String,members:Vec<Member>}
#[derive(Clone,Serialize,Deserialize)]
struct Catalog {version:u32,backup_id:String,volumes:Vec<Volume>}
fn hash_file(path:&Path)->Res<(u64,String)> {
    let mut reader=File::open(path)?;
    let mut digest=Sha256::new();
    let mut count=0u64;
    let mut buf=[0u8;65536];
    loop {
        let n=reader.read(&mut buf)?;
        if n==0 {break;}
        count=count.checked_add(n as u64).ok_or("Too large")?;
        digest.update(&buf[..n]);
    }
    Ok((count,hex::encode(digest.finalize())))
}
fn regular(path:&Path)->Res<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err("Symlink or nonregular archive member".into());
    }
    Ok(())
}
fn safe_object(path:&str)->bool {
    if path.is_empty()||path.len()>240||path.starts_with('/')||path.contains('\\')||
       path.split('/').any(|c|c.is_empty()||c=="."||c==".."){return false;}
    let s:Vec<_>=path.split('/').collect();
    match s.as_slice(){
        ["config.json.age"]=>true,
        ["manifests",n]|["stripes",n]=>n.strip_suffix(".json.age").is_some_and(safe_hash),
        ["chunks",a,b,n]=>{
            n.strip_suffix(".age").is_some_and(|id|
                safe_hash(id)&&*a==&id[..2]&&*b==&id[2..4])
        }
        ["parity",a,id,n]=>safe_hash(id)&&*a==&id[..2]&&
            (*n=="0.age"||*n=="1.age"),
        _=>false,
    }
}
pub fn export(store:&Store,key:&Path,id:&str,destination:&Path,volume_mib:u64)
 ->Res<Vec<PathBuf>> {
    if destination.exists()||!safe_hash(id)||!(1..=65536).contains(&volume_mib) {
        return Err("Destination must be new; invalid backup ID or volume size".into());
    }
    if !store.verify(key,id)?.healthy {return Err("Backup integrity verification failed".into());}
    let objects=store.portable_objects(key,id)?;
    let limit=volume_mib.checked_mul(1024*1024).ok_or("Invalid volume size")?;
    let mut groups:Vec<Vec<(Member,PathBuf)>>=vec![Vec::new()];
    let mut usage=0u64;
    for (path,source) in objects {
        if !safe_object(&path) {return Err("Invalid object path".into());}
        regular(&source)?;
        let (size,sha256)=hash_file(&source)?;
        if size.checked_add(65536).ok_or("Object too large")?>limit {
            return Err("Encrypted object too large for requested ZIP volume".into());
        }
        if usage>0 && usage.checked_add(size+65536).ok_or("Volume overflow")?>limit {
            groups.push(Vec::new());usage=0;
        }
        groups.last_mut().ok_or("Empty group")?.push((Member{path,size,sha256},source));
        usage+=size+1024;
    }
    if groups.iter().any(Vec::is_empty) {return Err("No objects to export".into());}
    let parent=destination.parent().ok_or("Missing destination parent")?;
    if !parent.is_dir(){return Err("Destination parent missing".into());}
    let stage=tempfile::Builder::new().prefix(".multi-backup-zip-").tempdir_in(parent)?;
    let mut volumes=Vec::new();
    for (n,group) in groups.iter().enumerate(){
        let name=format!("part-{:06}.zip",n+1);
        let path=stage.path().join(&name);
        let output=OpenOptions::new().create_new(true).write(true).open(&path)?;
        let mut zip=ZipWriter::new(output);
        let opts=FileOptions::default()
            .compression_method(CompressionMethod::Stored).large_file(true);
        for (member,source) in group {
            zip.start_file(format!("payload/{}",member.path),opts)?;
            let written=std::io::copy(&mut File::open(source)?,&mut zip)?;
            if written!=member.size {return Err("Source mutated during export".into());}
        }
        zip.finish()?.sync_all()?;
        let (size,sha256)=hash_file(&path)?;
        if size>limit {return Err("ZIP exceeds selected volume size".into());}
        volumes.push(Volume{name,size,sha256,
            members:group.iter().map(|(v,_)|v.clone()).collect()});
    }
    let catalog=Catalog{version:1,backup_id:id.into(),volumes};
    let index=age(&serde_json::to_vec(&catalog)?,Some(store.recipient()),None)?;
    let mut file=OpenOptions::new().write(true).create_new(true)
        .open(stage.path().join("bundle.index.age"))?;
    file.write_all(&index)?;file.sync_all()?;
    fs::rename(stage.path(),destination)?;
    Ok(catalog.volumes.iter().map(|v|destination.join(&v.name)).collect())
}
/// Import validates encrypted index, all volume SHA256s and every member,
/// followed by age authenticated chunk + parity verification. Never overwrite.
pub fn import(package:&Path,key:&Path,destination:&Path)->Res<String>{
    if destination.exists(){return Err("Import destination already exists".into());}
    if !fs::symlink_metadata(package)?.file_type().is_dir(){
        return Err("Package path not a real directory".into());
    }
    let encrypted=package.join("bundle.index.age");
    regular(&encrypted)?;
    if fs::metadata(&encrypted)?.len()>64*1024*1024 {
        return Err("Oversized bundle index".into());
    }
    let decoded=age(&fs::read(encrypted)?,None,Some(key))?;
    let index:Catalog=serde_json::from_slice(&decoded)?;
    if index.version!=1||!safe_hash(&index.backup_id)||
       index.volumes.is_empty()||index.volumes.len()>100000{
        return Err("Invalid portable index format".into());
    }
    let parent=destination.parent().ok_or("Missing target parent")?;
    if !parent.is_dir(){return Err("Destination parent missing".into());}
    let stage=tempfile::Builder::new().prefix(".multi-backup-import-").tempdir_in(parent)?;
    let mut seen=HashSet::new();
    for (n,volume) in index.volumes.iter().enumerate() {
        if volume.name!=format!("part-{:06}.zip",n+1)||
           !safe_hash(&volume.sha256)||volume.members.is_empty(){
            return Err("Invalid volume inventory".into());
        }
        let part=package.join(&volume.name);
        regular(&part)?;
        let (size,sha256)=hash_file(&part)?;
        if size!=volume.size||sha256!=volume.sha256 {
            return Err("ZIP volume checksum mismatch".into());
        }
        let mut zip=ZipArchive::new(File::open(part)?)?;
        if zip.len()!=volume.members.len(){return Err("Unexpected ZIP members".into());}
        for (i,member) in volume.members.iter().enumerate(){
            if !safe_object(&member.path)||!safe_hash(&member.sha256)||
               !seen.insert(member.path.clone()){return Err("Invalid ZIP object name".into());}
            let mut zipped=zip.by_index(i)?;
            if zipped.name()!=format!("payload/{}",member.path)||
               zipped.is_dir()||zipped.size()!=member.size{
                return Err("ZIP member mismatch".into());
            }
            let target=stage.path().join(".multi-backup-v1").join(&member.path);
            dir(target.parent().ok_or("Missing target parent")?)?;
            let mut out=OpenOptions::new().write(true).create_new(true).open(&target)?;
            let bytes=std::io::copy(&mut zipped.take(member.size+1),&mut out)?;
            if bytes!=member.size {return Err("ZIP object size mismatch".into());}
            out.sync_all()?;
            let (size,sha)=hash_file(&target)?;
            if size!=member.size||sha!=member.sha256 {
                return Err("ZIP member SHA256 mismatch".into());
            }
        }
    }
    let imported=Store::open(stage.path(),key)?;
    let report=imported.verify(key,&index.backup_id)?;
    if !report.healthy {return Err("Imported backup failed authentication".into());}
    let required:HashSet<_>=imported.portable_objects(key,&index.backup_id)?
        .into_keys().collect();
    if seen!=required {return Err("Missing or unexpected objects in bundle".into());}
    fs::rename(stage.path(),destination)?;
    Ok(index.backup_id)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rejects_file_path_attacks(){
        let hash="a".repeat(64);
        assert!(safe_object(&format!("manifests/{hash}.json.age")));
        for bad in ["/etc/passwd","../passwd","some/../../bad","chunk.tar.gz",
                    "chunks/ab/cd/foo.age","config.json.age/x"]{
            assert!(!safe_object(bad),"{bad}");
        }
    }
}
