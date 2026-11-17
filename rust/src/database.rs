//! Rust logical database input adapters. CLI clients are external executables:
//! pg_dump/pg_restore or mariadb-dump/mariadb. No shell interpolation.
//! Supports consistent FULL logical dumps, isolated NEW-target restores.
//! WAL/binlog PITR, replicas, snapshot fencing and physical incremental
//! backups are NOT implemented or certified.
use crate::{chunk_store::{BackupKind,LogicalBackup,Store},
    execution_policy::{ExecutionLimits,ThrottledReader,ThrottledWriter}};
use serde::{Deserialize,Serialize};
use std::{fs,io::{Read,Write},path::{Path,PathBuf},process::{Command,Stdio},
          time::Duration};
use serde_json::Value;
use url::Url;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Engine { Postgresql, Mysql }
#[derive(Clone,Debug,PartialEq,Eq)]
struct PatroniEvidence{ leader:String,timeline:u64,members:usize }
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
    /// Optional real Galera membership preflight, enforced before/after dump.
    /// This does not replace consistency proofs for concurrent DDL.
    #[serde(default)] pub galera_expected_nodes:Option<u16>,
    /// An OPTIONAL fail-closed leader-lock guard for genuine Patroni-managed
    /// PostgreSQL backup. Bare hostname never certifies HA consistency.
    /// HTTPS must use an OS-trusted CA; HTTP is never accepted here.
    #[serde(default)] pub patroni_api:Option<String>,
    #[serde(default)] pub patroni_expected_members:Option<u16>,
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
        if self.patroni_api.is_some()!=self.patroni_expected_members.is_some(){
            return Err("Patroni URL and expected membership must be supplied together".into());
        }
        if let Some(count)=self.patroni_expected_members{
            if self.engine!=Engine::Postgresql || !(2..=32).contains(&count){
                return Err("Patroni requires PostgreSQL and 2..32 expected members".into());
            }
            let api=Url::parse(self.patroni_api.as_deref().unwrap())
                .map_err(|_|"Invalid Patroni HTTPS authority")?;
            if api.scheme()!="https" || api.host_str()!=Some(self.host.as_str()) ||
               api.username()!="" || api.password().is_some() ||
               api.query().is_some() || api.fragment().is_some() ||
               api.path()!="/" {
                return Err("Patroni REST must be the exact PostgreSQL host over trusted HTTPS root".into());
            }
        }
        if let Some(count)=self.galera_expected_nodes {
            if self.engine!=Engine::Mysql || !(2..=64).contains(&count){
                return Err("Galera membership guard requires MariaDB and 2..64 nodes".into());
            }
        }
        regular_private(&self.credentials_file)?;
        if let Some(ca)=&self.tls_ca_file {regular_private(ca)?;}
        else if self.host!="localhost" && self.host!="127.0.0.1"{
            return Err("Remote database requires pinned TLS CA trust".into());
        }
        Ok(())
    }
    /// Read-only Patroni leader guard, never equivalent to a distributed
    /// snapshot barrier. The /primary 200 endpoint requires a local primary
    /// holding a leader lock. /cluster must corroborate expected membership.
    fn check_patroni(&self)->Result<Option<PatroniEvidence>,String>{
        let Some(api)=self.patroni_api.as_deref() else {return Ok(None)};
        let n=self.patroni_expected_members.ok_or("Patroni expected size missing")? as usize;
        let endpoint=Url::parse(api).map_err(|_|"Invalid configured Patroni URL")?;
        let client=ureq::AgentBuilder::new().redirects(0)
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(15)).build();
        let get=|route:&str|->Result<Value,String>{
            let mut url=endpoint.clone();
            url.set_path(route);
            let response=client.get(url.as_str()).set("Accept","application/json")
                .call().map_err(|_|"Patroni quorum or leader-lock verification failed")?;
            if response.status()!=200{
                return Err("Patroni REST must return 200 only for healthy leadership".into());
            }
            let mut body=Vec::new();
            response.into_reader().take(65_537).read_to_end(&mut body)
                .map_err(|_|"Patroni health response unreadable")?;
            if body.len()>65_536{return Err("Patroni health JSON exceeds bound".into());}
            serde_json::from_slice(&body).map_err(|_|"Patroni status is not JSON".into())
        };
        let primary=get("/primary")?;
        // Patroni /primary returns HTTP 200 only while holding the leader lock;
