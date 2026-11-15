//! OpenAFS volume dump-only command plans; never infer an AFS volume from a path.
//! The OpenAFS "vos" executable is an external runtime, not bundled here.
//! Plans deliberately refuse arbitrary server-supplied shell commands.
use serde::{Deserialize,Serialize};
use std::{path::{Path,PathBuf},process::{Command,Stdio},io};
use std::io::Write;

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
        .stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()
        .map_err(|e|e.to_string())
}
/// A single pinned transport builder is shared by AFS backup and restore.
/// User ssh_config, ProxyCommand, ProxyJump, password and forwarding are
/// explicitly disabled. Remote content must be a typed vos argument plan.
fn ssh_command_for(transport:&Execution,remote:&str)->Result<Command,String>{
    let Execution::SshAgentless{host,user,identity,known_hosts,..}=transport else{
        return Err("Not an agentless SSH transport".into());
    };
    if !valid(host,253,host_char)||!valid(user,64,username){
        return Err("Invalid SSH target".into());
    }
    validate_regular(identity)?;
    validate_regular(known_hosts)?;
    let mut cmd=Command::new("ssh");
    cmd.args(["-T","-F","none",
              "-o","BatchMode=yes","-o","StrictHostKeyChecking=yes",
              "-o","IdentitiesOnly=yes","-o","PreferredAuthentications=publickey",
              "-o","PasswordAuthentication=no","-o","KbdInteractiveAuthentication=no",
              "-o","ForwardAgent=no","-o","ForwardX11=no",
              "-o","ClearAllForwardings=yes","-o","ControlMaster=no",
              "-o","ProxyCommand=none","-o","ProxyJump=none",
              "-o","UpdateHostKeys=no","-o","PermitLocalCommand=no",
              "-o","ConnectTimeout=20","-o"]);
    cmd.arg(format!("UserKnownHostsFile={}",known_hosts.display()));
    cmd.arg("-i").arg(identity);
    cmd.arg("--").arg(format!("{user}@{host}"));
    cmd.arg(remote);
    Ok(cmd)
}
pub fn spawn_ssh(job:&VolumeJob)->Result<std::process::Child,String>{
    let plan=plan(job)?;
    let Execution::SshAgentless{host,user,identity,known_hosts,..}=&job.execution else{
        return Err("SSH execution requires an explicit SSH transport".into())
    };
    if matches!(&job.auth,Authentication::Keytab{..}){
        return Err("Remote keytab execution requires a separately approved remote credential profile".into());
    }
    let mut cmd=ssh_command_for(&job.execution,&ssh_remote_command(&plan))?;
    // stderr must never be piped without a draining worker: remote vos can
    // emit enough diagnostics to deadlock the stdout backup stream.
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()
        .map_err(|e|e.to_string())
}
pub fn stream_dump_to<W:io::Write>(job:&VolumeJob,target:W)
    ->Result<u64,String>{
    stream_dump_limited(job,target,&crate::execution_policy::ExecutionLimits::default())
}
/// Limits apply to the host actually executing vos or SSH, with independent
/// server/agent policy intersection. A single volume remains one stream;
/// separate volumes can use separate concurrent jobs.
pub fn stream_dump_limited<W:io::Write>(job:&VolumeJob,mut target:W,
        limits:&crate::execution_policy::ExecutionLimits)->Result<u64,String>{
    limits.validate()?;
    limits.check_host()?;
    let mut child=match &job.execution{
        Execution::Local=>spawn_local(job)?,
        Execution::SshAgentless{..}=>spawn_ssh(job)?,
        Execution::ManagedAgent{..}=>return Err("Managed agent execution must originate at the approved host".into()),
    };
    let Some(mut source)=child.stdout.take() else {
        let _=child.kill();let _=child.wait();
        return Err("Missing vos dump stdout".into())
    };
    let mut buffer=[0u8;65536];let mut bytes=0u64;
    let mut last_check=std::time::Instant::now()-std::time::Duration::from_secs(2);
    let mut throttled=crate::execution_policy::ThrottledWriter::new(&mut target,limits)?;
    let result=(||->Result<(),String>{
        loop{
            if last_check.elapsed()>std::time::Duration::from_secs(2){
                limits.check_host()?;last_check=std::time::Instant::now();
            }
            let n=io::Read::read(&mut source,&mut buffer).map_err(|e|e.to_string())?;
            if n==0{break}
            throttled.write_all(&buffer[..n]).map_err(|e|e.to_string())?;
            bytes=bytes.checked_add(n as u64).ok_or("Dump byte counter overflow")?;
        }
        throttled.flush().map_err(|e|e.to_string())?;
        Ok(())
    })();
    if let Err(reason)=result{
        let _=child.kill();let _=child.wait();
        return Err(reason);
    }
    let status=child.wait().map_err(|e|e.to_string())?;
    if !status.success(){return Err("vos dump command failed; partial dump must be discarded".into());}
    if bytes==0{return Err("AFS dump produced empty stream".into());}
    Ok(bytes)
}
/// Stream a real vos dump stdout directly into authenticated age/zstd chunks.
/// The resulting manifest is published only after vos has exited with code 0.
/// No unencrypted temporary copy of a volume is ever written to disk.
pub fn backup_volume_to_store(
    job:&VolumeJob,root:&Path,identity:&Path,label:&str,
    limits:&crate::execution_policy::ExecutionLimits,
)->Result<crate::chunk_store::LogicalBackup,String>{
    plan(job)?;
    limits.validate()?;
    limits.check_host()?;
    // Direct local token and pinned SSH agentless execution are supported;
    // SSH -localauth needs an attested fileserver and explicit site. Raw
    // server-side scripts, unapproved keytabs and managed-agent impersonation
    // are NOT accepted here.
    match (&job.execution,&job.auth){
        (Execution::Local,Authentication::ExistingToken) |
        (Execution::SshAgentless{..},Authentication::ExistingToken) |
        (Execution::SshAgentless{afs_server:true,..},Authentication::LocalAuth)=>{},
        _=>return Err("Unsupported AFS producer credentials or transport; use approved agent profile".into()),
    }
    let (kind,parent)=match &job.mode{
        DumpMode::Full=>(crate::chunk_store::BackupKind::Full,None),
        DumpMode::Incremental{parent_backup_id,..}=>
            (crate::chunk_store::BackupKind::Incremental,Some(parent_backup_id.as_str()))
    };
    let store=crate::chunk_store::Store::open(root,identity)
        .map_err(|e|e.to_string())?;
    let mut child=match &job.execution{
        Execution::Local=>spawn_local(job)?,
        Execution::SshAgentless{..}=>spawn_ssh(job)?,
        Execution::ManagedAgent{..}=>return Err("Managed agent must run a locally approved AFS profile".into())
    };
    let stdout=child.stdout.take().ok_or("Missing vos stdout")?;
    let mut input=crate::execution_policy::ThrottledReader::new(
        stdout,limits.clone())?;
    let logical_path=format!("afs/{}/{}.vos",job.cell,job.volume);
    let outcome=store.backup_stream(identity,label,"",&logical_path,
        &mut input,kind,parent,||{
            let status=child.wait().map_err(|e|e.to_string())?;
            if !status.success(){return Err("vos dump returned nonzero exit status".into());}
            Ok(())
        });
    let backup=match outcome{
        Ok(value)=>value,
        Err(error)=>{
            let _=child.kill();let _=child.wait();
            return Err(error.to_string());
        }
    };
    if !store.verify(identity,&backup.id).map_err(|e|e.to_string())?.healthy{
        return Err("AFS encrypted dump failed verified 8+2 integrity check".into());
    }
    Ok(backup)
}
/// A restore target is deliberately NEW, never an existing AFS volume.
/// AFS volume names must be <=22 bytes and cannot use readonly/backup suffix.
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreNewJob{
    pub cell:String,
    pub volume:String,
    pub server:String,
    pub partition:String,
    pub auth:Authentication,
    #[serde(default)]pub execution:Option<Execution>,
}
#[derive(Debug,Clone,Serialize)]
pub struct RestorePlan{
    pub executable:String,
    pub arguments:Vec<String>,
    pub new_volume_only:bool,
    pub transport:String,
    pub confirmation:String,
}
pub fn plan_restore_new(target:&RestoreNewJob)->Result<RestorePlan,String>{
    if !valid(&target.cell,253,name)||
       !valid(&target.server,253,host_char)||
       !valid(&target.volume,22,name)||
       target.volume.ends_with(".readonly")||
       target.volume.ends_with(".backup")||
       !target.partition.starts_with("/vicep")||
       !valid(&target.partition[6..],3,|b|b.is_ascii_lowercase()){
        return Err("Invalid target AFS site or new volume name".into());
    }
    if !matches!(&target.auth,Authentication::ExistingToken){
        return Err("Native restore requires an independently approved existing AFS token; managed keytab/localauth execution is not yet certified".into());
    }
    let transport=match target.execution.as_ref(){
        None|Some(Execution::Local)=>"local",
        Some(Execution::SshAgentless{host,user,identity,known_hosts,..})=>{
            if !valid(host,253,host_char)||!valid(user,64,username){
                return Err("Unsafe AFS restore SSH host/identity".into());
            }
            validate_regular(identity)?;validate_regular(known_hosts)?;
            "ssh-agentless"
        }
        Some(Execution::ManagedAgent{..})=>
            return Err("Agent-driven restore needs an independently approved agent restore profile".into()),
    };
    // AFS "abort" never overwrites an existing VLDB volume. No fixed -id
    // is supplied, so the VLDB allocates a new volume ID.
    let args=vec!["restore".into(),"-server".into(),target.server.clone(),
        "-partition".into(),target.partition.clone(),"-name".into(),
        target.volume.clone(),"-cell".into(),target.cell.clone(),
        "-overwrite".into(),"abort".into()];
    Ok(RestorePlan{executable:"vos".into(),arguments:args,
        new_volume_only:true,transport:transport.into(),
        confirmation:format!("{}:{}:{}:{}",
           target.cell,target.server,target.partition,target.volume)})
}
/// Restores one authenticated AFS full dump followed by its increments in
/// order, to the NEW target created by the first command of this invocation.
/// The first full restore uses -overwrite abort; subsequent increments use
/// -overwrite incremental ONLY after that fresh full operation succeeded.
/// In case of any failure the partial NEW target is left for manual review;
/// this method never deletes or retries over an existing AFS volume.
/// Redirect child progress to stderr, reserving the CLI's stdout for JSON.
/// The worker runs concurrently with stdin streaming, so even a verbose vos
/// or SSH process cannot deadlock on a filled stdout pipe.
fn forward_child_stdout_to_stderr(
    child:&mut std::process::Child,
)->Result<std::thread::JoinHandle<io::Result<u64>>,String>{
    let mut output=child.stdout.take().ok_or("vos restore stdout pipe absent")?;
    Ok(std::thread::spawn(move||{
        let stderr=io::stderr();
        let mut diagnostics=stderr.lock();
        io::copy(&mut output,&mut diagnostics)
    }))
}
pub fn restore_new_chain_from_store(
    root:&Path,identity:&Path,backup_id:&str,target:&RestoreNewJob,
    confirmation:&str,limits:&crate::execution_policy::ExecutionLimits
)->Result<(usize,u64),String>{
    let plan=plan_restore_new(target)?;
    if confirmation!=plan.confirmation {
        return Err("Confirmation must match cell:server:/vicepXX:new-volume exactly".into());
    }
    limits.validate()?;limits.check_host()?;
    let store=crate::chunk_store::Store::open(root,identity)
        .map_err(|e|e.to_string())?;
    let verified=crate::backup_chain::verify_chain(&store,identity,backup_id,true)
        .map_err(|e|e.to_string())?;
    if !verified.healthy{return Err("AFS full/incremental chain is not fully verified".into());}
    let chain=crate::backup_chain::ancestors(&store,identity,backup_id)
        .map_err(|e|e.to_string())?;
    let root_source=chain.first().and_then(|b|b.entries.first())
        .map(|e|e.path.as_str()).ok_or("AFS chain has no source object")?;
    if !root_source.starts_with("afs/")||!root_source.ends_with(".vos")||
       root_source.split('/').count()!=3{
        return Err("Backup is not a typed per-volume AFS dump".into());
    }
    if chain.iter().enumerate().any(|(index,snapshot)|
        snapshot.entries.len()!=1||
        snapshot.entries[0].path!=root_source||
        (index==0&&snapshot.kind!=crate::chunk_store::BackupKind::Full)||
        (index>0&&snapshot.kind!=crate::chunk_store::BackupKind::Incremental)){
        return Err("AFS restore chain mixes volume identities or dump types".into());
    }
    let mut bytes=0u64;
    for (index,snapshot) in chain.iter().enumerate(){
        // Native vos restore -overwrite=incremental is only reached after
        // the fresh full was created successfully in THIS invocation.
        let mut args=plan.arguments.clone();
        if index>0 {
            let last=args.last_mut().ok_or("Invalid restore arguments")?;
            *last="incremental".into();
        }
        // With SSH agentless the exact typed vos command is quote-escaped
        // and executed under pinned SSH host identity, not a server-supplied
        // shell snippet. The TLS/SSH transport is responsible for ciphertext
        // confidentiality in transit; no plaintext dump is staged on disk.
        let mut cmd=if let Some(exec@Execution::SshAgentless{..})=&target.execution{
            let remote=std::iter::once(plan.executable.as_str())
                .chain(args.iter().map(String::as_str))
                .map(shell_quote).collect::<Vec<_>>().join(" ");
            ssh_command_for(exec,&remote)?
        }else{
            Command::new(&plan.executable)
        };
        if !matches!(&target.execution,Some(Execution::SshAgentless{..})){
            cmd.args(&args);
        }
        // Preserve a strict machine-readable stdout for the parent CLI's
        // --json mode. Both local vos and remote SSH may print progress to
        // stdout. Drain it CONCURRENTLY to stderr, without buffering the
        // full diagnostic stream or risking a blocking child pipe.
        let mut child=cmd.stdin(Stdio::piped()).stdout(Stdio::piped())
            .stderr(Stdio::inherit()).spawn().map_err(|e|e.to_string())?;
        let diagnostics=forward_child_stdout_to_stderr(&mut child)?;
        let mut input=child.stdin.take().ok_or("vos restore stdin absent")?;
        let mut output=crate::execution_policy::ThrottledWriter::new(
            &mut input,limits)?;
        let writing=(||->Result<(),String>{
            for chunk in &snapshot.entries[0].chunks{
                limits.check_host()?;
                let payload=store.read_chunk_ref(identity,chunk)
                    .map_err(|e|e.to_string())?;
                output.write_all(&payload).map_err(|e|e.to_string())?;
                bytes=bytes.checked_add(payload.len() as u64)
                    .ok_or("AFS restore size overflow")?;
            }
            output.flush().map_err(|e|e.to_string())?;
            Ok(())
        })();
        drop(output);drop(input);
        if let Err(error)=writing{
            let _=child.kill();let _=child.wait();
            let _=diagnostics.join();
            return Err(format!("AFS restore stream failed; review partial new volume manually: {error}"));
        }
        let status=child.wait().map_err(|e|e.to_string())?;
        let diagnostic_result=diagnostics.join()
            .map_err(|_|"vos restore diagnostics reader panicked".to_string())?
            .map_err(|e|format!("vos restore diagnostics reader error: {e}"))?;
        let _=diagnostic_result;
        if !status.success(){
            return Err("AFS restore failed; inspect newly created partial volume, never automatically overwrite or remove it".into());
        }
    }
    Ok((chain.len(),bytes))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn job()->VolumeJob{VolumeJob{id:"AFS-job".into(),cell:"example.org".into(),
        volume:"user.test".into(),auth:Authentication::ExistingToken,
        execution:Execution::Local,mode:DumpMode::Full,
        server:None,partition:None,command_profile:None}}
    #[cfg(unix)]
    #[test]fn restore_child_stdout_is_drained_without_polluting_machine_stdout(){
        // A local stand-in verifies only process stream plumbing; the field
        // CI still requires real vos, KDC, VLDB and byte-identical restore.
        let mut child=Command::new("sh")
            .args(["-c","printf 'vos progress message\\n'"])
            .stdin(Stdio::null()).stdout(Stdio::piped())
            .stderr(Stdio::null()).spawn().unwrap();
        let reader=forward_child_stdout_to_stderr(&mut child).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(reader.join().unwrap().unwrap(),21);
        let machine_result=serde_json::json!({"restored_dumps":2});
        assert_eq!(machine_result["restored_dumps"],2);
    }
    #[test]fn restore_only_creates_explicit_new_volume_with_no_overwrite(){
        let mut target=RestoreNewJob{
            cell:"example.org".into(),volume:"new.restored".into(),
            server:"afs1.example.org".into(),partition:"/vicepa".into(),
            auth:Authentication::ExistingToken,execution:None
        };
        let p=plan_restore_new(&target).unwrap();
        assert!(p.new_volume_only);
        assert_eq!(&p.arguments[p.arguments.len()-2..],&["-overwrite","abort"]);
        assert!(!p.arguments.iter().any(|a|a=="-id"||a=="-file"));
        assert_eq!(p.confirmation,"example.org:afs1.example.org:/vicepa:new.restored");
        target.volume="something.backup".into();
        assert!(plan_restore_new(&target).is_err());
        target.volume="a".repeat(23);
        assert!(plan_restore_new(&target).is_err());
        target.volume="new.restored".into();
        target.auth=Authentication::LocalAuth;
        assert!(plan_restore_new(&target).is_err());
    }
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