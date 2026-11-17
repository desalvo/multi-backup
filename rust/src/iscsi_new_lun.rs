//! Provision, populate and verify one NEW LOCAL TGT iSCSI LUN, never an
//! existing SAN target. Dedicated Linux lab profile: loopback only, root,
//! deterministic ownership (new backing file + new IQN/TID + target listing).
//! Not a general remote SAN or CHAP/multipath implementation.
use crate::chunk_store::{BackupKind,Store};
use serde::Serialize;
use sha2::{Digest,Sha256};
use std::{fs::{self,OpenOptions},io::{Read,Write,Seek,SeekFrom},
          os::unix::fs::{FileTypeExt,OpenOptionsExt},
          path::Path,process::{Command,Stdio},thread,time::Duration};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Serialize)]
pub struct Receipt{
    pub provider:&'static str,pub backup_id:String,
    pub target_iqn:String,pub target_id:u32,pub lun:u32,
    pub new_backing_file:String,pub device:String,
    pub bytes:u64,pub sha256:String,
    pub created_new_lun:bool,pub verified_remote_block_readback:bool,
    pub remote_san_certified:bool
}
fn run(bin:&str,args:&[&str])->Result<String>{
    let output=Command::new(bin).args(args).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
    if !output.status.success()||output.stdout.len()>65536{
        return Err(format!("{bin} refused new iSCSI target operation").into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn check_id(id:&str,tid:u32)->Result<()>{
    if tid<10000||tid>1_000_000||!id.starts_with("iqn.")||
       id.len()>220||id.len()<12||
       !id.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'-'|b'_'|b':'))||
       id.contains(".."){
        return Err("New iSCSI LUN requires unique strict IQN and high dedicated target ID".into());
    }
    Ok(())
}
struct TargetGuard{tid:u32,iqn:String,active:bool}
impl Drop for TargetGuard{
    fn drop(&mut self){
        if self.active{
            let _=run("iscsiadm",&["-m","node","-T",&self.iqn,
                "-p","127.0.0.1:3260","--logout"]);
            let _=run("tgtadm",&["--lld","iscsi","--mode","target","--op",
                "delete","--tid",&self.tid.to_string()]);
        }
    }
}
pub fn create_and_restore(root:&Path,identity:&Path,id:&str,backing_dir:&Path,
                          iqn:&str,tid:u32)->Result<Receipt>{
    check_id(iqn,tid)?;
    if unsafe{libc::geteuid()}!=0{
        return Err("Dedicated local iSCSI TGT provisioning requires root".into());
    }
    let root=fs::canonicalize(root)?;
    let backing_dir=fs::canonicalize(backing_dir)?;
    if !backing_dir.is_dir()||backing_dir.starts_with(&root)||
       root.starts_with(&backing_dir)||identity.starts_with(&backing_dir){
        return Err("New target backing directory must be isolated from the encrypted store".into());
    }
    if !fs::symlink_metadata(&backing_dir)?.file_type().is_dir(){
        return Err("Unsafe iSCSI target backing path".into());
    }
    let store=Store::open(&root,identity)?;
    if !store.verify(identity,id)?.healthy{
        return Err("Cannot provision iSCSI target from invalid encrypted backup".into());
    }
    let source=store.load(identity,id)?;
    if source.kind!=BackupKind::Full||source.parent_id.is_some()||
       !source.top_namespace.is_empty()||source.entries.len()!=1{
        return Err("New iSCSI LUN requires exactly one full verified image".into());
    }
    let entry=&source.entries[0];
    let parts:Vec<_>=entry.path.split('/').collect();
    if parts.len()!=3||parts[0]!="iscsi"||
       parts[1].len()<8||parts[1].len()>512||
       !parts[1].bytes().all(|b|b.is_ascii_hexdigit())||
       !parts[2].starts_with("lun_")||!parts[2].ends_with(".raw")||
       entry.bytes==0||entry.bytes%512!=0 {
        return Err("Backup manifest does not represent a compatible aligned iSCSI LUN".into());
    }
    let source_iqn=String::from_utf8(hex::decode(parts[1])?)?;
    if !source_iqn.starts_with("iqn."){
        return Err("Invalid original iSCSI source IQN".into());
    }
    // No pre-existing backing file or target is accepted, even if empty.
    let base_name=format!("mb-new-lun-{tid}.img");
    let backing=backing_dir.join(base_name);
    let base=backing.to_str().ok_or("Invalid backing file path")?;
    let target_id=tid.to_string();
    let _unique_file=OpenOptions::new().write(true).create_new(true)
        .mode(0o600).open(&backing)?;
    _unique_file.set_len(entry.bytes)?;
    _unique_file.sync_all()?;
    // If a target already uses this TID, fail closed; never modify it.
    if let Err(e)=run("tgtadm",&["--lld","iscsi","--mode","target","--op","new",
                                "--tid",&target_id,"--targetname",iqn]){
        let _=fs::remove_file(&backing);
        return Err(e);
    }
    let mut guard=TargetGuard{tid,iqn:iqn.to_owned(),active:true};
    let provision=(||->Result<Receipt>{
        run("tgtadm",&["--lld","iscsi","--mode","logicalunit","--op",
            "new","--tid",&target_id,"--lun","1","-b",base])?;
        run("tgtadm",&["--lld","iscsi","--mode","target","--op","bind",
            "--tid",&target_id,"-I","127.0.0.1"])?;
        let listing=run("tgtadm",&["--lld","iscsi","--mode","target",
            "--op","show","--tid",&target_id])?;
        if !listing.contains(iqn)||!listing.contains(base){
            return Err("TGT did not confirm exact NEW IQN and private backing file".into());
        }
        run("iscsiadm",&["-m","discovery","-t","sendtargets",
            "-p","127.0.0.1:3260"])?;
        run("iscsiadm",&["-m","node","-T",iqn,"-p","127.0.0.1:3260","--login"])?;
        run("udevadm",&["settle"])?;
        let expected=format!("/dev/disk/by-path/ip-127.0.0.1:3260-iscsi-{iqn}-lun-1");
        let link=Path::new(&expected);
        let mut device=None;
        for _ in 0..35{
            if let Ok(meta)=fs::symlink_metadata(link){
                if meta.file_type().is_symlink(){
                    device=Some(fs::canonicalize(link)?);
                    break;
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
        let device=device.ok_or("New TGT iSCSI LUN did not appear in kernel by-path")?;
        if !device.starts_with("/dev/")||
           !fs::symlink_metadata(&device)?.file_type().is_block_device(){
            return Err("New target did not resolve to a genuine block device".into());
        }
        let dev=device.to_str().ok_or("Non-UTF8 new block device")?;
        if run("blockdev",&["--getro",dev])?.trim()!="0" ||
           run("blockdev",&["--getsize64",dev])?.trim()!=entry.bytes.to_string(){
            return Err("New iSCSI target size/RO bit differs from requested image".into());
        }
        let properties=run("udevadm",&["info","--query=property","--name",dev])?;
        let link_name=link.file_name().and_then(|x|x.to_str()).ok_or("Invalid by-path")?;
        if !properties.lines().any(|s|s==format!("ID_PATH={link_name}")){
            return Err("New iSCSI initiator kernel device ID_PATH mismatch".into());
        }
        // Refuse a device already claimed by a filesystem/other block owner.
        let mounts=Command::new("findmnt").args(["-rn","-S",dev])
            .stdout(Stdio::piped()).stderr(Stdio::null()).output()?;
        if !mounts.stdout.is_empty(){
            return Err("New iSCSI target has a mounted filesystem".into());
        }
        let mut output=OpenOptions::new().write(true).custom_flags(libc::O_EXCL)
            .open(&device)?;
        output.seek(SeekFrom::Start(0))?;
        let mut hash=Sha256::new();let mut bytes=0u64;
        for c in &entry.chunks{
            let data=store.read_chunk_ref(identity,c)?;
            output.write_all(&data)?;
            hash.update(&data);
            bytes=bytes.checked_add(data.len() as u64).ok_or("iSCSI restore size overflow")?;
        }
        output.sync_all()?;drop(output);
        if bytes!=entry.bytes{return Err("New LUN write incomplete".into());}
        let expected_sha=hex::encode(hash.finalize());
        let mut input=OpenOptions::new().read(true).open(&device)?;
        let mut observed=Sha256::new();let mut read_bytes=0u64;
        let mut buf=[0u8;131072];
        loop{
            let n=input.read(&mut buf)?;
            if n==0{break;}
            read_bytes=read_bytes.checked_add(n as u64)
                .ok_or("iSCSI readback overflow")?;
            if read_bytes>bytes{return Err("New LUN readback longer than source".into());}
            observed.update(&buf[..n]);
        }
        if read_bytes!=bytes||hex::encode(observed.finalize())!=expected_sha{
            return Err("NEW iSCSI target failed independent block-device readback".into());
        }
        Ok(Receipt{provider:"iscsi",backup_id:id.into(),target_iqn:iqn.into(),
            target_id:tid,lun:1,new_backing_file:base.into(),device:expected,
            bytes,sha256:expected_sha,created_new_lun:true,
            verified_remote_block_readback:true,remote_san_certified:false})
    })();
    if provision.is_ok(){guard.active=false;}
    // On failed new-target creation do not auto-delete private backing file
    // until operator inspects possible partial content. Never touch other LUNs.
    provision
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn rejected_iqn_or_low_tid(){
        assert!(check_id("iqn.2026-11.test:mb",10001).is_ok());
        assert!(check_id("iqn.bad/../../root",10001).is_err());
        assert!(check_id("iqn.2026-11.test:mb",1).is_err());
    }
    #[test]fn cannot_provision_without_key(){
        assert!(create_and_restore(Path::new("/none"),Path::new("/none"),
            "invalid",Path::new("/tmp"),"bad",2).is_err());
    }
}
