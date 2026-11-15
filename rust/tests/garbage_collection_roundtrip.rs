//! Real age ciphertext GC acceptance: no physical deletion at logical purge.
//! Grace period and live parity stripe references must survive every sweep.
use multi_backup_core::chunk_store::{BackupKind,Source,Store,DEFAULT_CHUNK};
use std::{fs, path::{Path,PathBuf}, process::Command, time::{Duration,SystemTime}};
use tempfile::tempdir;

fn credentials(root:&Path)->(String,PathBuf){
    let key=root.join("identity.txt");
    let output=Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("age-keygen required");
    assert!(output.status.success());
    let raw=fs::read_to_string(&key).unwrap();
    (raw.lines().find_map(|s|s.strip_prefix("# public key: "))
        .unwrap().to_string(),key)
}
fn old(path:&Path){
    // Avoid waiting 48 hours for the release-policy grace window.
    let timestamp=SystemTime::now()-Duration::from_secs(72*3600);
    if path.is_dir() {
        fs::File::open(path).unwrap().set_modified(timestamp).unwrap();
    } else {
        fs::OpenOptions::new().write(true).open(path).unwrap()
            .set_modified(timestamp).unwrap();
    }
}
fn chunk(root:&Path,id:&str)->PathBuf{
    root.join(".multi-backup-v1/chunks")
        .join(&id[..2]).join(&id[2..4]).join(format!("{id}.age"))
}
fn stripe_id(root:&Path)->String {
    let folder=root.join(".multi-backup-v1/stripes");
    let list:Vec<_>=fs::read_dir(folder).unwrap().map(|v|v.unwrap()).collect();
    assert_eq!(list.len(),1);
    list[0].file_name().to_str().unwrap()
        .strip_suffix(".json.age").unwrap().to_owned()
}
fn age_stripe(root:&Path,id:&str){
    let private=root.join(".multi-backup-v1");
    old(&private.join("stripes").join(format!("{id}.json.age")));
    let parity=private.join("parity").join(&id[..2]).join(id);
    old(&parity.join("0.age"));
    old(&parity.join("1.age"));
}

#[test]
fn purge_keeps_objects_until_explicit_grace_qualified_gc(){
    let temp=tempdir().unwrap();
    let (recipient,key)=credentials(temp.path());
    let root=temp.path().join("store");
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let input=temp.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("obsolete.txt"),b"payload that will disappear").unwrap();
    let sources=[Source{namespace:"files".into(),directory:input.clone()}];
    let first=store.backup(&key,"full",&sources).unwrap();
    let old_id=first.entries[0].chunks[0].id.clone();
    let old_path=chunk(&root,&old_id);
    let old_stripe=stripe_id(&root);
    fs::remove_file(input.join("obsolete.txt")).unwrap();
    fs::write(input.join("current.txt"),b"completely new payload").unwrap();
    let second=store.backup_in_namespace_mode(&key,"incremental","",&sources,
        BackupKind::Incremental,Some(&first.id)).unwrap();
    // Parity 8+2 and the explicit user-requested backup hold are distinct.
    assert!(store.protect_backup(&key,&second.id).unwrap());
    assert!(store.is_backup_protected(&key,&second.id).unwrap());
    store.unprotect_backup(&key,&first.id,&first.id).unwrap();
    // Existing descendants cannot be silently modified while protected.
    assert!(store.delete_backup_rebase(&key,&first.id,&first.id).is_err());
    store.unprotect_backup(&key,&second.id,&second.id).unwrap();
    assert_eq!(store.delete_backup_rebase(&key,&first.id,&first.id).unwrap(),1);
    assert!(old_path.exists(),"Purge is metadata-only and must preserve chunks");
    assert!(store.load(&key,&first.id).is_err());
    assert!(store.verify(&key,&second.id).unwrap().healthy);
    let before=store.garbage_collect(&key,48,false).unwrap();
    assert_eq!(before.deleted_chunks,0);
    assert_eq!(before.orphan_chunk_candidates,0,"Grace must prevent recent deletion");
    let recent=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(recent.deleted_chunks,0);
    assert!(old_path.exists());
    old(&old_path);
    age_stripe(&root,&old_stripe);
    let preview=store.garbage_collect(&key,48,false).unwrap();
    assert_eq!(preview.orphan_chunk_candidates,1);
    assert_eq!(preview.obsolete_stripe_candidates,1);
    assert!(preview.reclaimable_bytes>0);
    assert!(old_path.exists(),"Preview must never delete files");
    let reclaimed=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(reclaimed.deleted_chunks,1);
    assert_eq!(reclaimed.deleted_stripes,1);
    assert_eq!(reclaimed.reclaimed_bytes,preview.reclaimable_bytes);
    assert!(!old_path.exists());
    assert!(store.verify(&key,&second.id).unwrap().healthy);
    let restore=temp.path().join("restore");
    store.restore(&key,&second.id,None,&restore).unwrap();
    assert_eq!(fs::read(restore.join("files/current.txt")).unwrap(),
        b"completely new payload");
    assert!(!restore.join("files/obsolete.txt").exists());
}

