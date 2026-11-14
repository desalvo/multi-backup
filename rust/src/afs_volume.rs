//! OpenAFS volume dump-only command plans; never infer an AFS volume from a path.
//! The OpenAFS "vos" executable is an external runtime, not bundled here.
//! Plans deliberately refuse arbitrary server-supplied shell commands.
use serde::{Deserialize,Serialize};
use std::{path::{Path,PathBuf},process::{Command,Stdio},io};

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="snake_case",tag="type")]
pub enum Authentication {
    ExistingToken,
    Keytab { principal:String, keytab:PathBuf },
    LocalAuth,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="snake_case",tag="type")]
pub enum Execution {
    Local,
    SshAgentless {host:String,user:String,identity:PathBuf,known_hosts:PathBuf,
                  #[serde(default)] afs_server:bool},
    ManagedAgent {agent_id:String},
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="snake_case",tag="kind")]
pub enum DumpMode {
    Full,
    Incremental{since:String, parent_backup_id:String},
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeJob {
    pub id:String,
    pub cell:String,
    pub volume:String,
    pub auth:Authentication,
    pub execution:Execution,
    pub mode:DumpMode,
    #[serde(default)] pub server:Option<String>,
    #[serde(default)] pub partition:Option<String>,
    #[serde(default)] pub command_profile:Option<String>,
}
#[derive(Debug,Serialize)]
pub struct ExecutionPlan {
    pub job_id:String,
    pub volume:String,
    pub executable:String,
    pub arguments:Vec<String>,
    pub transport:String,
    pub requires_host_agent:bool,
    pub requires_preflight:Vec<String>,
}
fn valid(s:&str,max:usize,allowed:fn(u8)->bool)->bool{
    !s.is_empty()&&s.len()<=max&&s.bytes().all(allowed)
}
fn name(v:u8)->bool {v.is_ascii_alphanumeric()||b"._-".contains(&v)}
fn host_char(v:u8)->bool {name(v)||v==b':'}
fn username(v:u8)->bool {name(v)}
fn validate_regular(path:&Path)->Result<(),String>{
    if !path.is_absolute(){return Err("Credential paths must be absolute".into());}
    let md=std::fs::symlink_metadata(path).map_err(|e|e.to_string())?;
    if !md.file_type().is_file()||md.file_type().is_symlink(){
        return Err("Credential path must be a regular non-symlink file".into());
    }
    Ok(())
}
pub fn plan(job:&VolumeJob)->Result<ExecutionPlan,String>{
    for (s,max) in [(&job.id,96),(&job.cell,253),(&job.volume,253)]{
        if !valid(s,max,name){return Err("Invalid AFS identifier".into());}
    }
    match (&job.server,&job.partition){
        (None,None)=>{},
        (Some(s),Some(p))=>{
            if !valid(s,253,host_char)||!p.starts_with("/vicep")||!valid(&p[6..],3,|b|b.is_ascii_lowercase()){
                return Err("Unsafe AFS site/partition selection".into());
            }
        }
        _=>return Err("AFS -server and -partition must be supplied together".into())
    }
    if let Some(profile)=&job.command_profile{
        // Locally pinned profiles only; remote call may choose profile ID but
        // must never submit executable shell text for interpolation.
        if !valid(profile,96,name){return Err("Invalid command profile ID".into());}
    }
    let mut args=vec!["dump".into(),"-id".into(),job.volume.clone()];
    let mut prerequisites=vec!["Inspect VLDB location and volume type before dumping".into(),
        "Verify source/destination AFS cell and selected volume scope".into()];
    if let DumpMode::Incremental{since,parent_backup_id}=&job.mode{
        // OpenAFS accepts explicit mm/dd/yyyy HH:MM timestamps, not a generic
        // chain identifier; require metadata from the previous successful dump.
        let date=since.as_bytes();
        if date.len()!=16 || date[2]!=b'/'||date[5]!=b'/'||date[10]!=b' '||date[13]!=b':'||
           date.iter().enumerate().any(|(i,b)|![2,5,10,13].contains(&i)&&!b.is_ascii_digit())||
           !valid(parent_backup_id,128,|b|b.is_ascii_alphanumeric()||b==b'-'){
            return Err("Incremental requires a validated previous dump timestamp and backup ID".into());
        }
        args.push("-time".into());args.push(since.clone());
        prerequisites.push("Confirm previous dump timestamp is gap-free, especially RO release/clone".into());
    }
    if let (Some(s),Some(p))=(&job.server,&job.partition){
        args.extend(["-server".into(),s.clone(),"-partition".into(),p.clone()]);
    }
    let transport=match &job.execution {
        Execution::Local=>"local".to_owned(),
        Execution::SshAgentless{host,user,identity,known_hosts,afs_server}=>{
            if !valid(host,253,host_char)||!valid(user,64,username){
                return Err("Invalid SSH host or remote principal".into());
            }
            if !*afs_server && matches!(&job.auth,Authentication::LocalAuth){
                return Err("Remote -localauth needs an explicitly attested AFS server".into());
            }
            validate_regular(identity)?;validate_regular(known_hosts)?;
            prerequisites.push("Verify SSH host public key against pinned known_hosts".into());
            "ssh-agentless".into()
        }
        Execution::ManagedAgent{agent_id}=>{
            if !valid(agent_id,96,name){return Err("Invalid approved agent ID".into());}
            prerequisites.push("Server must approve the exact live authenticated agent before dispatch".into());
            "managed-agent".into()
        }
    };
    match &job.auth{
        Authentication::ExistingToken=>{
            args.extend(["-cell".into(),job.cell.clone()]);
            prerequisites.push("Ensure the selected AFS administrator token is valid".into());
        }
        Authentication::Keytab{principal,keytab}=>{
            if !valid(principal,253,|x|x.is_ascii_alphanumeric()||b"@/._-".contains(&x)){
                return Err("Invalid Kerberos principal".into());
            }
            validate_regular(keytab)?;
            args.extend(["-cell".into(),job.cell.clone()]);
            prerequisites.push("Acquire Kerberos tickets using locally authorized keytab, then aklog".into());
        }
        Authentication::LocalAuth=>{
            if job.server.is_none()&&!matches!(&job.execution,Execution::Local|Execution::ManagedAgent{..}){
                return Err("Remote -localauth requires explicit site and server-side authority".into());
            }
            args.push("-localauth".into());
            prerequisites.push("Require an AFS fileserver host and approved privileged identity".into());
        }
    }
    Ok(ExecutionPlan{job_id:job.id.clone(),volume:job.volume.clone(),
        executable:"vos".into(),arguments:args,transport,
        requires_host_agent:matches!(&job.execution,Execution::ManagedAgent{..}),
        requires_preflight:prerequisites})
}
fn shell_quote(value:&str)->String{
    format!("'{}'",value.replace('\'',"'\\''"))
}
pub fn ssh_remote_command(plan:&ExecutionPlan)->String{
    std::iter::once(plan.executable.as_str())
        .chain(plan.arguments.iter().map(String::as_str))
        .map(shell_quote).collect::<Vec<_>>().join(" ")
}
pub fn spawn_local(job:&VolumeJob)->Result<std::process::Child,String>{
    if !matches!(&job.execution,Execution::Local){
        return Err("Local execution cannot impersonate remote SSH/managed agents".into());
    }
    let plan=plan(job)?;
    if matches!(&job.auth,Authentication::Keytab{..}){
        return Err("Keytab jobs require managed credential acquisition; standalone dump refuses uninitialized Kerberos context".into());
    }
    if matches!(&job.auth,Authentication::LocalAuth){
        return Err("Privileged -localauth needs server-side managed command approval".into());
    }
    Command::new(&plan.executable).args(&plan.arguments)
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|e|e.to_string())
}
pub fn spawn_ssh(job:&VolumeJob)->Result<std::process::Child,String>{
    let plan=plan(job)?;
    let Execution::SshAgentless{host,user,identity,known_hosts,..}=&job.execution else{
        return Err("SSH execution requires an explicit SSH transport".into())
    };
    if matches!(&job.auth,Authentication::Keytab{..}){
        return Err("Remote keytab execution requires a separately approved remote credential profile".into());
    }
    let mut cmd=Command::new("ssh");
    cmd.args(["-T","-o","BatchMode=yes","-o","StrictHostKeyChecking=yes",
              "-o","IdentitiesOnly=yes","-o","ForwardAgent=no",
              "-o","PermitLocalCommand=no","-o","ConnectTimeout=20","-o"]);
    cmd.arg(format!("UserKnownHostsFile={}",known_hosts.display()));
    cmd.arg("-i").arg(identity);
    cmd.arg("--").arg(format!("{user}@{host}"));
    cmd.arg(ssh_remote_command(&plan));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|e|e.to_string())
}
pub fn stream_dump_to<W:io::Write>(job:&VolumeJob,mut target:W)
    ->Result<u64,String>{
    let mut child=match &job.execution{
        Execution::Local=>spawn_local(job)?,
        Execution::SshAgentless{..}=>spawn_ssh(job)?,
        Execution::ManagedAgent{..}=>return Err("Managed agent execution must originate at the approved host".into()),
    };
    let Some(mut source)=child.stdout.take() else {return Err("Missing vos dump stdout".into())};
    let bytes=io::copy(&mut source,&mut target).map_err(|e|e.to_string())?;
    let result=child.wait().map_err(|e|e.to_string())?;
    if !result.success(){return Err("vos dump command failed; partial dump must be discarded".into());}
    if bytes==0{return Err("AFS dump produced empty stream".into());}
    Ok(bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn job()->VolumeJob{VolumeJob{id:"AFS-job".into(),cell:"example.org".into(),
        volume:"user.test".into(),auth:Authentication::ExistingToken,
        execution:Execution::Local,mode:DumpMode::Full,
        server:None,partition:None,command_profile:None}}
    #[test]fn safe_full_plan(){
        let p=plan(&job()).unwrap();
        assert_eq!(p.executable,"vos");
        assert_eq!(p.arguments,vec!["dump","-id","user.test","-cell","example.org"]);
        assert!(!p.arguments.contains(&"-file".to_string()));
    }
    #[test]fn incremental_requires_explicit_chain_anchor(){
        let mut j=job();
        j.mode=DumpMode::Incremental{since:"11/14/2026 14:30".into(),
            parent_backup_id:"parent01".into()};
        let p=plan(&j).unwrap();assert!(p.arguments.contains(&"-time".to_string()));
        j.mode=DumpMode::Incremental{since:"$(touch /tmp/pwn)".into(),
            parent_backup_id:"parent".into()};
        assert!(plan(&j).is_err());
    }
    #[test]fn no_shell_or_bad_sites(){
        let mut j=job();
        j.volume="x;rm -rf /".into();assert!(plan(&j).is_err());
        j=job();j.server=Some("afs.example.org".into());
        assert!(plan(&j).is_err());
    }
    #[test]fn remote_localauth_requires_pinned_site(){
        let mut j=job();j.auth=Authentication::LocalAuth;
        j.execution=Execution::SshAgentless{host:"afs1".into(),user:"root".into(),
            identity:"/missing".into(),known_hosts:"/missing".into(),afs_server:false};
        assert!(plan(&j).is_err());
    }
}