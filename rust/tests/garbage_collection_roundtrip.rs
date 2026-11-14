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
    fs::OpenOptions::new().write(true).open(path).unwrap()
        .set_modified(timestamp).unwrap();
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
    let report=store.garbage_collect(&key,48,true).unwrap();
    assert_eq!(report.deleted_chunks,0);
    assert_eq!(report.deleted_stripes,0);
    assert!(report.blocked_chunks_by_shared_parity>0);
    assert!(chunk(&root,&abandoned).exists());
    assert!(store.verify(&key,&second.id).unwrap().healthy);
}
