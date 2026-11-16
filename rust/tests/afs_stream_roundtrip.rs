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

    // Agentless SSH restore: no real network is used, but the transport must
    // execute only the fixed quote-escaped vos command and require pinned key.
    let ssh=fakedir.join("ssh");
    fs::write(&ssh,r#"#!/bin/sh
set -eu
case " $* " in
  *" -F none "*" StrictHostKeyChecking=yes "*" ProxyCommand=none "*) : ;;
  *) exit 51 ;;
esac
for arg in "$@"; do remote="$arg"; done
exec /bin/sh -c "$remote"
"#).unwrap();
    fs::set_permissions(&ssh,fs::Permissions::from_mode(0o700)).unwrap();
    let sshkey=root.join("ssh-key");
    let known=root.join("known-hosts");
    fs::write(&sshkey,b"fake-public-key-for-test-only").unwrap();
    fs::write(&known,b"fake-pinned-host-for-test-only").unwrap();
    let remote_target=root.join("remote-target.json");
    fs::write(&remote_target,serde_json::to_vec(&json!({
        "cell":"example.org","volume":"new.remote",
        "server":"afs.example.org","partition":"/vicepa",
        "auth":{"type":"existing_token"},
        "execution":{"type":"ssh_agentless","host":"pinned.example.org",
           "user":"afsrestore","identity":sshkey,
           "known_hosts":known,"afs_server":false}
    })).unwrap()).unwrap();
    let remote_plan=cli(&["plugins","afs-restore-plan","--target-json",
        string(&remote_target),"--json"],&fakedir,&root.join("remote-restored"),true);
    let p:Value=serde_json::from_slice(&remote_plan.stdout).unwrap();
    assert_eq!(p["transport"],"ssh-agentless");
    let output=root.join("remote-restored");
    cli(&["plugins","afs-restore-new","--target-json",string(&remote_target),
        "--root",string(&storepath),"--identity",string(&key),
        "--id",current,"--confirm-target",
        "example.org:afs.example.org:/vicepa:new.remote","--json"],
        &fakedir,&output,true);
    assert_eq!(fs::read(&output).unwrap(),
        b"full-AFS-payloadincremental-AFS-payload");
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


/// Stateful virtual AFS cell: the fake vos process persists restored volumes
/// and honors dump full/-time and restore abort/incremental semantics. The
/// compiled Rust CLI, age, zstd, manifest chain and parity remain real.
#[test]
fn afs_virtual_cell_refuses_overwrite_and_partial_dump(){
    let temp=tempdir().unwrap();
    let root=temp.path();
    let cell=root.join("cell");
    fs::create_dir_all(cell.join("source")).unwrap();
    fs::create_dir_all(cell.join("restored")).unwrap();
    let fakedir=root.join("bin");
    fs::create_dir(&fakedir).unwrap();
    let vos=fakedir.join("vos");
    fs::write(&vos,r#"#!/bin/sh
set -eu
mode=$1; shift
volume=''; overwrite=''; increment='no'
while [ "$#" -gt 0 ]; do
  case "$1" in
    -id|-name) volume=$2; shift 2;;
    -overwrite) overwrite=$2; shift 2;;
    -time) increment=yes; shift 2;;
    -cell|-server|-partition) shift 2;;
    *) exit 82;;
  esac
done
case "$volume" in ''|*[!A-Za-z0-9._-]*) exit 83;; esac
case "$mode" in
  dump)
    if [ "$AFS_TEST_FAIL_DUMP" = 1 ]; then printf 'partial-dump'; exit 71; fi
    if [ "$increment" = yes ]; then cat "$AFS_TEST_CELL/source/$volume.delta";
    else cat "$AFS_TEST_CELL/source/$volume.full"; fi;;
  restore)
    dest="$AFS_TEST_CELL/restored/$volume.dat"
    case "$overwrite" in
      abort)
        if [ -e "$dest" ]; then exit 73; fi
        ( set -C; cat > "$dest" ) || exit 74;;
      incremental)
        if [ ! -f "$dest" ]; then exit 75; fi
        cat >> "$dest";;
      *) exit 76;;
    esac;;
  *) exit 77;;
