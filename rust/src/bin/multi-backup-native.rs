//! Standalone native Rust encrypted backup CLI. Native plugin execution is
//! the primary development path; Python data migration is optional.
use multi_backup_core::{portable, chunk_store, archive, archive_management, checkpoint_audit, dropbox_client, dropbox_hash, github_client, journal, remote_restore, updater, vault, plugins, plugin_executor, backup_chain, account_restore, scheduler};
use zeroize::Zeroizing;
use std::io::{self, Read};
use std::{collections::BTreeMap, env, path::Path, process};

fn usage() -> &'static str {
    "Multi Backup Rust archive CLI (EUPL-1.2)\n\
     Usage:\n\
       multi-backup-native --version\n\
       multi-backup-native plugins list [--role input|output] [--mode backup|restore] [--json]\n\
       multi-backup-native plugins plan --input PLUGIN --output PLUGIN --mode backup|restore [--input-lvm MODE] [--output-lvm MODE] [--input-gpfs-access ces-nfs|ces-smb] [--output-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins probe --plugin gpfs|sshfs|nfs|smb|cephfs|glusterfs|lustre --mount PREMOUNTED_DIR [--access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins backup --input posix|nfs|smb|sshfs|gpfs|cephfs|glusterfs|lustre --output posix --source DIR --root STORE --identity AGE_KEY --label NAME [--namespace NAME] [--top-namespace PATH] [--source-snapshot-attested] [--input-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins restore --input posix --output posix|nfs|sshfs|cephfs|glusterfs|lustre --root STORE --identity AGE_KEY --id HASH --destination NEW_DIR [--selection PATH] [--json]\n\
       multi-backup-native schedule run --config JOBS.json --state PRIVATE_STATE.json [--force] [--json]\n\
       multi-backup-native schedule daemon --config JOBS.json --state PRIVATE_STATE.json [--poll-seconds 60]\n\
       multi-backup-native storage init --root DIR --recipient age1KEY [--chunk-mib 4]\n\
       multi-backup-native storage backup --root DIR --identity IDENTITY --label NAME --sources-json SOURCES.json [--mode full|incremental] [--parent BACKUP_ID] [--top-namespace PATH]\n\
       multi-backup-native storage info --root DIR --identity IDENTITY --id HASH [--json]\n\
       multi-backup-native storage verify-chain --root DIR --identity IDENTITY --id HASH [--no-chain] [--json]\n\
       multi-backup-native storage delete-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH (automatically rebase dependents)\n\
       multi-backup-native storage gc-preview --root DIR --identity IDENTITY [--grace-hours 48] [--json]\n\
       multi-backup-native storage gc-collect --root DIR --identity IDENTITY --confirm ERASE-ORPHANS [--grace-hours 48] [--json]\n\
       multi-backup-native storage retention-preview --root DIR --identity IDENTITY [--days 30] [--keep-last 1] [--json]\n\
       multi-backup-native storage list --root DIR --identity IDENTITY [--json]\n\
       multi-backup-native storage verify --root DIR --identity IDENTITY --id HASH\n\
       multi-backup-native storage protect-backup --root DIR --identity IDENTITY --id HASH\n\
       multi-backup-native storage unprotect-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH\n\
       multi-backup-native storage delete-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH\n\
       multi-backup-native storage migrate-legacy --root DIR --identity IDENTITY --label NAME --archives-json SOURCES.json\n\
       multi-backup-native storage export-legacy --root DIR --identity IDENTITY --id HASH --destination NEW.tar.gz.age\n\
       multi-backup-native storage export-zip --root DIR --identity IDENTITY --id HASH --destination NEW_DIR [--volume-mib 2048]\n\
       multi-backup-native storage import-zip --root NEW_DIR --identity IDENTITY --bundle PORTABLE_DIR\n\
       multi-backup-native storage relocate --root SOURCE --identity IDENTITY --destination NEW_DIR [--dry-run]\n\
       multi-backup-native storage restore --root DIR --identity IDENTITY --id HASH --destination NEW_DIR [--selection PATH]\n\
       multi-backup-native archive create --source DIR --output FILE.tar.gz.age --recipient age1... [--prefix multi-backup]\n\
       multi-backup-native archive list --archive FILE.tar.gz.age --identity AGE_KEY [--json]\n\
       multi-backup-native archive verify --archive FILE.tar.gz.age --identity AGE_KEY\n\
       multi-backup-native archive verify --archive FILE.tar.gz.age --vault PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native archive extract --archive FILE.tar.gz.age --identity AGE_KEY --destination NEW_DIR\n\
       multi-backup-native archive delete --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age --confirm EXACT_FILENAME\n\
       multi-backup-native archive protect --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age\n\
       multi-backup-native archive unprotect --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age --confirm EXACT_FILENAME\n\
       multi-backup-native checkpoint audit --file ACTIVE.json [--json]\n\
       multi-backup-native journal start --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal resume --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal record --directory STATE_DIR --config CONFIG.json --step STEP --file ARCHIVE\n\
       multi-backup-native journal finish --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native dropbox hash --file LOCAL_FILE\n\
       multi-backup-native dropbox backup --output-dir PRIVATE_DIR --recipient age1KEY [--remote-path /folder]\n\
       multi-backup-native dropbox resume --output-dir PRIVATE_DIR --recipient age1KEY [--remote-path /folder]\n\
       multi-backup-native github backup --output-dir PRIVATE_DIR --recipient age1KEY [--max-runs 5]\n\
       multi-backup-native github resume --output-dir PRIVATE_DIR --recipient age1KEY [--max-runs 5]\n\
       multi-backup-native restore github --archive ARCHIVE --identity AGE_KEY --target OWNER/NEW [--visibility private] --confirm CREATE-NEW\n\
       multi-backup-native restore github --archive ARCHIVE --vault PROTECTED.agevault --target OWNER/NEW --confirm CREATE-NEW [--password-stdin]\n\
       multi-backup-native restore dropbox --archive ARCHIVE --identity AGE_KEY --path /new/file --confirm CREATE-NEW\n\
       multi-backup-native vault verify --file PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native vault create --file NEW.agevault [--password-stdin]\n\
       multi-backup-native vault export --file SOURCE.agevault --destination RECOVERY.agevault [--password-stdin]\n\
       multi-backup-native vault import --file RECOVERY.agevault --destination NEW.agevault [--password-stdin]\n\
       multi-backup-native vault change-password --file PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native updates check --channel early-release|stable\n\
       multi-backup-native updates stage --channel early-release|stable --directory PRIVATE_UPDATE_DIR\n\
     Safety: explicit identity required, encrypted output only, no overwriting.\n"
}

