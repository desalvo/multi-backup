//! Multi Backup unattended Rust agent (AFS first, extensible per-plugin).
//! No inbound listener. Pull = outbound TLS poll for approved local profiles;
//! push = locally initiated signed TLS upload. No Python or shell evaluation.
use multi_backup_core::{afs_volume::{self,VolumeJob,Execution},
    agent_protocol as wire,execution_policy::{ExecutionLimits,ThrottledReader}};
use base64::{engine::general_purpose::STANDARD as B64,Engine as _};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{
    collections::BTreeMap,
    env,fs::{self,File,OpenOptions},
    io::{Read,Write},
    path::{Path,PathBuf},
    process::{Command,Stdio},
    time::Duration
};

type Res<T>=Result<T,String>;
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    agent_id:String,user:String,server_url:String,
    signing_key:PathBuf,token_file:PathBuf,sequence_file:PathBuf,
    profiles_dir:PathBuf,spool_dir:PathBuf,
    #[serde(default)] limits:ExecutionLimits,
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentProfile {
    id:String,
    recipient:String,
    job:VolumeJob,
    #[serde(default)] enable_push:bool,
}
#[derive(Serialize)]
struct UploadManifest<'a> {
    backup_id:&'a str,
    profile_id:&'a str,
    profile_sha256:&'a str,
    provider:&'static str,
    cell:&'a str,
    volume:&'a str,
    encrypted_bytes:u64,
    recipient:&'a str,
    #[serde(skip_serializing_if="Option::is_none")]
    pull_job_id:Option<&'a str>,
}
fn err<T:ToString>(value:T)->String{value.to_string()}
fn arguments(args:&[String])->Res<BTreeMap<String,String>>{
    let mut map=BTreeMap::new();
    let mut iter=args.iter();
    while let Some(key)=iter.next(){
        if !key.starts_with("--"){return Err(format!("Unexpected option {key}"));}
        let value=iter.next().ok_or_else(||format!("Missing value for {key}"))?;
        if value.starts_with("--")||map.insert(key.clone(),value.clone()).is_some(){
            return Err(format!("Unsafe or duplicate option {key}"));
        }
    }
    Ok(map)
}
fn required<'a>(m:&'a BTreeMap<String,String>,name:&str)->Res<&'a str>{
    m.get(name).map(String::as_str).ok_or_else(||format!("Missing {name}"))
}
fn path(m:&BTreeMap<String,String>,name:&str)->Res<PathBuf>{
    Ok(PathBuf::from(required(m,name)?))
}
fn private_file(file:&Path)->Res<Vec<u8>>{
    let info=fs::symlink_metadata(file).map_err(err)?;
    if !info.file_type().is_file(){return Err("Credential is not a regular file".into());}
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if info.permissions().mode() & 0o077 != 0{
            return Err(format!("Unsafe credential permissions: {}",file.display()));
        }
    }
    fs::read(file).map_err(err)
}
fn atomic_private(path:&Path,bytes:&[u8])->Res<()>{
    let parent=path.parent().ok_or("Missing private configuration parent")?;
    if !parent.is_dir(){return Err("Private configuration parent does not exist".into());}
    let mut file=tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.as_file().sync_all().map_err(err)?;
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file.path(),fs::Permissions::from_mode(0o600)).map_err(err)?;
    }
    file.persist(path).map_err(|e|err(e.error))?;
    Ok(())
}
fn init(opts:&BTreeMap<String,String>)->Res<()>{
    let agent_id=required(opts,"--id")?;
    let user=required(opts,"--user")?;
    let server_url=required(opts,"--server")?;
    if !wire::valid_id(agent_id)||!wire::valid_user(user)||!wire::valid_tls_endpoint(server_url){
        return Err("Agent identity or HTTPS endpoint invalid".into());
    }
    let location=path(opts,"--config")?;
    if location.exists(){return Err("Refusing to overwrite existing agent identity".into());}
    let parent=location.parent().ok_or("Missing config directory")?;
    let parent=fs::canonicalize(parent).map_err(err)?;
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&parent).map_err(err)?.permissions().mode()&0o077!=0{
            return Err("Agent config directory must be accessible only by its owner (0700)".into());
        }
    }
    let key=parent.join("agent-device-ed25519.key");
    let (secret,public)=wire::device_keypair();
    if key.exists(){return Err("Existing device key; refusing to rotate silently".into());}
    atomic_private(&key,&secret)?;
    let profiles=parent.join("profiles");
    let spool=parent.join("encrypted-spool");
    fs::create_dir_all(&profiles).map_err(err)?;
    fs::create_dir_all(&spool).map_err(err)?;
    #[cfg(unix)]{
        use std::os::unix::fs::PermissionsExt;
        for dir in [&profiles,&spool]{
            fs::set_permissions(dir,fs::Permissions::from_mode(0o700)).map_err(err)?;
        }
    }
    let cfg=Config{agent_id:agent_id.into(),user:user.into(),
        server_url:server_url.into(),signing_key:key,
        token_file:parent.join("agent-server-token.txt"),
        sequence_file:parent.join("agent-sequence.txt"),
        profiles_dir:profiles,spool_dir:spool,
        limits:ExecutionLimits::default()};
    atomic_private(&location,&serde_json::to_vec_pretty(&cfg).map_err(err)?)?;
    println!("{}",serde_json::to_string_pretty(&json!({
        "agent_id":agent_id,"user":user,"public_key":public,
        "status":"pending server registration and admin approval",
        "next":"Have server administrator register this device public key and provide a high-entropy token in the private token_file; never transmit private key."
    })).map_err(err)?);
    Ok(())
}
fn load(opts:&BTreeMap<String,String>)->Res<Config>{
    let c:Config=serde_json::from_slice(&private_file(&path(opts,"--config")? )?).map_err(err)?;
    if !wire::valid_id(&c.agent_id)||!wire::valid_user(&c.user)||
       !wire::valid_tls_endpoint(&c.server_url){
        return Err("Invalid agent identity or HTTPS server endpoint".into());
    }
    c.limits.validate()?;
    Ok(c)
}
fn profile(config:&Config,id:&str)->Res<AgentProfile>{
    if !wire::valid_id(id){return Err("Invalid approved profile identifier".into());}
    let location=config.profiles_dir.join(format!("{id}.json"));
    let info=fs::symlink_metadata(&location).map_err(err)?;
    if !info.file_type().is_file(){return Err("Unsafe agent job profile".into());}
    let body=fs::read(&location).map_err(err)?;
    if body.len()>32*1024{return Err("Agent profile too large".into());}
    let p:AgentProfile=serde_json::from_slice(&body).map_err(err)?;
    if p.id!=id||p.job.id!=id||!p.recipient.starts_with("age1")||
       p.recipient.len()>120 || !p.recipient.bytes().all(|b|b.is_ascii_alphanumeric()){
        return Err("Invalid pinned agent volume profile".into());
    }
    match &p.job.execution{
        Execution::ManagedAgent{agent_id} if agent_id==&config.agent_id=>{},
        _=>return Err("Profile is not pinned to this exact agent ID".into()),
    };
    // Configured profile is trusted locally; remote job may only select this
    // exact profile and must provide matching profile_sha256 to start it.
    Ok(p)
}
fn credentials(config:&Config)->Res<(String,[u8;32])>{
    let token=String::from_utf8(private_file(&config.token_file)?)
        .map_err(err)?.trim_end_matches(['\r','\n']).to_owned();
    if token.len()<43||token.len()>256 ||token.chars().any(char::is_whitespace){
        return Err("Agent server token missing, weak or malformed".into());
    }
    let keybytes=private_file(&config.signing_key)?;
    let key:[u8;32]=keybytes.as_slice().try_into()
        .map_err(|_|"Invalid Ed25519 private key file")?;
    Ok((token,key))
}
fn next_sequence(config:&Config)->Res<u64>{
    let lock_path=config.sequence_file.with_extension("lock");
    let lock=OpenOptions::new().create(true).write(true).open(lock_path).map_err(err)?;
    fs2::FileExt::lock_exclusive(&lock).map_err(err)?;
    let old=fs::read_to_string(&config.sequence_file).unwrap_or_else(|_|"0".into());
    let next=old.trim().parse::<u64>().map_err(err)?.checked_add(1)
        .ok_or("Agent request sequence exhausted")?;
    atomic_private(&config.sequence_file,next.to_string().as_bytes())?;
    fs2::FileExt::unlock(&lock).map_err(err)?;
    Ok(next)
}
fn signed_headers(config:&Config,method:&str,route:&str,
                  body_sha:&str,metadata:&[u8])->Res<Vec<(String,String)>>{
    let (token,private)=credentials(config)?;
    let proof=wire::sign(&private,method,route,&config.agent_id,
        &config.user,next_sequence(config)?,wire::now(),body_sha,metadata)?;
    Ok(vec![
        ("Authorization".into(),format!("Bearer {token}")),
        ("X-MB-Agent-Proof".into(),B64.encode(serde_json::to_vec(&proof).map_err(err)?)),
        ("X-MB-Agent-Metadata".into(),B64.encode(metadata)),
    ])
}
fn request(config:&Config,method:&str,route:&str,body:Option<&Path>,
           metadata:&[u8],limits:&ExecutionLimits)->Res<Value>{
    limits.validate()?;
    limits.check_host()?;
    if !wire::valid_tls_endpoint(&config.server_url){
        return Err("Refusing non-HTTPS agent connection".into());
    }
    let expected=if let Some(path)=body{
        let mut reader=File::open(path).map_err(err)?;
        let mut sha=Sha256::new();let mut buffer=[0u8;65536];
        loop{let n=reader.read(&mut buffer).map_err(err)?;
            if n==0{break;}sha.update(&buffer[..n]);}
        hex::encode(sha.finalize())
    }else{wire::hex_sha(b"")};
    let headers=signed_headers(config,method,route,&expected,metadata)?;
    let endpoint=format!("{}{}",config.server_url.trim_end_matches('/'),route);
    let agent=ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .timeout_write(Duration::from_secs(300)).redirects(0).build();
    let mut r=agent.request(method,&endpoint);
    for (key,value) in headers {r=r.set(&key,&value);}
    let response=if let Some(path)=body{
        r.set("Content-Type","application/octet-stream")
            .send(ThrottledReader::new(File::open(path).map_err(err)?,
                limits.clone())?)
    }else{r.call()}.map_err(err)?;
    response.into_json::<Value>().map_err(err)
}
/// Process-wide, crash-released slot permits. Each configured agent enforces
/// concurrent jobs across multiple CLI commands/processes, not only across
/// threads created by its own poll loop. Slots are files inside the private
/// agent spool directory, held open for the full backup+upload lifecycle.
struct SlotGuard(File);
impl Drop for SlotGuard {
    fn drop(&mut self){let _=fs2::FileExt::unlock(&self.0);}
}
fn acquire_slot(config:&Config,limits:&ExecutionLimits)->Res<SlotGuard>{
    limits.validate()?;
    let root=fs::canonicalize(&config.spool_dir).map_err(err)?;
    if !fs::symlink_metadata(&root).map_err(err)?.file_type().is_dir(){
        return Err("Unsafe agent concurrency directory".into());
    }
    for index in 0..limits.max_concurrent_jobs{
        let filename=root.join(format!(".backup-slot-{index:03}.lock"));
        let lock=OpenOptions::new().read(true).write(true).create(true)
            .open(&filename).map_err(err)?;
        if !fs::symlink_metadata(&filename).map_err(err)?.file_type().is_file(){
            return Err("Unsafe non-regular agent worker slot".into());
        }
        if fs2::FileExt::try_lock_exclusive(&lock).is_ok(){
            return Ok(SlotGuard(lock));
        }
    }
    Err("Agent concurrent job ceiling reached: no free worker slot".into())
}
fn age_encrypt_to_spool(config:&Config,p:&AgentProfile,limits:&ExecutionLimits)->Res<PathBuf>{
    limits.validate()?;limits.check_host()?;
    fs::create_dir_all(&config.spool_dir).map_err(err)?;
    let ciphertext=tempfile::Builder::new().prefix("pending-afs-")
        .suffix(".age").tempfile_in(&config.spool_dir).map_err(err)?;
    let target=ciphertext.path().to_owned();
    // No plaintext AFS dump is ever materialized on disk.
    let mut encryption=Command::new("age").arg("-r").arg(&p.recipient)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
        .spawn().map_err(err)?;
    let stdout=encryption.stdout.take().ok_or("age stdout unavailable")?;
    let mut encrypted_file=ciphertext.reopen().map_err(err)?;
    let copy=std::thread::spawn(move||{
        let mut encrypted=stdout;
        let result=std::io::copy(&mut encrypted,&mut encrypted_file);
        if result.is_ok(){let _=encrypted_file.sync_all();}
        result
    });
    let mut stdin=encryption.stdin.take().ok_or("age stdin unavailable")?;
    let mut job=p.job.clone();
    // The pinned configuration names the agent; locally it has no need
    // for a network listener and executes only its own allowlisted vos command.
    job.execution=Execution::Local;
    let dump=afs_volume::stream_dump_limited(&job,&mut stdin,limits);
    drop(stdin);
    let output=copy.join().map_err(|_|"Encrypted pipeline worker failed")?
        .map_err(err)?;
    let status=encryption.wait().map_err(err)?;
    if let Err(reason)=dump{return Err(reason);}
    if !status.success()||output==0{return Err("age encryption failed".into());}
    ciphertext.keep().map_err(|e|err(e.error))?;
    Ok(target)
}
fn push(config:&Config,profile_id:&str,force:bool,limits:&ExecutionLimits,
        pull_job_id:Option<&str>)->Res<()>{
    let p=profile(config,profile_id)?;
    if !p.enable_push&&!force {return Err("This agent profile has push disabled".into());}
    // Push is initiated by the AGENT, independently of a server schedule,
    // but it still obtains the server's signed-session policy BEFORE running.
    let effective=if force{
        limits.clone() // Already intersected with server policy by signed poll.
    }else{
        let policy=request(config,"GET","/api/agents/policy",None,b"",limits)?;
        if policy["approved"]!=true{return Err("Agent is not approved".into());}
        let server:ExecutionLimits=serde_json::from_value(
            policy.get("limits").cloned().ok_or("Missing server resource policy")?
        ).map_err(err)?;
        ExecutionLimits::strictest(&server,&config.limits)?
    };
    let _worker_slot=acquire_slot(config,&effective)?;
    let encrypted=age_encrypt_to_spool(config,&p,&effective)?;
    let id=hex::encode(rand::random::<[u8;32]>());
    // The server approves the exact bytes of this locally installed profile,
    // not merely the agent identity or a human-readable profile name.
    let profile_body=fs::read(config.profiles_dir.join(format!("{profile_id}.json")))
        .map_err(err)?;
    let profile_sha256=wire::hex_sha(&profile_body);
    let size=fs::metadata(&encrypted).map_err(err)?.len();
    let manifest=UploadManifest{backup_id:&id,profile_id:&p.id,
        profile_sha256:&profile_sha256,provider:"afs",
        cell:&p.job.cell,volume:&p.job.volume,encrypted_bytes:size,
        recipient:&p.recipient,pull_job_id};
    let meta=serde_json::to_vec(&manifest).map_err(err)?;
    match request(config,"POST","/api/agents/upload",Some(&encrypted),&meta,&effective){
        Ok(result)=>{
            fs::remove_file(encrypted).map_err(err)?;
            println!("{}",result);
            Ok(())
        },
        Err(error)=>{
            eprintln!("Encrypted spool retained for operator review: {}",encrypted.display());
            Err(error)
        }
    }
}
fn poll(config:&Config,once:bool,interval:u64)->Res<()>{
    if !(10..=3600).contains(&interval){return Err("Poll seconds must be 10..3600".into());}
    loop{
        match request(config,"GET","/api/agents/poll",None,b"",&config.limits){
            Ok(json)=>{
                if json["approved"]!=true {return Err("Agent is not approved by server".into());}
                let server_limits:ExecutionLimits=serde_json::from_value(
                    json.get("limits").cloned().ok_or(
                        "Server poll omitted mandatory resource limits")?
                    ).map_err(err)?;
                let effective=ExecutionLimits::strictest(&server_limits,&config.limits)?;
                if let Some(jobs)=json["jobs"].as_array(){
                    let mut ready=Vec::new();
                    for job in jobs{
                        let id=job["profile_id"].as_str().ok_or("Missing profile")?;
                        let id=id.to_owned();
                        let p=profile(config,&id)?;
                        let body=fs::read(config.profiles_dir.join(format!("{id}.json")))
                            .map_err(err)?;
                        if wire::hex_sha(&body)!=job["profile_sha256"].as_str().unwrap_or(""){
                            return Err("Server job differs from locally approved profile".into());
                        }
                        let task=job["job_id"].as_str().ok_or("Missing pull lease ID")?;
                        if !wire::valid_id(task){return Err("Invalid remote pull ID".into());}
                        let _=p;
                        ready.push((id,task.to_owned()));
                    }
                    // Distinct AFS volumes are independent streams. Bounded
                    // parallel workers cannot split an individual vos dump.
                    // Each upload carries its own signed nonce and a lease ID.
                    let max=effective.max_concurrent_jobs as usize;
                    std::thread::scope(|scope|->Res<()>{
                        for group in ready.chunks(max){
                            let mut handles=Vec::new();
                            for (id,task) in group{
                                let l=effective.clone();
                                handles.push(scope.spawn(move||
                                    push(config,id,true,&l,Some(task))));
                            }
                            for worker in handles{
                                worker.join().map_err(|_|"Agent worker panicked")??;
                            }
                        }
                        Ok(())
                    })?;
                }
            },
            Err(e)=>eprintln!("Agent poll failed (no job will run): {e}"),
        }
        if once {return Ok(());}
        std::thread::sleep(Duration::from_secs(interval));
    }
}
fn run()->Res<()>{
    let args:Vec<_>=env::args().skip(1).collect();
    if args.len()==1&&args[0]=="--version"{
        println!("multi-backup-agent 0.1.0-r039-dev");return Ok(());
    }
    if args.is_empty(){return Err("Usage: multi-backup-agent init|status|preview|run|push|poll [--config FILE] [--profile ID]".into());}
    let cmd=&args[0];let opts=arguments(&args[1..])?;
    match cmd.as_str(){
        "init"=>init(&opts),
        "status"=>{
            let cfg=load(&opts)?;
            println!("{}",serde_json::to_string_pretty(&json!({
                "agent_id":cfg.agent_id,"user":cfg.user,"server":cfg.server_url,
                "mode":"agent-outbound-push-or-pull",
                "token_configured":cfg.token_file.exists(),
                "server_approval":"must be confirmed via signed poll"
            })).map_err(err)?);Ok(())
        }
        "preview"=>{
            let cfg=load(&opts)?;
            let profile_id=required(&opts,"--profile")?;
            let p=profile(&cfg,profile_id)?;
            let mut job=p.job;
            job.execution=Execution::Local;
            let command=afs_volume::plan(&job)?;
            let profile_sha256=wire::hex_sha(
                &fs::read(cfg.profiles_dir.join(format!("{profile_id}.json"))).map_err(err)?);
            println!("{}",serde_json::to_string_pretty(&json!({
                "agent_id":cfg.agent_id,
                "profile_id":profile_id,
                "profile_sha256":profile_sha256,
                "requires_admin_approval":true,
                "command_plan":command
            })).map_err(err)?);
            Ok(())
        }
        "run"=>{
            let cfg=load(&opts)?;let p=profile(&cfg,required(&opts,"--profile")?)?;
            let _worker_slot=acquire_slot(&cfg,&cfg.limits)?;
            let outfile=age_encrypt_to_spool(&cfg,&p,&cfg.limits)?;
            println!("Encrypted standalone AFS volume dump: {}",outfile.display());Ok(())
        }
        "push"=>{
            let cfg=load(&opts)?;
            push(&cfg,required(&opts,"--profile")?,false,&cfg.limits,None)
        },
        "poll"=>{
            let interval=opts.get("--poll-seconds").map(|x|x.parse::<u64>().map_err(err))
                .transpose()?.unwrap_or(60);
            let once=opts.get("--once").is_some_and(|v|v=="true");
            poll(&load(&opts)?,once,interval)
        }
        _=>Err("Unknown agent subcommand".into())
    }
}
fn main(){if let Err(error)=run(){eprintln!("Agent error: {error}");std::process::exit(1)}}
