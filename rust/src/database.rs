//! Rust logical database input adapters. CLI clients are external executables:
//! pg_dump/pg_restore or mariadb-dump/mariadb. No shell interpolation.
//! Supports consistent FULL logical dumps, isolated NEW-target restores.
//! WAL/binlog PITR, replicas, snapshot fencing and physical incremental
//! backups are NOT implemented or certified.
use crate::{chunk_store::{BackupKind,LogicalBackup,Store},
    execution_policy::{ExecutionLimits,ThrottledReader,ThrottledWriter}};
use serde::{Deserialize,Serialize};
use std::{fs,io::Write,path::{Path,PathBuf},process::{Command,Stdio}};

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Engine { Postgresql, Mysql }
impl Engine {
    fn id(self)->&'static str {
        match self {Self::Postgresql=>"postgresql",Self::Mysql=>"mysql"}
    }
    fn suffix(self)->&'static str {
        match self {Self::Postgresql=>"pgcustom",Self::Mysql=>"sql"}
    }
}
#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub engine:Engine,
    pub host:String,
    pub port:u16,
    pub user:String,
    pub database:String,
    pub credentials_file:PathBuf,
    #[serde(default)] pub tls_ca_file:Option<PathBuf>,
}
fn ident(s:&str)->bool {
    !s.is_empty() && s.len()<=63 &&
    s.as_bytes()[0].is_ascii_alphabetic() &&
    s.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_')
}
fn regular_private(path:&Path)->Result<(),String>{
    if !path.is_absolute(){return Err("Database secret/certificate path must be absolute".into());}
    let md=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !md.file_type().is_file()||md.file_type().is_symlink(){
        return Err("Database credential/certificate must be a regular file".into());
    }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if md.permissions().mode()&0o077!=0 {
            return Err("Database credential/certificate file must have 0600 permissions".into());
        }
    }
    Ok(())
}
impl Profile {
    pub fn validate(&self)->Result<(),String>{
        if !ident(&self.user)||!ident(&self.database)||self.port==0||
           self.host.is_empty()||self.host.len()>253||
           !self.host.bytes().all(|c|c.is_ascii_alphanumeric()||
                 c==b'.'||c==b'-'){
            return Err("Unsafe database identity, endpoint or port".into());
        }
        regular_private(&self.credentials_file)?;
        if let Some(ca)=&self.tls_ca_file {regular_private(ca)?;}
        else if self.host!="localhost" && self.host!="127.0.0.1"{
            return Err("Remote database requires pinned TLS CA trust".into());
        }
        Ok(())
    }
    pub fn confirmation(&self)->String{
        format!("{}:{}:{}:{}",self.engine.id(),self.host,self.port,self.database)
    }
    fn command(&self,executable:&str)->Command{
        let mut cmd=Command::new(executable);
        cmd.env_remove("PGPASSWORD").env_remove("MYSQL_PWD")
           .env_remove("PGSERVICE").env_remove("PGHOSTADDR");
        match self.engine {
            Engine::Postgresql=>{
                cmd.env("PGPASSFILE",&self.credentials_file);
                if let Some(ca)=&self.tls_ca_file {
                    cmd.env("PGSSLMODE","verify-full").env("PGSSLROOTCERT",ca);
                }else{cmd.env("PGSSLMODE","disable");}
            }
            Engine::Mysql=>{
                // Must be the first argument to MariaDB command-line clients.
                cmd.arg(format!("--defaults-extra-file={}",
                    self.credentials_file.display()));
                if let Some(ca)=&self.tls_ca_file {
                    cmd.arg(format!("--ssl-ca={}",ca.display()));
                    cmd.arg("--ssl-verify-server-cert");
                }
            }
        };
        cmd
    }
    fn connection(&self,cmd:&mut Command){
        match self.engine {
            Engine::Postgresql=>{
                cmd.args(["--host",&self.host,"--port",&self.port.to_string(),
                          "--username",&self.user,"--no-password"]);
            }
            Engine::Mysql=>{
                cmd.arg(format!("--host={}",self.host))
                   .arg(format!("--port={}",self.port))
                   .arg(format!("--user={}",self.user))
                   .arg("--protocol=tcp");
            }
        }
    }
}
fn must_succeed(mut cmd:Command,name:&str)->Result<(),String>{
    let status=cmd.stdin(Stdio::null()).stdout(Stdio::null())
        .stderr(Stdio::inherit()).status().map_err(|e|e.to_string())?;
    if status.success(){Ok(())}
    else{Err(format!("{name} refused or failed; target needs manual inspection"))}
}
/// Stream the database CLI process stdout into the real encrypted age/zstd
/// content-addressed store; refuse to publish any manifest after exit != 0.
pub fn backup(profile:&Profile,root:&Path,identity:&Path,label:&str,
              limits:&ExecutionLimits)->Result<LogicalBackup,String>{
    profile.validate()?;limits.validate()?;limits.check_host()?;
    let mut cmd=match profile.engine {
        Engine::Postgresql=>{
            let mut c=profile.command("pg_dump");profile.connection(&mut c);
            c.args(["--format=custom","--no-owner","--no-acl",
                    "--dbname",&profile.database]);
            c
        }
        Engine::Mysql=>{
            let mut c=profile.command("mariadb-dump");profile.connection(&mut c);
            c.args(["--single-transaction","--quick","--skip-lock-tables",
                    "--hex-blob","--routines","--triggers","--events",
                    "--no-create-db",&profile.database]);
            c
        }
    };
    let store=Store::open(root,identity).map_err(|e|e.to_string())?;
    let mut child=cmd.stdin(Stdio::null()).stdout(Stdio::piped())
        .stderr(Stdio::inherit()).spawn().map_err(|e|e.to_string())?;
    let stdout=child.stdout.take().ok_or("Database dumper stdout missing")?;
    let mut reader=ThrottledReader::new(stdout,limits.clone())?;
    let logical=format!("database/{}/{}.{}",profile.engine.id(),
                        profile.database,profile.engine.suffix());
    let result=store.backup_stream(identity,label,"",&logical,&mut reader,
        BackupKind::Full,None,||{
            if child.wait().map_err(|e|e.to_string())?.success(){Ok(())}
            else{Err("Database dump exited nonzero; incomplete data rejected".into())}
        });
    let snapshot=match result{
        Ok(s)=>s,
        Err(e)=>{let _=child.kill();let _=child.wait();return Err(e.to_string());}
    };
    if !store.verify(identity,&snapshot.id).map_err(|e|e.to_string())?.healthy {
        return Err("Encrypted database backup failed verification".into());
    }
    Ok(snapshot)
}
/// Never modifies an existing target. CREATE DATABASE fails when it exists;
/// a failed restore leaves its NEW partial target for manual inspection.
pub fn restore_new(profile:&Profile,root:&Path,identity:&Path,id:&str,
                   confirmation:&str,limits:&ExecutionLimits)->Result<u64,String>{
    profile.validate()?;limits.validate()?;limits.check_host()?;
    if confirmation!=profile.confirmation(){
        return Err("Destination confirmation differs from new target".into());
    }
    let store=Store::open(root,identity).map_err(|e|e.to_string())?;
    if !store.verify(identity,id).map_err(|e|e.to_string())?.healthy{
        return Err("Database backup integrity verification failed".into());
    }
    let snap=store.load(identity,id).map_err(|e|e.to_string())?;
    if snap.kind!=BackupKind::Full||snap.parent_id.is_some()||
       snap.entries.len()!=1||!snap.top_namespace.is_empty(){
        return Err("Database restore requires one standalone full dump".into());
    }
    let entry=&snap.entries[0];
    let prefix=format!("database/{}/",profile.engine.id());
    let ending=format!(".{}",profile.engine.suffix());
    if !entry.path.starts_with(&prefix)||!entry.path.ends_with(&ending)||
       entry.path.split('/').count()!=3{
        return Err("Wrong provider or incompatible database dump".into());
    }
    match profile.engine{
        Engine::Postgresql=>{
            let mut c=profile.command("createdb");profile.connection(&mut c);
            c.args(["--maintenance-db=postgres","--template=template0",
                    &profile.database]);
            must_succeed(c,"PostgreSQL create-new")?;
        }
        Engine::Mysql=>{
            let mut c=profile.command("mariadb");profile.connection(&mut c);
            // The validated ASCII identifier cannot contain backticks or SQL.
            c.arg(format!("--execute=CREATE DATABASE {} CHARACTER SET utf8mb4",
                          profile.database));
            must_succeed(c,"MariaDB create-new")?;
        }
    }
    let mut cmd=match profile.engine{
        Engine::Postgresql=>{
            let mut c=profile.command("pg_restore");profile.connection(&mut c);
            c.args(["--exit-on-error","--no-owner","--no-acl",
                    "--dbname",&profile.database]);c
        }
        Engine::Mysql=>{
            let mut c=profile.command("mariadb");profile.connection(&mut c);
            c.arg(&profile.database);c
        }
    };
    let mut child=cmd.stdin(Stdio::piped()).stdout(Stdio::null())
        .stderr(Stdio::inherit()).spawn().map_err(|e|e.to_string())?;
    let mut stdin=child.stdin.take().ok_or("Restore stdin missing")?;
    let mut writer=ThrottledWriter::new(&mut stdin,limits)?;
    let transferred=(||->Result<u64,String>{
        let mut bytes=0u64;
        for chunk in &entry.chunks{
            limits.check_host()?;
            let data=store.read_chunk_ref(identity,chunk)
                .map_err(|e|e.to_string())?;
            writer.write_all(&data).map_err(|e|e.to_string())?;
            bytes=bytes.checked_add(data.len() as u64)
                .ok_or("Restore counter overflow")?;
        }
        writer.flush().map_err(|e|e.to_string())?;
        Ok(bytes)
    })();
    drop(writer);drop(stdin);
    let bytes=match transferred{
        Ok(n)=>n,
        Err(e)=>{
            let _=child.kill();let _=child.wait();
            return Err(format!("Restore stream failed; leave NEW database for review: {e}"));
        }
    };
    if !child.wait().map_err(|e|e.to_string())?.success(){
        return Err("Database restore failed; inspect new partial database".into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn reject_untrusted_database_identifiers(){
        assert!(!ident("x;DELETE FROM table"));
        assert!(!ident("a-b"));
        assert!(!ident("1st"));
        assert!(ident("mb_source1"));
    }
}