fn options(args: &[String]) -> Result<(BTreeMap<&str, &str>, bool, bool), String> {
    let mut parsed = BTreeMap::new();
    let mut json = false;
    let mut password_stdin = false;
    let mut pos = 0;
    while pos < args.len() {
        if args[pos] == "--password-stdin" {
            if password_stdin { return Err("Duplicate password input flag".into()); }
            password_stdin = true;
            pos += 1;
            continue;
        }
        if ["--dry-run","--source-snapshot-attested","--no-chain","--rebase-dependents","--force"].contains(&args[pos].as_str()) {
            let flag=args[pos].as_str();
            if parsed.insert(flag,"true").is_some() {
                return Err(format!("Duplicate {flag} flag"));
            }
            pos += 1;
            continue;
        }
        if args[pos] == "--json" {
            if json { return Err("Duplicate --json flag".into()); }
            json = true;
            pos += 1;
            continue;
        }
        let key = args[pos].as_str();
        if !key.starts_with("--") || pos + 1 >= args.len() {
            return Err(format!("Expected --flag value, found {key}"));
        }
        let value = args[pos + 1].as_str();
        if value.starts_with("--") || parsed.insert(key, value).is_some() {
            return Err(format!("Missing or duplicate value for {key}"));
        }
        pos += 2;
    }
    Ok((parsed, json, password_stdin))
}
fn required<'a>(args: &'a BTreeMap<&str, &str>, flag: &str) -> Result<&'a str, String> {
    args.get(flag).copied().ok_or_else(|| format!("Missing {flag}"))
}
fn accepted(args: &BTreeMap<&str, &str>, known: &[&str]) -> Result<(), String> {
    for key in args.keys() {
        if !known.contains(key) {
            return Err(format!("Unknown option {key}"));
        }
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("multi-backup-native 0.1.0-r038-preview");
        return Ok(());
    }
    if args.len() < 2 { return Err(usage().into()); }
    let family = args[0].as_str();
    let command = args[1].as_str();
    let (flags, json, password_stdin) = options(&args[2..])?;
    if family == "plugins" {
        if password_stdin { return Err("Plugin catalog does not accept passwords".into()); }
        match command {
            "list" => {
                accepted(&flags, &["--role", "--mode"])?;
                let mode = plugins::Mode::parse(flags.get("--mode").copied().unwrap_or("backup"))?;
                let list = match flags.get("--role").copied() {
                    None => plugins::CATALOG.iter().collect::<Vec<_>>(),
                    Some("input") => plugins::catalog_for(mode, true),
                    Some("output") => plugins::catalog_for(mode, false),
                    _ => return Err("Role must be input or output".into()),
                };
                if json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for p in list {
                        let input = p.input_stage(mode);
                        let output = p.output_stage(mode);
                        println!("{}\t{}\t{:?}\tinput={:?}\toutput={:?}",
                                 p.id, p.title, p.kind, input, output);
                    }
                }
            },
            "plan" => {
                accepted(&flags, &["--input", "--output", "--mode",
                                    "--input-lvm", "--output-lvm",
                                    "--input-gpfs-access","--output-gpfs-access"])?;
                let mode = plugins::Mode::parse(required(&flags, "--mode")?)?;
                let input_lvm = flags.get("--input-lvm").map(|v|plugins::LvmMode::parse(v))
                    .transpose()?;
                let output_lvm = flags.get("--output-lvm").map(|v|plugins::LvmMode::parse(v))
                    .transpose()?;
                let gpfs_in=flags.get("--input-gpfs-access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let gpfs_out=flags.get("--output-gpfs-access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let item = plugins::plan_profiles(mode, required(&flags,"--input")?,
                    required(&flags,"--output")?,input_lvm,output_lvm,gpfs_in,gpfs_out)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&item)?);
                } else {
                    println!("{:?}: {} -> {} (input={:?}, output={:?}); executable={}; preview_executable={}",
                             item.mode, item.source_plugin, item.destination_plugin,
                             item.source_stage, item.destination_stage, item.executable,
                             item.preview_executable);
                    println!("{}", item.reason);
                    for check in &item.required_preflight {
                        println!("preflight: {check}");
                    }
                }
            },
            "probe" => {
                accepted(&flags, &["--plugin","--mount","--access"])?;
                let plugin=required(&flags,"--plugin")?;
                let access=flags.get("--access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let mount=Path::new(required(&flags,"--mount")?);
                let evidence=multi_backup_core::mount_probe::probe(plugin,mount,access)?;
                if json {println!("{}",serde_json::to_string_pretty(&evidence)?);}
                else {println!("{} {} fs={} rw_flag={} (read-only probe)",
                    evidence.plugin,evidence.mountpoint,evidence.fs_type,
                    evidence.rw_mount_option);}
            },
            "backup" => {
                accepted(&flags, &["--input","--output","--source","--root",
                                    "--identity","--label","--namespace","--top-namespace",
                                    "--source-snapshot-attested","--input-gpfs-access",
                                    "--mode","--parent"])?;
                let kind=chunk_store::BackupKind::parse(
                    flags.get("--mode").copied().unwrap_or("full"))?;
                let snapshot = plugin_executor::backup_with_mode(
                    required(&flags,"--input")?,required(&flags,"--output")?,
                    Path::new(required(&flags,"--source")?),
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--label")?,
                    flags.get("--namespace").copied().unwrap_or("files"),
                    flags.get("--top-namespace").copied().unwrap_or(""),
                    flags.contains_key("--source-snapshot-attested"),
                    flags.get("--input-gpfs-access").map(|v|plugins::GpfsAccess::parse(v)).transpose()?,
                    kind,flags.get("--parent").copied())?;
                if json { println!("{}",serde_json::to_string_pretty(&snapshot)?); }
                else {println!("Verified encrypted plugin backup {} ({} files)",
                              snapshot.id,snapshot.entries.len());}
            },
            "restore" => {
                accepted(&flags, &["--input","--output","--root","--identity",
                                    "--id","--destination","--selection"])?;
                let count = plugin_executor::restore(
                    required(&flags,"--input")?,required(&flags,"--output")?,
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?),
                    flags.get("--selection").copied())?;
                if json { println!("{}",serde_json::json!({"restored_files":count})); }
                else {println!("Restored {count} verified files to a new directory");}
            },
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "schedule" {
        if password_stdin {return Err("Scheduler uses separate age identity paths".into());}
        accepted(&flags,&["--config","--state","--force","--poll-seconds"])?;
        let config=Path::new(required(&flags,"--config")?);
        let state=Path::new(required(&flags,"--state")?);
        match command {
            "run"=>{
                if flags.contains_key("--poll-seconds") {
                    return Err("Polling interval is valid only in daemon mode".into());
                }
                let now=std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let output=scheduler::run_once(config,state,flags.contains_key("--force"),now)?;
                println!("{}",serde_json::to_string_pretty(&output)?);
            },
            "daemon"=>{
                if flags.contains_key("--force"){
                    return Err("--force is only valid for a one-shot manual run".into());
                }
                let poll=flags.get("--poll-seconds").copied().unwrap_or("60").parse::<u64>()?;
                if !(10..=3600).contains(&poll){return Err("Poll seconds must be 10..3600".into());}
                loop {
                    let now=std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                    match scheduler::run_once(config,state,false,now){
                        Ok(events)=>{
                            for event in events {
                                if event.executed {
                                    println!("Scheduler {} completed; backup={:?}",
                                        event.job_id,event.backup_id);
                                }
                            }
                        },
                        Err(error)=>eprintln!("Scheduler run failed: {error}"),
                    }
                    std::thread::sleep(std::time::Duration::from_secs(poll));
                }
            },
            _=>return Err(usage().into())
        }
        return Ok(());
    }
    if family == "storage" {
        if password_stdin { return Err("Storage uses a separately supplied age identity".into()); }
        let root = Path::new(required(&flags, "--root")?);
        match command {
            "init" => {
                accepted(&flags, &["--root","--recipient","--chunk-mib"])?;
                let chunk_mib = flags.get("--chunk-mib").copied().unwrap_or("4").parse::<usize>()?;
                let bytes = chunk_mib.checked_mul(1024*1024).ok_or("Invalid chunk size")?;
                chunk_store::Store::init(root, required(&flags,"--recipient")?, bytes)?;
                println!("Initialized immutable age/Zstandard storage; legacy archives untouched");
            }
            "backup" => {
                accepted(&flags, &["--root","--identity","--label","--sources-json",
                                    "--top-namespace","--mode","--parent"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let sources_file = std::fs::File::open(required(&flags, "--sources-json")?)?;
                let values: Vec<serde_json::Value> = serde_json::from_reader(sources_file)?;
                if values.is_empty() { return Err("Sources JSON must be nonempty".into()); }
                let sources: Vec<chunk_store::Source> = values.iter().map(|value| {
                    let namespace = value["namespace"].as_str().ok_or("Missing namespace")?;
                    let path = value["directory"].as_str().ok_or("Missing directory")?;
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(chunk_store::Source {
                        namespace: namespace.to_owned(), directory: path.into(),
                    })
                }).collect::<Result<_,_>>()?;
                let kind=chunk_store::BackupKind::parse(
                    flags.get("--mode").copied().unwrap_or("full"))?;
                let snapshot = store.backup_in_namespace_mode(identity, required(&flags,"--label")?,
                    flags.get("--top-namespace").copied().unwrap_or(""), &sources,
                    kind,flags.get("--parent").copied())?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }
            "list" => {
                accepted(&flags, &["--root","--identity"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let snapshots = chunk_store::Store::open(root, identity)?.list(identity)?;
                if json {
                    let values:Vec<_>=snapshots.iter().map(|backup|{
                        let mut value=serde_json::to_value(backup)?;
                        if let Some(obj)=value.as_object_mut(){
                            obj.insert("protected".into(),serde_json::json!(backup.protected));
                        }
                        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(value)
                    }).collect::<Result<_,_>>()?;
                    println!("{}",serde_json::to_string_pretty(&values)?);
                } else {
                    for backup in &snapshots {
                        println!("{} {} {} ({} logical files)",
                            if backup.protected {"[PROTECTED]"}else{"[UNPROTECTED]"},
                            backup.id,format!("{} [{:?}]",backup.label,backup.kind),backup.entries.len());
                    }
                }
            }
            "retention-preview" => {
                accepted(&flags,&["--root","--identity","--days","--keep-last"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let policy=multi_backup_core::backup_policy::Retention {
                    days:flags.get("--days").copied().unwrap_or("30").parse()?,
                    keep_last_verified:flags.get("--keep-last").copied().unwrap_or("1").parse()?,
                    immutable_retention_enforced:true,
                };
                let now=std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let preview=chunk_store::Store::open(root,key)?
                    .preview_retention(key,&policy,now)?;
                println!("{}",serde_json::to_string_pretty(&preview)?);
            }
            "protect-backup" => {
                accepted(&flags,&["--root","--identity","--id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let changed=chunk_store::Store::open(root,key)?.protect_backup(key,id)?;
                println!("Backup {id}: {}",if changed {"protected"}else{"already protected"});
            }
            "unprotect-backup" => {
                accepted(&flags,&["--root","--identity","--id","--confirm-id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let changed=chunk_store::Store::open(root,key)?.unprotect_backup(
                    key,id,required(&flags,"--confirm-id")?)?;
                println!("Backup {id}: {}",if changed {"protection removed"}else{"not protected"});
            }
            "delete-backup" => {
                accepted(&flags,&["--root","--identity","--id","--confirm-id",
                                  "--rebase-dependents"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let store=chunk_store::Store::open(root,key)?;
                let count=store.delete_backup_rebase(key,id,required(&flags,"--confirm-id")?)?;
                println!("Safely rebased {count} immediate descendants and purged manifest {id}. No chunks or parity deleted until separate GC.");
            }
            "gc-preview" | "gc-collect" => {
                accepted(&flags,&["--root","--identity","--grace-hours","--confirm"])?;
                let perform=command=="gc-collect";
                if perform {
                    if required(&flags,"--confirm")?!="ERASE-ORPHANS" {
                        return Err("GC execution requires explicit --confirm ERASE-ORPHANS".into());
                    }
                } else if flags.contains_key("--confirm") {
                    return Err("GC preview must never accept an execution confirmation".into());
                }
                let hours=flags.get("--grace-hours").copied().unwrap_or("48").parse()?;
                let key=Path::new(required(&flags,"--identity")?);
                let report=chunk_store::Store::open(root,key)?
                    .garbage_collect(key,hours,perform)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }
            "info" => {
                accepted(&flags,&["--root","--identity","--id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let info=backup_chain::details(&store,key,required(&flags,"--id")?)?;
                println!("{}",serde_json::to_string_pretty(&info)?);
            }
            "verify-chain" => {
                accepted(&flags,&["--root","--identity","--id","--no-chain"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let full=!flags.contains_key("--no-chain");
                let result=backup_chain::verify_chain(&store,key,required(&flags,"--id")?,full)?;
                println!("{}",serde_json::to_string_pretty(&result)?);
                if !result.healthy{return Err("Incremental chain integrity check failed".into());}
            }
            "relocate" => {
                accepted(&flags,&["--root","--identity","--destination","--dry-run"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let dest=Path::new(required(&flags,"--destination")?);
                let dry=flags.contains_key("--dry-run");
                let report=chunk_store::Store::open(root,key)?.relocate(key,dest,dry)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }
            "export-legacy" => {
                accepted(&flags,&["--root","--identity","--id","--destination"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let count=chunk_store::Store::open(root,key)?.export_legacy(
                    key,required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?))?;
                println!("Stream-exported {count} verified legacy tar.gz.age members");
            }
            "migrate-legacy" => {
                accepted(&flags,&["--root","--identity","--label","--archives-json"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let source_file=std::fs::File::open(required(&flags,"--archives-json")?)?;
                let values:Vec<serde_json::Value>=serde_json::from_reader(source_file)?;
                if values.is_empty(){return Err("Legacy archive list is empty".into());}
                let sources:Vec<chunk_store::LegacySource>=values.iter().map(|v|{
                    Ok::<_,Box<dyn std::error::Error+Send+Sync>>(chunk_store::LegacySource {
                        namespace:v["namespace"].as_str().ok_or("Missing namespace")?.into(),
                        archive:v["archive"].as_str().ok_or("Missing archive path")?.into()
                    })
                }).collect::<Result<_,_>>()?;
                let store=chunk_store::Store::open(root,key)?;
                let backup=store.import_legacy(key,required(&flags,"--label")?,&sources)?;
                println!("Migrated {} files into unified logical backup {}",
                    backup.entries.len(),backup.id);
            }
            "export-zip" => {
                accepted(&flags,&["--root","--identity","--id","--destination","--volume-mib"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let mib=flags.get("--volume-mib").copied().unwrap_or("2048").parse()?;
                let result=portable::export(&store,key,required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?),mib)?;
                println!("Created {} portable encrypted ZIP64 volumes",result.len());
            }
            "import-zip" => {
                accepted(&flags,&["--root","--identity","--bundle"])?;
                let id=portable::import(Path::new(required(&flags,"--bundle")?),
                    Path::new(required(&flags,"--identity")?),root)?;
                println!("Imported and verified logical backup {id} into new storage");
            }
            "verify" => {
                accepted(&flags, &["--root","--identity","--id"])?;
                let identity = Path::new(required(&flags,"--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let report = store.verify(identity, required(&flags,"--id")?)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
                if !report.healthy {return Err("Backup integrity or parity verification failed".into());}
            }
            "protect" | "repair" => {
                accepted(&flags, &["--root","--identity","--id"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let id = required(&flags, "--id")?;
                let count = if command == "protect" {store.protect(identity,id)?}
                            else {store.repair(identity,id)?};
                println!("{}: {} stripes/chunks processed",command,count);
            }
            "restore" => {
                accepted(&flags, &["--root","--identity","--id","--selection","--destination"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let count = chunk_store::Store::open(root, identity)?.restore(
                    identity,required(&flags,"--id")?,
                    flags.get("--selection").copied(),
                    Path::new(required(&flags,"--destination")?))?;
                println!("Restored {} verified files to a fresh directory",count);
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }

    if family == "updates" {
        if password_stdin { return Err("No credentials accepted for update checks".into()); }
        accepted(&flags, if command == "stage" {
            &["--channel", "--directory"] } else { &["--channel"] })?;
        let channel = required(&flags, "--channel")?;
        let info = updater::get_release(channel)?;
        match command {
            "check" => {
                if json { println!("{}", serde_json::to_string_pretty(&info)?); }
                else {
                    println!("Native Rust release: {} ({})", info.tag, info.channel);
                    println!("Matching verified asset: {}",
                        info.asset.as_ref().map_or("unavailable", |a|a.name.as_str()));
                }
            }
            "stage" => {
                let directory = Path::new(required(&flags, "--directory")?);
                let file = updater::stage(&info, directory)?;
                println!("SHA-256 verified native update staged; NOT installed: {}", file.display());
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "vault" {
        if json { return Err("--json is not supported for vault operations".into()); }
        let count = match command {
            "verify" | "create" => 1,
            "export" | "import" | "change-password" => 2,
            _ => return Err(usage().into()),
        };
        if command == "export" || command == "import" {
            accepted(&flags, &["--file", "--destination"])?;
        } else {
            accepted(&flags, &["--file"])?;
        }
        let secrets: Vec<Zeroizing<String>> = if password_stdin {
            let mut input = Zeroizing::new(Vec::new());
            io::stdin().take(4096).read_to_end(&mut *input)?;
            let text = std::str::from_utf8(&input)?;
            let lines: Vec<_> = text.lines().collect();
            if lines.len() != count {
                return Err("Expected the required number of passwords on stdin".into());
            }
            lines.iter().map(|line| Zeroizing::new(
                line.trim_end_matches('\r').to_string())).collect()
        } else {
            let mut supplied = Vec::new();
            for index in 0..count {
                let prompt = format!("Vault password {}: ", index + 1);
                supplied.push(Zeroizing::new(rpassword::prompt_password(prompt)?));
            }
            supplied
        };
        let file = Path::new(required(&flags, "--file")?);
        let recipient = match command {
            "verify" => vault::verify(file, &secrets[0])?,
            "create" => vault::create(file, &secrets[0])?,
            "export" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::export_recovery(file, to, &secrets[0], &secrets[1])?
            }
            "import" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::import_recovery(file, to, &secrets[0], &secrets[1])?
            }
            _ => vault::change_password(file, &secrets[0], &secrets[1])?,
        };
        println!("Vault {command} authenticated for recipient {recipient}; private identity not exported");
        return Ok(());
    }
    if password_stdin && family != "archive" && family != "restore" {
        return Err("--password-stdin requires vault or protected archive".into());
    }
    if family == "journal" {
        if password_stdin { return Err("--password-stdin not valid for journals".into()); }
        if !["start", "resume", "record", "finish", "restart", "show"].contains(&command) {
            return Err(usage().into());
        }
        let known = if command == "record" {
            vec!["--directory","--config","--step","--file"]
        } else {
            vec!["--directory","--config"]
        };
        accepted(&flags, &known)?;
        let dir = Path::new(required(&flags, "--directory")?);
        let configuration: serde_json::Value = serde_json::from_reader(
            std::fs::File::open(required(&flags, "--config")?)?)?;
        let start_new = command == "start" || command == "restart";
        let mut state = journal::Journal::start(dir, &configuration,
                                    start_new, !start_new)?;
        match command {
            "record" => state.record(required(&flags, "--step")?,
                                    Path::new(required(&flags, "--file")?))?,
            "finish" => state.finish()?,
            _ => {},
        }
        if json {
            println!("{}", serde_json::to_string_pretty(state.status())?);
        } else {
            println!("Journal {}: cycle={} resumed={} steps={}",
                     command, state.cycle()?, state.resumed,
                     state.status()["steps"].as_object().map_or(0, |x| x.len()));
        }
        return Ok(());
    }
    if family == "checkpoint" && command == "audit" {
        accepted(&flags, &["--file"])?;
        let state = Path::new(required(&flags, "--file")?);
        let audit = checkpoint_audit::audit(state)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&audit)?);
        } else {
            println!("Checkpoint {}: {} verified, {} invalid",
                audit.cycle, audit.valid.len(), audit.invalid.len());
        }
        if !audit.invalid.is_empty() {
            return Err("Checkpoint contains missing or modified archives".into());
        }
        return Ok(());
    }
    if family == "github" && (command == "backup" || command == "resume") {
        accepted(&flags, &["--output-dir", "--recipient", "--max-runs"])?;
        let out = Path::new(required(&flags, "--output-dir")?);
        let recipient = required(&flags, "--recipient")?;
        let limit = flags.get("--max-runs").copied().unwrap_or("5").parse::<usize>()?;
        if limit > 100_000 { return Err("CI run limit exceeds safety ceiling".into()); }
        let result = github_client::backup(out, recipient, command == "resume", limit)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if family == "restore" {
        if json { return Err("Unsupported remote restore JSON flag".into()); }
        if required(&flags, "--confirm")? != "CREATE-NEW" {
            return Err("Remote restore requires explicit --confirm CREATE-NEW".into());
        }
        let plain = flags.get("--identity").copied();
        let protected = flags.get("--vault").copied();
        if plain.is_some() == protected.is_some() {
            return Err("Specify exactly one --identity or --vault".into());
        }
        if password_stdin && protected.is_none() {
            return Err("--password-stdin requires encrypted --vault".into());
        }
        let secret = if let Some(file) = protected {
            let password = if password_stdin {
                let mut buffer = Zeroizing::new(Vec::new());
                io::stdin().take(1026).read_to_end(&mut *buffer)?;
                let line = std::str::from_utf8(&buffer)?;
                Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_owned())
            } else {
                Zeroizing::new(rpassword::prompt_password("Vault password: ")?)
            };
            Some(vault::unlock_identity(Path::new(file), &password)?)
        } else { None };
        let from = Path::new(required(&flags, "--archive")?);
        match command {
            "github" => {
                accepted(&flags, &["--archive", "--identity", "--vault", "--target", "--visibility", "--confirm",
                                    "--source-account","--target-account"])?;
                let target = required(&flags, "--target")?;
                let (target_owner,_)=remote_restore::github_slug(target)?;
                if let Some(expected)=flags.get("--target-account") {
                    if !expected.eq_ignore_ascii_case(target_owner){
                        return Err("GitHub destination account must match new repository owner".into());
                    }
                }
                if let Some(origin)=flags.get("--source-account") {
                    let _plan=account_restore::AccountRestorePlan::prepare(
                        "github",origin,target_owner,Vec::new())?;
                    // The source account is informational. gh login + owner
                    // permissions authorize writes only to the new account.
                }
                let visibility = flags.get("--visibility").copied().unwrap_or("private");
                if visibility != "private" && visibility != "public" {
                    return Err("Visibility must be private or public".into());
                }
                let url = if let Some(secret) = &secret {
                    remote_restore::github_new_repository_from_memory(
                        from, secret, target, visibility == "private")?
                } else {
                    remote_restore::github_new_repository(
                        from, Path::new(plain.unwrap()), target, visibility == "private")?
                };
                println!("New GitHub repository restored: {url}");
            },
            "dropbox" => {
                accepted(&flags, &["--archive", "--identity", "--vault", "--path", "--confirm",
                                    "--source-account","--target-account"])?;
                let path = required(&flags, "--path")?;
                let target_account=flags.get("--target-account").copied();
                if let Some(origin)=flags.get("--source-account") {
                    let destination=target_account.ok_or(
                        "Cross-user Dropbox restore requires explicit --target-account")?;
                    let _plan=account_restore::AccountRestorePlan::prepare(
                        "dropbox",origin,destination,Vec::new())?;
                }
                let result = if let Some(secret) = &secret {
                    if let Some(destination)=target_account {
                        remote_restore::dropbox_new_file_for_account_from_memory(
                            from,secret,path,destination)?
                    }else {
                        remote_restore::dropbox_new_file_from_memory(from,secret,path)?
                    }
                } else {
                    if let Some(destination)=target_account {
                        remote_restore::dropbox_new_file_for_account(
                            from,Path::new(plain.unwrap()),path,destination)?
                    }else{
                        remote_restore::dropbox_new_file(from,Path::new(plain.unwrap()),path)?
                    }
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "dropbox" && (command == "backup" || command == "resume") {
        accepted(&flags, &["--output-dir", "--recipient", "--remote-path"])?;
        let out = Path::new(required(&flags, "--output-dir")?);
        let recipient = required(&flags, "--recipient")?;
        let remote = flags.get("--remote-path").copied().unwrap_or("");
        let result = dropbox_client::backup(out, remote, recipient, command == "resume")?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if family == "dropbox" && command == "hash" {
        if json { return Err("--json not valid for dropbox hash".into()); }
        accepted(&flags, &["--file"])?;
        let file = Path::new(required(&flags, "--file")?);
        println!("{}", dropbox_hash::hash_file(file)?);
        return Ok(());
    }
    if family == "archive" && (command == "protect" || command == "unprotect") {
        accepted(&flags,&["--root","--archive","--confirm"])?;
        if password_stdin {return Err("Archive protection uses no plaintext password input".into());}
        let root=Path::new(required(&flags,"--root")?);
        let file=Path::new(required(&flags,"--archive")?);
        let result=if command=="protect" {
            if flags.contains_key("--confirm"){return Err("--confirm not used for protection".into());}
            archive_management::protect(root,file)?
        }else{
            archive_management::unprotect(root,file,required(&flags,"--confirm")?)?
        };
        println!("Legacy archive {}: {}",
            if command=="protect"{"protection"}else{"unprotection"},
            if result {"changed"}else{"already in requested state"});
        return Ok(());
    }
    if family == "archive" && command == "delete" {
        accepted(&flags, &["--root", "--archive", "--confirm"])?;
        if password_stdin { return Err("Password input not used for local deletion".into()); }
        let root = Path::new(required(&flags, "--root")?);
        let file = Path::new(required(&flags, "--archive")?);
        let confirmation = required(&flags, "--confirm")?;
        let removed = archive_management::delete(root, file, confirmation, false)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&removed)?);
        } else {
            println!("Deleted {} local files with durable audit log", removed.len());
        }
        return Ok(());
    }
    if family != "archive" { return Err(usage().into()); }
    match command {
        "create" => {
            accepted(&flags, &["--source", "--output", "--recipient", "--prefix"])?;
            if password_stdin { return Err("No password accepted for archive creation".into()); }
            if json { return Err("--json not valid for create".into()); }
            let source = Path::new(required(&flags, "--source")?);
            let output = Path::new(required(&flags, "--output")?);
            let recipient = required(&flags, "--recipient")?;
            let prefix = flags.get("--prefix").copied().unwrap_or("multi-backup");
            let count = archive::create(source, output, recipient, prefix)?;
            println!("Created {count} verified input entries in encrypted archive {}", output.display());
        },
        "list" | "verify" | "extract" => {
            let known = if command == "extract" {
                vec!["--archive", "--identity", "--vault", "--destination"]
            } else {
                vec!["--archive", "--identity", "--vault"]
            };
            accepted(&flags, &known)?;
            if json && command != "list" {
                return Err("--json valid only for list".into());
            }
            let path = Path::new(required(&flags, "--archive")?);
            let plain = flags.get("--identity").copied();
            let protected = flags.get("--vault").copied();
            if plain.is_some() == protected.is_some() {
                return Err("Specify exactly one --identity or --vault".into());
            }
            if password_stdin && protected.is_none() {
                return Err("--password-stdin requires an encrypted --vault".into());
            }
            let unlocked = if let Some(vault_file) = protected {
                let password = if password_stdin {
                    let mut bytes = Zeroizing::new(Vec::new());
                    io::stdin().take(1026).read_to_end(&mut *bytes)?;
                    let line = std::str::from_utf8(&bytes)?;
                    Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_string())
                } else {
                    Zeroizing::new(rpassword::prompt_password("Vault password: ")?)
                };
                Some(vault::unlock_identity(Path::new(vault_file), &password)?)
            } else { None };
            let plain_path = plain.map(Path::new);
            match command {
                "list" => {
                    let members = if let Some(secret) = &unlocked {
                        archive::list_from_memory(path, secret)?
                    } else {
                        archive::list(path, plain_path.unwrap())?
                    };
                    if json {
                        println!("{}", serde_json::to_string_pretty(&members)?);
                    } else {
                        for item in members {
                            println!("{}\t{}\t{}",
                                if item.directory { "dir" } else { "file" },
                                item.bytes, item.path);
                        }
                    }
                }
                "verify" => {
                    let count = if let Some(secret) = &unlocked {
                        archive::verify_from_memory(path, secret)?
                    } else {
                        archive::verify(path, plain_path.unwrap())?
                    };
                    println!("Verified {count} members, encrypted stream integrity checked");
                }
                _ => {
                    let destination = Path::new(required(&flags, "--destination")?);
                    let members = if let Some(secret) = &unlocked {
                        archive::extract_from_memory(path, secret, destination)?
                    } else {
                        archive::extract(path, plain_path.unwrap(), destination)?
                    };
                    println!("Restored {} members safely to {}", members.len(), destination.display());
                }
            }
        },
        _ => return Err(usage().into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Multi Backup native archive error: {error}");
        process::exit(2);
    }
}

#[cfg(test)]
mod flag_tests{
    use super::*;
    #[test]
    fn dry_run_is_value_free_and_rejects_duplicate(){
        let words=vec!["--root".into(),"/tmp/data".into(),"--dry-run".into()];
        let (flags,json,password)=options(&words).unwrap();
        assert_eq!(flags.get("--dry-run"),Some(&"true"));
        assert!(!json&&!password);
        let repeated=vec!["--dry-run".into(),"--dry-run".into()];
        assert!(options(&repeated).is_err());
    }
}