#[test]
fn shared_parity_prevents_unsafe_space_reclamation(){
    let temp=tempdir().unwrap();
    let (recipient,key)=credentials(temp.path());
    let root=temp.path().join("store");
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let input=temp.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("shared.bin"),b"content retained across cycles").unwrap();
    fs::write(input.join("removed.bin"),b"content that is obsolete").unwrap();
    let sources=[Source{namespace:"files".into(),directory:input.clone()}];
    let first=store.backup(&key,"full",&sources).unwrap();
    let old_stripe=stripe_id(&root);
    let abandoned=first.entries.iter().find(|e|e.path.ends_with("removed.bin"))
        .unwrap().chunks[0].id.clone();
    fs::remove_file(input.join("removed.bin")).unwrap();
    let second=store.backup_in_namespace_mode(&key,"incremental","",&sources,
        BackupKind::Incremental,Some(&first.id)).unwrap();
    store.unprotect_backup(&key,&first.id,&first.id).unwrap();
    store.unprotect_backup(&key,&second.id,&second.id).unwrap();
    store.delete_backup_rebase(&key,&first.id,&first.id).unwrap();
    old(&chunk(&root,&abandoned));
    age_stripe(&root,&old_stripe);
    let preview=store.garbage_collect(&key,48,false).unwrap();
    assert_eq!(preview.mixed_stripes_to_repack,1);
    assert_eq!(preview.repacked_stripes,0);
    let report=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(report.repacked_stripes,1);
    assert_eq!(report.deleted_chunks,0);
    assert_eq!(report.deleted_stripes,0);
    assert!(report.blocked_chunks_by_shared_parity>0);
    assert!(chunk(&root,&abandoned).exists());
    assert!(store.verify(&key,&second.id).unwrap().healthy);
    let all:Vec<_>=fs::read_dir(root.join(".multi-backup-v1/stripes")).unwrap()
        .map(|v|v.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(all.len(),2,"Old protection remains for crash recovery");
    let fresh=all.iter().find(|s|!s.starts_with(&old_stripe)).unwrap();
    let fresh_path=root.join(".multi-backup-v1/stripes").join(fresh);
    old(&fresh_path); // Simulate another GC after the committed replacement's grace.
    let status=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(status.deleted_stripes,1);
    assert_eq!(status.deleted_chunks,1);
    assert!(!chunk(&root,&abandoned).exists());
    assert!(store.verify(&key,&second.id).unwrap().healthy);
    let output=temp.path().join("after-repack");
    store.restore(&key,&second.id,None,&output).unwrap();
    assert_eq!(fs::read(output.join("files/shared.bin")).unwrap(),
        b"content retained across cycles");
}

#[cfg(unix)]
#[test]
fn incomplete_parity_repack_preparation_is_ignored_then_garbage_collected(){
    let temp=tempdir().unwrap();
    let (recipient,key)=credentials(temp.path());
    let root=temp.path().join("store");
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let input=temp.path().join("input");fs::create_dir(&input).unwrap();
    fs::write(input.join("file.txt"),b"still protected").unwrap();
    let snapshot=store.backup(&key,"full",&[Source {
        namespace:"files".into(),directory:input
    }]).unwrap();
    // Model a crash between parity preparation and atomic index commit:
    // only orphan encrypted shard bytes exist, without a stripe index.
    let orphan_id="b".repeat(64);
    let orphan=root.join(".multi-backup-v1/parity/bb").join(&orphan_id);
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("0.age"),b"uncommitted encrypted parity").unwrap();
    old(&orphan.join("0.age"));
    // Make the directory itself old after creating its contents.
    old(&orphan);
    assert!(store.verify(&key,&snapshot.id).unwrap().healthy);
    let preview=store.garbage_collect(&key,48,false).unwrap();
    assert_eq!(preview.orphan_parity_directories,1);
    assert!(orphan.exists());
    let report=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(report.deleted_orphan_parity_directories,1);
    assert!(!orphan.exists());
    assert!(store.verify(&key,&snapshot.id).unwrap().healthy);
}
