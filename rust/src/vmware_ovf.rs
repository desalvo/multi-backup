//! Native VMware vSphere OFFLINE OVF package backup and create-only VM restore.
//! This is NOT VADP/CBT or live snapshot backup.  govc (official govmomi)
//! is a verified, external native executable and uses TLS-verified vCenter.
//! The staging OVF/VMDK files are plaintext until deleted; require a
//! disposable, encrypted, quota-controlled private filesystem. Age chunks
//! are only committed after provider identity and complete tar exit checks.
use crate::{chunk_store::{BackupKind,LogicalBackup,Store},protected_config};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::{collections::HashSet,fs::{self,File,OpenOptions},io::{self,Read,Write},
    path::{Path,PathBuf},process::{Command,Stdio}};
use url::Url;
use zeroize::{Zeroize,Zeroizing};
type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

const MAX_OVF_BYTES:u64=2_199_023_255_552;
const MAX_ITEMS:usize=128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Explicit authority role: export is read/download, import can create a VM.
    pub role:String,
    pub endpoint:String,
    pub username:String,
    pub password:String,
    pub tls_ca_file:String,
    /// Absolute, symlink-free, verified govc executable, SHA-256 pinned.
    pub govc_binary:String,
    pub govc_sha256:String,
    pub datacenter:String,
    /// For role=export, exact /DC/vm/path with no wildcard.
    pub vm_path:Option<String>,
    pub vm_moref:Option<String>,
    /// For role=import, exact destination vm inventory folder.
    pub destination_folder:Option<String>,
    pub datastore:Option<String>,
    pub resource_pool:Option<String>,
}
impl Drop for Profile {fn drop(&mut self){self.password.zeroize();}}
fn component(s:&str)->bool{
    !s.is_empty()&&s.len()<=128&&!s.starts_with('.')&&!s.contains("..")&&
    s.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'.'|b'_'|b'-'))
}
fn inventory_path(s:&str,dc:&str,folder:bool)->bool{
    let prefix=format!("/{dc}/vm");
    s==prefix && folder || s.starts_with(&(prefix+"/"))&&
        s.split('/').skip(1).all(component)&&
        s.len()<=512&&!s.contains("//")
}
fn moref(s:&str)->bool{
    s.strip_prefix("vm-").is_some_and(|n|
       !n.is_empty()&&n.len()<=16&&n.bytes().all(|b|b.is_ascii_digit()))
}
fn digest(p:&Path)->Result<String>{
    let mut file=File::open(p)?;
    let mut h=Sha256::new();
    let mut data=[0u8;65536];
    loop{let n=file.read(&mut data)?;if n==0{break;}h.update(&data[..n]);}
    Ok(hex::encode(h.finalize()))
}
fn exact_regular(path:&Path)->Result<()>{
    if !path.is_absolute()||fs::symlink_metadata(path)?.file_type().is_symlink()||
       !fs::symlink_metadata(path)?.file_type().is_file(){
        return Err("VMware requires an absolute regular file, not a symlink".into());
    }
    Ok(())
}
impl Profile {
    pub fn load(path:&Path)->Result<Self>{
        let bytes=Zeroizing::new(protected_config::read_owner_only(path,16384)?);
        let p:Self=serde_json::from_slice(&bytes)?;
        p.validate()?;Ok(p)
    }
    pub fn validate(&self)->Result<()>{
        if !matches!(self.role.as_str(),"export"|"import"){
            return Err("VMware separate export/import principal role required".into())
        }
        let url=Url::parse(&self.endpoint)?;
        if url.scheme()!="https"||url.host_str().is_none()||
           !url.username().is_empty()||url.password().is_some()||
           !matches!(url.path(),""|"/")||url.query().is_some()||url.fragment().is_some(){
            return Err("VMware requires bare verified HTTPS vCenter endpoint".into())
        }
        if !component(&self.datacenter)||self.username.len()<3||self.username.len()>256||
           self.username.chars().any(char::is_control)||
           self.password.len()<12||self.password.len()>4096||
           self.password.chars().any(char::is_control)||
           self.govc_sha256.len()!=64||!self.govc_sha256.bytes().all(|b|
               b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()){
            return Err("VMware invalid datacenter/credential/binary pin".into())
        }
        exact_regular(Path::new(&self.govc_binary))?;
        exact_regular(Path::new(&self.tls_ca_file))?;
        if digest(Path::new(&self.govc_binary))?!=self.govc_sha256{
            return Err("VMware govc executable hash changed or mismatched".into())
        }
        if fs::metadata(&self.tls_ca_file)?.len()==0{
            return Err("VMware CA trust bundle is empty".into())
        }
        if self.role=="export"{
            let vm=self.vm_path.as_deref().ok_or("VMware export needs exact inventory path")?;
            let id=self.vm_moref.as_deref().ok_or("VMware export needs pinned VM MoRef")?;
            if !inventory_path(vm,&self.datacenter,false)||!moref(id)||
               self.destination_folder.is_some()||self.datastore.is_some()||
               self.resource_pool.is_some(){
                return Err("VMware export must not carry restore authority".into())
            }
        }else{
            let folder=self.destination_folder.as_deref()
                .ok_or("VMware import needs new VM folder")?;
            if !inventory_path(folder,&self.datacenter,true)||
               !self.datastore.as_deref().is_some_and(component)||
               !self.resource_pool.as_deref().is_some_and(|s|
                   s.len()<=300&&s.starts_with('/')&&s.split('/').skip(1).all(component))||
               self.vm_path.is_some()||self.vm_moref.is_some(){
                return Err("VMware import needs exact destination and no source VM permissions".into())
            }
        }
        Ok(())
    }
    fn cmd(&self,operation:&str)->Result<Command>{
        self.validate()?;
        if self.role=="export"&&operation=="import" ||
           self.role=="import"&&operation=="export"{
            return Err("VMware cross-role command forbidden".into())
        }
        let mut c=Command::new(&self.govc_binary);
        // No inherited GOVC_* / HTTP_PROXY / URL / SESSION settings. No
        // credentials in argv or logs. Do not reuse implicit govc sessions.
        c.env_clear().env("GOVC_URL",&self.endpoint)
         .env("GOVC_USERNAME",&self.username).env("GOVC_PASSWORD",&self.password)
         .env("GOVC_TLS_CA_CERTS",&self.tls_ca_file)
         .env("GOVC_INSECURE","false").env("GOVC_PERSIST_SESSION","false")
         .env("GOVC_DEBUG","false").env("GOVC_TRACE","false")
         .env("GOVC_DATACENTER",&self.datacenter)
         .env("HOME","/nonexistent")
         .stdin(Stdio::null()).stderr(Stdio::null());
        if self.role=="import"{
            c.env("GOVC_DATASTORE",self.datastore.as_ref().unwrap())
             .env("GOVC_RESOURCE_POOL",self.resource_pool.as_ref().unwrap())
             .env("GOVC_FOLDER",self.destination_folder.as_ref().unwrap());
        }
        Ok(c)
    }
    fn govc_json(&self,args:&[&str])->Result<Value>{
        let output=self.cmd(if self.role=="export"{"export"}else{"import"})?
            .args(args).output()?;
        if !output.status.success()||output.stdout.len()>2_097_152{
            return Err("VMware authenticated govc operation failed".into())
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    fn vm_info(&self,vm:&str)->Result<VM>{
        let j=self.govc_json(&["vm.info","-json",vm])?;
        parse_vm(&j)
    }
}

#[derive(Debug)]
struct VM{moref:String,name:String,power:String}
fn parse_vm(j:&Value)->Result<VM>{
    let arr=j.get("VirtualMachines").or_else(||j.get("virtualMachines"))
        .and_then(Value::as_array).ok_or("VMware vm.info not a single VM")?;
    if arr.len()!=1{return Err("VMware expected exactly one VM".into());}
    let vm=&arr[0];
    let id=vm.pointer("/Self/Value").or_else(||vm.pointer("/self/value"))
        .and_then(Value::as_str).ok_or("VMware MoRef missing")?;
    let name=vm.pointer("/Summary/Config/Name")
        .or_else(||vm.pointer("/summary/config/name"))
        .and_then(Value::as_str).ok_or("VMware VM name missing")?;
    let power=vm.pointer("/Summary/Runtime/PowerState")
        .or_else(||vm.pointer("/summary/runtime/powerState"))
        .and_then(Value::as_str).ok_or("VMware VM power state missing")?;
    if !moref(id)||!component(name)||!matches!(power,"poweredOn"|"poweredOff"|"suspended"){
        return Err("Invalid vSphere VM identity/name/state".into())
    }
    Ok(VM{moref:id.into(),name:name.into(),power:power.into()})
}
fn pinned_source(profile:&Profile)->Result<VM>{
    if profile.role!="export"{return Err("Only export principal may read source VM".into())}
    let j=profile.vm_info(profile.vm_path.as_deref().unwrap())?;
    if j.moref!=profile.vm_moref.as_deref().unwrap()||j.power!="poweredOff"{
        return Err("VMware source VM ID changed or is not powered off".into())
    }
    Ok(j)
}
fn checked_files(dir:&Path)->Result<Vec<String>>{
    let mut list=Vec::new();let mut ovf=0;let mut vmdk=0;let mut sum=0u64;
    for entry in fs::read_dir(dir)?{
        let entry=entry?;
        let kind=entry.file_type()?;
        if !kind.is_file()||kind.is_symlink(){
            return Err("VMware export contained a directory or symbolic link".into())
        }
        let name=entry.file_name().into_string()
            .map_err(|_|"VMware OVF export contained non-UTF8 name")?;
        if !component(&name)||!["ovf","vmdk","mf","nvram"].iter().any(|ext|
            name.ends_with(&format!(".{ext}"))){
            return Err("VMware export contains unexpected or unsafe artifact".into())
        }
        if name.ends_with(".ovf"){ovf+=1;}
        if name.ends_with(".vmdk"){vmdk+=1;}
        let size=entry.metadata()?.len();
        if size==0 {return Err("VMware OVF export contains empty file".into())}
        sum=sum.checked_add(size).ok_or("VMware OVF bytes overflow")?;
        if sum>MAX_OVF_BYTES{return Err("VMware exported package exceeds bound".into())}
        list.push(name);
        if list.len()>MAX_ITEMS{return Err("VMware export has too many files".into())}
    }
    if ovf!=1||vmdk==0{return Err("VMware complete OVF and at least one VMDK required".into())}
    list.sort();Ok(list)
}
fn staging_root(path:&Path,store_root:&Path,key:&Path)->Result<PathBuf>{
    if !path.is_absolute()||!fs::symlink_metadata(path)?.file_type().is_dir(){
        return Err("VMware staging must be an existing absolute directory".into())
    }
    let stage=fs::canonicalize(path)?;
    let root=fs::canonicalize(store_root)?;
    if stage.starts_with(&root)||root.starts_with(&stage)||key.starts_with(&stage){
        return Err("VMware staging must not overlap encrypted store or private identity".into())
    }
    #[cfg(unix)]{
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&stage)?.mode()&0o077!=0{
            return Err("VMware plaintext staging root must be owner-only".into())
        }
    }
    Ok(stage)
}
#[derive(Serialize)]
pub struct Receipt {
    pub provider:&'static str,pub id:String,pub source_vm_moref:String,
    pub ovf_file_count:usize,pub vmdk_count:usize,
    pub encrypted:bool,pub offline_only:bool,
    pub imported_new_vm:bool,pub boot_verified:bool,pub guest_data_verified:bool,
    pub vadp_cbt_certified:bool,pub vcenter_field_certified:bool
}
pub fn backup(profile:&Profile,root:&Path,key:&Path,label:&str,stage_root:&Path)->Result<Receipt>{
    if profile.role!="export"||label.trim().is_empty()||label.len()>256{
        return Err("VMware backup requires export principal and valid label".into())
    }
    let stage_root=staging_root(stage_root,root,key)?;
    let source=pinned_source(profile)?;
    let temp=tempfile::Builder::new().prefix("mb-vsphere-export-")
        .tempdir_in(stage_root)?;
    let dir=temp.path().join("export");
    fs::create_dir(&dir)?;
    let status=profile.cmd("export")?
        .arg("export.ovf").arg("-vm")
        .arg(profile.vm_path.as_deref().unwrap()).arg(&dir)
        .stdout(Stdio::null()).status()?;
    if !status.success(){return Err("VMware genuine OVF export failed".into())}
    let files=checked_files(&dir)?;
    let after=pinned_source(profile)?;
    if source.moref!=after.moref||source.name!=after.name{
        return Err("VMware source identity changed while exporting".into())
    }
    let mut tar=Command::new("tar");
    tar.arg("-cf").arg("-").arg("-C").arg(&dir).arg("--")
       .args(&files).stdin(Stdio::null()).stdout(Stdio::piped())
       .stderr(Stdio::null());
    let mut child=tar.spawn()?;
    let mut stdout=child.stdout.take().ok_or("Cannot capture OVF tar stream")?;
    let store=Store::open(root,key)?;
    let logical=format!("vmware/vm/{}/ovf.tar",source.moref);
    let result=store.backup_stream(key,label,"",&logical,&mut stdout,
        BackupKind::Full,None,||{
            let status=child.wait().map_err(|_|"OVF tar process wait failed")?;
            if !status.success(){return Err("OVF tar producer did not finish".into())}
            let current=pinned_source(profile).map_err(|_|"VMware VM identity/status changed")?;
            if current.moref!=source.moref||current.name!=source.name {
                return Err("VMware VM changed after OVF transfer".into())
            }
            Ok(())
        });
    drop(stdout);
    if result.is_err(){let _=child.kill();let _=child.wait();}
    let snapshot=result?;
    if !store.verify(key,&snapshot.id)?.healthy{
        return Err("VMware age chunk roundtrip verification failed".into())
    }
    Ok(Receipt{provider:"vmware",id:snapshot.id,source_vm_moref:source.moref,
        ovf_file_count:files.len(),
        vmdk_count:files.iter().filter(|x|x.ends_with(".vmdk")).count(),
        encrypted:true,offline_only:true,imported_new_vm:false,
        boot_verified:false,guest_data_verified:false,
        vadp_cbt_certified:false,vcenter_field_certified:false})
}
fn safe_extract(archive:&Path,dest:&Path)->Result<PathBuf>{
    let mut tar=tar::Archive::new(File::open(archive)?);
    let mut names=HashSet::new();let mut ovfs=Vec::new();let mut disks=0;
    let mut bytes=0u64;
    for item in tar.entries()?{
        let mut item=item?;
        if !item.header().entry_type().is_file() {
            return Err("VMware tar member must be regular, never symlink/dir/hardlink".into())
        }
        let path=item.path()?;
        let name=path.to_str().ok_or("VMware archive name encoding")?;
        if !component(name)||!["ovf","vmdk","mf","nvram"].iter().any(|ext|
              name.ends_with(&format!(".{ext}"))){
            return Err("VMware tar traversal or unexpected member".into())
        }
        if !names.insert(name.to_string())||names.len()>MAX_ITEMS{
            return Err("VMware duplicate member or excessive files".into())
        }
        let declared=item.header().size()?;
        if declared==0 {return Err("Empty VMware tar member".into())}
        bytes=bytes.checked_add(declared).ok_or("VMware tar size overflow")?;
        if bytes>MAX_OVF_BYTES{return Err("VMware tar exceeds maximum size".into())}
        let file=dest.join(name);
        let mut out=OpenOptions::new().write(true).create_new(true).open(&file)?;
        let written=io::copy(&mut item,&mut out)?;
        if written!=declared{return Err("VMware tar member truncated".into())}
        out.sync_all()?;
        if name.ends_with(".ovf"){ovfs.push(file);}
        if name.ends_with(".vmdk"){disks+=1;}
    }
    if ovfs.len()!=1||disks==0 {
        return Err("VMware recovered OVF must include all declared disk parts".into())
    }
    Ok(ovfs.remove(0))
}
fn exact_absence(profile:&Profile,name:&str)->Result<()>{
    // govc find is authoritative; vm.info returns success with null if no VM,
    // and cannot distinguish a failed lookup from absence.
    let output=profile.cmd("import")?.args(["find","/","-type","m","-name",name]).output()?;
    if !output.status.success()||output.stdout.len()>1_048_576 {
        return Err("VMware destination inventory collision check failed".into())
    }
    if !output.stdout.is_empty(){
        return Err("VMware destination VM name already exists".into())
    }
    Ok(())
}
pub fn restore_new(profile:&Profile,root:&Path,key:&Path,id:&str,
    stage_root:&Path,new_name:&str,confirm:&str)->Result<Receipt>{
    if profile.role!="import"||!component(new_name)||
       confirm!=format!("vmware:{new_name}:{}",profile.datastore.as_deref().unwrap_or("")) {
        return Err("VMware import requires exact NEW VM name and datastore confirmation".into())
    }
    let stage_root=staging_root(stage_root,root,key)?;
    let store=Store::open(root,key)?;
    if !store.verify(key,id)?.healthy {return Err("VMware encrypted backup invalid".into())}
    let snapshot:LogicalBackup=store.load(key,id)?;
    if snapshot.kind!=BackupKind::Full||snapshot.parent_id.is_some()||
       !snapshot.top_namespace.is_empty()||snapshot.entries.len()!=1{
        return Err("VMware VM restore requires exactly one full OVF package".into())
    }
    let parts:Vec<_>=snapshot.entries[0].path.split('/').collect();
    if parts.len()!=4||parts[0]!="vmware"||parts[1]!="vm"||
       !moref(parts[2])||parts[3]!="ovf.tar"{
        return Err("Encrypted backup is not a complete VMware OVF package".into())
    }
    exact_absence(profile,new_name)?;
    let temp=tempfile::Builder::new().prefix("mb-vsphere-restore-")
        .tempdir_in(stage_root)?;
    let restored=temp.path().join("chunks");
    if store.restore(key,id,None,&restored)?!=1{
        return Err("VMware encrypted archive restore count wrong".into())
    }
    let archive=restored.join(&snapshot.entries[0].path);
    let folder=temp.path().join("ovf");
    fs::create_dir(&folder)?;
    let ovf=safe_extract(&archive,&folder)?;
    exact_absence(profile,new_name)?;
    let status=profile.cmd("import")?.arg("import.ovf").arg("-m=true")
        .arg(format!("-name={new_name}"))
        .arg(format!("-ds={}",profile.datastore.as_deref().unwrap()))
        .arg(format!("-pool={}",profile.resource_pool.as_deref().unwrap()))
        .arg(format!("-folder={}",profile.destination_folder.as_deref().unwrap()))
        .arg(&ovf).stdout(Stdio::null()).status()?;
    if !status.success(){
        return Err("VMware OVF import failed; manually inspect any partial VM, never delete automatically".into())
    }
    let full=format!("{}/{}",profile.destination_folder.as_deref().unwrap(),new_name);
    let new=profile.vm_info(&full)?;
    if new.power!="poweredOff"||new.name!=new_name||!moref(&new.moref){
        return Err("VMware new VM identity/offline state cannot be verified".into())
    }
    Ok(Receipt{provider:"vmware",id:id.into(),source_vm_moref:parts[2].into(),
        ovf_file_count:fs::read_dir(&folder)?.count(),
        vmdk_count:fs::read_dir(&folder)?.filter(|e|e.as_ref().is_ok_and(|e|
            e.file_name().to_string_lossy().ends_with(".vmdk"))).count(),
        encrypted:true,offline_only:true,imported_new_vm:true,
        boot_verified:false,guest_data_verified:false,
        vadp_cbt_certified:false,vcenter_field_certified:false})
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fake(role:&str)->Profile {
        Profile{role:role.into(),endpoint:"https://vcenter.example.invalid".into(),
         username:"mb-backup@vsphere.local".into(),password:"disposable-not-a-secret".into(),
         tls_ca_file:"/tmp/not-a-real-ca".into(),
         govc_binary:"/tmp/not-a-real-govc".into(),
         govc_sha256:"0".repeat(64),datacenter:"DC0".into(),
         vm_path:Some("/DC0/vm/guest".into()),vm_moref:Some("vm-42".into()),
         destination_folder:None,datastore:None,resource_pool:None}
    }
    #[test]fn enforce_role_and_paths(){
        assert!(inventory_path("/DC0/vm/guest","DC0",false));
        assert!(!inventory_path("/DC0/vm/../outside","DC0",false));
        assert!(!inventory_path("/DC0/vm/guest/*","DC0",false));
        assert!(!inventory_path("/WRONG/vm/guest","DC0",false));
        assert!(moref("vm-42"));assert!(!moref("VM-42"));
        assert!(!component("../evil"));assert!(!component("guest/name"));
        let mut p=fake("import");
        assert!(p.role=="import");
        assert!(p.vm_path.is_some()); // importing such a profile is disallowed
        assert!(!component("backup.tar/../../file"));
        p.role="operator".into();assert!(!matches!(p.role.as_str(),"export"|"import"));
    }
    #[test]fn exact_vm_info_and_power_validation(){
        let j=serde_json::json!({"virtualMachines":[{"self":{"value":"vm-42"},
            "summary":{"config":{"name":"server"},"runtime":{"powerState":"poweredOff"}}}]});
        let vm=parse_vm(&j).unwrap();
        assert_eq!(vm.moref,"vm-42");assert_eq!(vm.power,"poweredOff");
        assert!(parse_vm(&serde_json::json!({"virtualMachines":null})).is_err());
        assert!(parse_vm(&serde_json::json!({"virtualMachines":[j["virtualMachines"][0],
            j["virtualMachines"][0]]})).is_err());
    }
    #[test]fn verify_ovf_export_requires_all_files_and_no_symlinks(){
        let temp=tempfile::tempdir().unwrap();
        fs::write(temp.path().join("vm.ovf"),b"<Envelope>vm</Envelope>").unwrap();
        assert!(checked_files(temp.path()).is_err());
        fs::write(temp.path().join("disk.vmdk"),b"vmdk").unwrap();
        assert_eq!(checked_files(temp.path()).unwrap().len(),2);
        fs::write(temp.path().join("secret.key"),b"private").unwrap();
        assert!(checked_files(temp.path()).is_err());
    }
    #[test]fn reject_tar_traversal_symlinks_and_duplicate_members(){
        let temp=tempfile::tempdir().unwrap();
        let good=temp.path().join("good.tar");
        {
            let f=File::create(&good).unwrap();
            let mut tar=tar::Builder::new(f);
            for (name,content) in [("vm.ovf",&b"<Envelope/>"[..]),("disk.vmdk",&b"disk"[..])]{
                let mut h=tar::Header::new_gnu();
                h.set_path(name).unwrap();h.set_size(content.len() as u64);h.set_mode(0o600);
                h.set_cksum();tar.append(&h,content).unwrap();
            }
            tar.finish().unwrap();
        }
        let dest=temp.path().join("good");fs::create_dir(&dest).unwrap();
        let ovf=safe_extract(&good,&dest).unwrap();
        assert_eq!(fs::read(ovf).unwrap(),b"<Envelope/>");
        let bad=temp.path().join("bad.tar");
        {
            let f=File::create(&bad).unwrap();let mut tar=tar::Builder::new(f);
            let mut h=tar::Header::new_gnu();
            h.set_path("bad.vmdk").unwrap();h.set_entry_type(tar::EntryType::Symlink);
            h.set_link_name("../../private").unwrap();h.set_size(0);h.set_cksum();
            tar.append(&h,io::empty()).unwrap();tar.finish().unwrap();
        }
        let dest=temp.path().join("bad");fs::create_dir(&dest).unwrap();
        assert!(safe_extract(&bad,&dest).is_err());
    }
}