esac
"#).unwrap();
    fs::set_permissions(&vos,fs::Permissions::from_mode(0o700)).unwrap();
    let mut full=b"AFS full\0\xff".to_vec();
    for _ in 0..512 {full.extend(0u8..=255);}
    let delta=b"\0AFS-increment\xff".repeat(100);
    fs::write(cell.join("source/user.original.full"),&full).unwrap();
    fs::write(cell.join("source/user.original.delta"),&delta).unwrap();
    fs::write(cell.join("restored/production.volume.dat"),b"original-production").unwrap();
    let key=root.join("age-identity");
    let result=Command::new("age-keygen").args(["-o",string(&key)])
        .output().unwrap();
    assert!(result.status.success());
    let raw=fs::read_to_string(&key).unwrap();
    let recipient=raw.lines().find_map(|x|x.strip_prefix("# public key: "))
        .unwrap().trim();
    let storepath=root.join("store");
    Store::init(&storepath,recipient,DEFAULT_CHUNK).unwrap();
    let jobfile=root.join("job.json");
    let targetfile=root.join("target.json");
    fs::write(&targetfile,serde_json::to_vec(&json!({
        "cell":"example.org","volume":"new.recovered",
        "server":"afs.example.org","partition":"/vicepa",
        "auth":{"type":"existing_token"}
    })).unwrap()).unwrap();
    let exec=|args:&[&str],fail:bool|->Output{
        let path=std::env::var("PATH").unwrap_or_default();
        Command::new(env!("CARGO_BIN_EXE_multi-backup-native"))
            .env("PATH",format!("{}:{path}",fakedir.display()))
            .env("AFS_TEST_CELL",&cell)
            .env("AFS_TEST_FAIL_DUMP",if fail {"1"} else {"0"})
            .args(args).output().unwrap()
    };
    let job=json!({"id":"volume-job","cell":"example.org","volume":"user.original",
        "auth":{"type":"existing_token"},"execution":{"type":"local"},
        "mode":{"kind":"full"}});
    fs::write(&jobfile,serde_json::to_vec(&job).unwrap()).unwrap();
    let backup_args=["plugins","afs-backup","--job-json",string(&jobfile),
        "--root",string(&storepath),"--identity",string(&key),
        "--label","virtual-afs","--json"];
    let result=exec(&backup_args,false);
    assert!(result.status.success(),"full: {}",String::from_utf8_lossy(&result.stderr));
    let fullid:Value=serde_json::from_slice(&result.stdout).unwrap();
    let id=fullid["id"].as_str().unwrap().to_owned();
    let store=Store::open(&storepath,&key).unwrap();
    assert!(store.verify(&key,&id).unwrap().healthy);
    let mut incr=job.clone();
    incr["mode"]=json!({"kind":"incremental",
        "since":"11/15/2026 01:00","parent_backup_id":id});
    fs::write(&jobfile,serde_json::to_vec(&incr).unwrap()).unwrap();
    let result=exec(&backup_args,false);
    assert!(result.status.success(),"incremental: {}",String::from_utf8_lossy(&result.stderr));
    let value:Value=serde_json::from_slice(&result.stdout).unwrap();
    let current=value["id"].as_str().unwrap().to_owned();
    assert!(store.verify(&key,&current).unwrap().healthy);
    let restore_args=["plugins","afs-restore-new","--target-json",string(&targetfile),
        "--root",string(&storepath),"--identity",string(&key),
        "--id",&current,"--confirm-target",
        "example.org:afs.example.org:/vicepa:new.recovered","--json"];
    let wrong=exec(&["plugins","afs-restore-new","--target-json",string(&targetfile),
        "--root",string(&storepath),"--identity",string(&key),
        "--id",&current,"--confirm-target","wrong","--json"],false);
    assert!(!wrong.status.success());
    assert!(!cell.join("restored/new.recovered.dat").exists());
    let result=exec(&restore_args,false);
    assert!(result.status.success(),"restore: {}",String::from_utf8_lossy(&result.stderr));
    let restored:Value=serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(restored["restored_dumps"],2);
    full.extend_from_slice(&delta);
    assert_eq!(fs::read(cell.join("restored/new.recovered.dat")).unwrap(),full);
    assert!(!exec(&restore_args,false).status.success());
    assert_eq!(fs::read(cell.join("restored/new.recovered.dat")).unwrap(),full);
    assert_eq!(fs::read(cell.join("restored/production.volume.dat")).unwrap(),
        b"original-production");
    fs::write(&jobfile,serde_json::to_vec(&job).unwrap()).unwrap();
    assert!(!exec(&backup_args,true).status.success());
    assert_eq!(store.list(&key).unwrap().len(),2);
    assert!(store.verify(&key,&id).unwrap().healthy);
    assert!(store.verify(&key,&current).unwrap().healthy);
}
