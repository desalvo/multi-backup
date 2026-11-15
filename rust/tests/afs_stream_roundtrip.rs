//! Real compiled Rust CLI + age ciphertext + fake vos process acceptance.
//! The fake executable verifies subprocess streaming without requiring a live
//! privileged AFS cell. LIVE CELL restore certification is still separate.
#![cfg(unix)]
use multi_backup_core::chunk_store::{BackupKind,Store,DEFAULT_CHUNK};
use serde_json::{json,Value};
use std::{fs,os::unix::fs::PermissionsExt,path::{Path,PathBuf},process::{Command,Output}};
use tempfile::tempdir;
fn string(path:&Path)->&str{path.to_str().unwrap()}
fn cli(args:&[&str],bin:&Path,dump_result:&Path,success:bool)->Output{
    let path=std::env::var("PATH").unwrap_or_default();
    let result=Command::new(env!("CARGO_BIN_EXE_multi-backup-native"))
        .env("PATH",format!("{}:{path}",bin.display()))
        .env("FAKE_VOS_DEST",dump_result)
        .args(args).output().unwrap();
    assert_eq!(result.status.success(),success,
       "CLI {args:?}: stdout={} stderr={}",
       String::from_utf8_lossy(&result.stdout),
       String::from_utf8_lossy(&result.stderr));
    result
}
#[test]
fn afs_full_incremental_encrypted_stream_then_new_volume_restore(){
    let temp=tempdir().unwrap();
    let root=temp.path();
    let key=root.join("key");
    let out=Command::new("age-keygen").args(["-o",string(&key)])
        .output().unwrap();
    assert!(out.status.success());
    let raw=fs::read_to_string(&key).unwrap();
    let recipient=raw.lines().find_map(|l|l.strip_prefix("# public key: "))
        .unwrap().trim().to_owned();
    let storepath=root.join("store");
    Store::init(&storepath,&recipient,DEFAULT_CHUNK).unwrap();
    let fakedir=root.join("fakebin");fs::create_dir(&fakedir).unwrap();
    let vos=fakedir.join("vos");
    fs::write(&vos,r#"#!/bin/sh
set -eu
case "$1" in
  dump)
    case " $* " in
      *" -time "*) printf 'incremental-AFS-payload' ;;
      *) printf 'full-AFS-payload' ;;
    esac
    ;;
  restore)
    case " $* " in
      *" -overwrite abort "*) : ;;
      *" -overwrite incremental "*) : ;;
      *) exit 31 ;;
    esac
    cat >> "$FAKE_VOS_DEST"
    ;;
  *) exit 32 ;;
esac
"#).unwrap();
    fs::set_permissions(&vos,fs::Permissions::from_mode(0o700)).unwrap();
    let target=root.join("target.json");
    fs::write(&target,serde_json::to_vec(&json!({
        "cell":"example.org","volume":"new.recovered",
        "server":"afs.example.org","partition":"/vicepa",
        "auth":{"type":"existing_token"}
    })).unwrap()).unwrap();
    let plan=cli(&["plugins","afs-restore-plan","--target-json",
        string(&target),"--json"],&fakedir,&root.join("restored"),true);
    let decoded:Value=serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(decoded["new_volume_only"],true);
    assert_eq!(decoded["arguments"].as_array().unwrap().last().unwrap(),"abort");
    assert_eq!(decoded["confirmation"],"example.org:afs.example.org:/vicepa:new.recovered");
    let jobfile=root.join("job.json");
    let job=|kind:Value|json!({"id":"afs-job-one","cell":"example.org",
        "volume":"user.original","auth":{"type":"existing_token"},
        "execution":{"type":"local"},"mode":kind});
    fs::write(&jobfile,serde_json::to_vec(&job(json!({"kind":"full"}))).unwrap()).unwrap();
    let full=cli(&["plugins","afs-backup","--job-json",string(&jobfile),
        "--root",string(&storepath),"--identity",string(&key),
        "--label","afs-full","--json"],&fakedir,&root.join("restored"),true);
    let full:Value=serde_json::from_slice(&full.stdout).unwrap();
    let id=full["id"].as_str().unwrap().to_owned();
    let store=Store::open(&storepath,&key).unwrap();
    let backup=store.load(&key,&id).unwrap();
    assert_eq!(backup.kind,BackupKind::Full);
    assert_eq!(backup.entries.len(),1);
    assert_eq!(backup.entries[0].path,"afs/example.org/user.original.vos");
    assert!(store.verify(&key,&id).unwrap().healthy);
    assert!(!storepath.join(".multi-backup-v1").join("plaintext.vos").exists());
    fs::write(&jobfile,serde_json::to_vec(&job(json!({
        "kind":"incremental","since":"11/15/2026 01:00",
        "parent_backup_id":id
    }))).unwrap()).unwrap();
    let increment=cli(&["plugins","afs-backup","--job-json",string(&jobfile),
        "--root",string(&storepath),"--identity",string(&key),
        "--label","afs-inc","--json"],&fakedir,&root.join("restored"),true);
    let increment:Value=serde_json::from_slice(&increment.stdout).unwrap();
    let current=increment["id"].as_str().unwrap();
    assert_eq!(store.load(&key,current).unwrap().kind,BackupKind::Incremental);
    assert_eq!(store.load(&key,current).unwrap().parent_id.as_deref(),Some(id.as_str()));
    let restored=root.join("restored");
    cli(&["plugins","afs-restore-new","--target-json",string(&target),
        "--root",string(&storepath),"--identity",string(&key),
        "--id",current,"--confirm-target","wrong","--json"],
        &fakedir,&restored,false);
    assert!(!restored.exists(),"Bad confirmation must not call vos restore");
    let completed=cli(&["plugins","afs-restore-new","--target-json",string(&target),
        "--root",string(&storepath),"--identity",string(&key),
        "--id",current,"--confirm-target",
        "example.org:afs.example.org:/vicepa:new.recovered","--json"],
        &fakedir,&restored,true);
    let status:Value=serde_json::from_slice(&completed.stdout).unwrap();
    assert_eq!(status["restored_dumps"],2);
    assert_eq!(fs::read(&restored).unwrap(),b"full-AFS-payloadincremental-AFS-payload");
}
#[test]
fn failed_stream_producer_never_commits_a_backup(){
    let temp=tempdir().unwrap();
    let key=temp.path().join("key");
    assert!(Command::new("age-keygen").args(["-o",string(&key)])
        .status().unwrap().success());
    let raw=fs::read_to_string(&key).unwrap();
    let recipient=raw.lines().find_map(|l|l.strip_prefix("# public key: "))
        .unwrap().trim();
    let folder=temp.path().join("store");
    Store::init(&folder,recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&folder,&key).unwrap();
    let mut data=std::io::Cursor::new(b"unverified-incomplete-dump".to_vec());
    let result=store.backup_stream(&key,"failed","",
        "afs/example.org/user.original.vos",&mut data,
        BackupKind::Full,None,||Err("vos failed".into()));
    assert!(result.is_err());
    assert!(store.list(&key).unwrap().is_empty());
}