// the JSON PG role is "master" in Patroni 3.x and "primary" in newer
// terminology. Require BOTH 200 and an independently matching DCS leader.
        if !matches!(primary["role"].as_str(),Some("master")|Some("primary")) ||
           primary["state"]!="running"{
            return Err("Patroni refuses backup on replica or stale-primary state".into());
        }
        let leader=primary.pointer("/patroni/name")
            .and_then(Value::as_str).ok_or("Patroni primary lacks member identity")?;
        if leader.len()>128||leader.is_empty(){return Err("Unsafe Patroni leader name".into());}
        let timeline=primary["timeline"].as_u64()
            .ok_or("Patroni primary lacks timeline")?;
        let cluster=get("/cluster")?;
        let members=cluster["members"].as_array()
            .ok_or("Patroni cluster lacks members")?;
        if members.len()!=n{return Err("Patroni membership is not at required quorum size".into());}
        let leaders=members.iter().filter(|x|{
            x["role"]=="leader" || x["role"]=="primary"
        }).collect::<Vec<_>>();
        if leaders.len()!=1 || leaders[0]["name"]!=leader ||
           leaders[0]["state"]!="running"{
            return Err("Patroni cluster leadership contradicts local /primary".into());
        }
        let mut unique=std::collections::HashSet::new();
        for member in members{
            let name=member["name"].as_str().ok_or("Nameless Patroni member")?;
            if !unique.insert(name){return Err("Patroni membership duplicates a member".into());}
            if !matches!(member["state"].as_str(),Some("running")|Some("streaming")){
                return Err("Patroni member is not healthy".into());
            }
        }
        Ok(Some(PatroniEvidence{leader:leader.to_owned(),timeline,members:n}))
    }

    /// Only a genuine Galera Synced Primary member at the expected cluster
    /// size can publish a snapshot under an explicit Galera profile.
    fn check_galera(&self)->Result<(),String>{
        let Some(expected)=self.galera_expected_nodes else{return Ok(());};
        let mut cmd=self.command("mariadb");
        self.connection(&mut cmd);
        cmd.args(["--batch","--skip-column-names",
                  "--execute=SHOW GLOBAL STATUS LIKE 'wsrep_%'"]);
        let out=cmd.stdout(Stdio::piped()).stderr(Stdio::null())
            .output().map_err(|e|e.to_string())?;
        if !out.status.success()||out.stdout.len()>65536{
            return Err("Galera wsrep health query failed".into());
        }
        let contents=String::from_utf8(out.stdout)
            .map_err(|_|"Galera status returned invalid UTF-8")?;
        let field=|key:&str|->Option<&str>{
            contents.lines().find_map(|line|{
                let (name,value)=line.split_once('\t')?;
                if name==key {Some(value)} else {None}
            })
        };
        if field("wsrep_cluster_status")!=Some("Primary") ||
           field("wsrep_ready")!=Some("ON") ||
           field("wsrep_local_state_comment")!=Some("Synced") ||
           field("wsrep_cluster_size").and_then(|s|s.parse::<u16>().ok())!=Some(expected){
            return Err(format!(
                "Galera backup refused: expected {expected} Synced nodes, Primary and ready"
            ));
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
    profile.check_galera()?;
    let initial_patroni=profile.check_patroni()?;
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
            if child.wait().map_err(|e|e.to_string())?.success(){
                profile.check_galera().map_err(|e|
                    format!("Galera membership changed during backup: {e}"))?;
                if profile.check_patroni()? != initial_patroni {
                    return Err("Patroni leader/timeline/healthy membership changed during dump; incomplete backup rejected".into());
                }
                Ok(())
            } else {
                Err("Database dump exited nonzero; incomplete data rejected".into())
            }
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
    if profile.galera_expected_nodes.is_some(){
        return Err("Restore into a Galera cluster is not approved; use an isolated standalone destination".into());
    }
    if confirmation!=profile.confirmation(){
        return Err("Destination confirmation differs from new target".into());
    }
    let initial_patroni=profile.check_patroni()?;
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
    if profile.check_patroni()? != initial_patroni {
        return Err("Patroni leadership changed during new-database restore; new partial target requires manual inspection".into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn patroni_guard_refuses_replica_and_insecure_authority(){
        let mut p=Profile{engine:Engine::Postgresql,host:"primary.example.test".to_owned(),
            port:5432,user:"postgres".to_owned(),database:"testdb".to_owned(),
            credentials_file:PathBuf::from("/irrelevant"),
            tls_ca_file:None,galera_expected_nodes:None,
            patroni_api:Some("http://primary.example.test:8008/".to_owned()),
            patroni_expected_members:Some(3)};
        assert!(p.validate().is_err());
        p.patroni_api=Some("https://other.example.test:8008/".to_owned());
        assert!(p.validate().is_err());
        p.patroni_api=None;
        assert!(p.validate().is_err());
    }
    #[test]fn reject_untrusted_database_identifiers(){
        assert!(!ident("x;DELETE FROM table"));
        assert!(!ident("a-b"));
        assert!(!ident("1st"));
        assert!(ident("mb_source1"));
    }
}
