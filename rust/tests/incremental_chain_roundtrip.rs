//! End-to-end full/incremental chain acceptance with real age ciphertext.
//! Snapshots contain complete file inventories; only unique chunks are stored.
use multi_backup_core::{
    backup_chain,
    chunk_store::{BackupKind, Source, Store, DEFAULT_CHUNK},
};
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

fn credentials(root:&Path)->(String,std::path::PathBuf){
    let key=root.join("identity.txt");
    let out=Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("age-keygen required for native encrypted backup acceptance");
    assert!(out.status.success(),"age-keygen: {}",String::from_utf8_lossy(&out.stderr));
    let raw=fs::read_to_string(&key).unwrap();
    let recipient=raw.lines().find_map(|line|line.strip_prefix("# public key: "))
        .expect("age-keygen recipient").to_owned();
    (recipient,key)
}

#[test]
fn full_incremental_verify_details_and_delete_rebase_without_data_loss(){
    let tmp=tempdir().unwrap();
    let (recipient,key)=credentials(tmp.path());
    let root=tmp.path().join("store");
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let source=tmp.path().join("data");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("shared.bin"),b"unchanged content").unwrap();
    fs::write(source.join("changed.bin"),b"old content").unwrap();
    fs::write(source.join("removed.bin"),b"deleted later").unwrap();
    let sources=[Source{namespace:"documents".into(),directory:source.clone()}];
    let full=store.backup_in_namespace_mode(&key,"first","team",&sources,
        BackupKind::Full,None).unwrap();
    assert_eq!(full.kind,BackupKind::Full);
    assert!(full.parent_id.is_none());

    fs::write(source.join("changed.bin"),b"new content").unwrap();
    fs::remove_file(source.join("removed.bin")).unwrap();
    fs::write(source.join("added.bin"),b"new file").unwrap();
    let first=store.backup_in_namespace_mode(&key,"increment 1","team",&sources,
        BackupKind::Incremental,Some(&full.id)).unwrap();
    assert_eq!(first.parent_id.as_deref(),Some(full.id.as_str()));
    let info=backup_chain::details(&store,&key,&first.id).unwrap();
    assert_eq!(info.full_reference_id,full.id);
    assert_eq!(info.chain.len(),2);
    assert_eq!(info.snapshot.file_count,3);
    assert_eq!(info.snapshot.added_or_modified_files,2);
    assert_eq!(info.snapshot.deleted_files,1);
    assert!(info.snapshot.uncompressed_bytes>0);
    assert!(info.snapshot.compressed_bytes>=info.snapshot.deduplicated_compressed_bytes);
    assert!(info.snapshot.incremental_new_compressed_bytes>0);
    let all=backup_chain::verify_chain(&store,&key,&first.id,true).unwrap();
    assert!(all.healthy);
    assert_eq!(all.verified_ids,vec![full.id.clone(),first.id.clone()]);
    let only=backup_chain::verify_chain(&store,&key,&first.id,false).unwrap();
    assert!(only.healthy);
    assert_eq!(only.verified_ids,vec![first.id.clone()]);

    fs::write(source.join("added.bin"),b"new third generation").unwrap();
    let second=store.backup_in_namespace_mode(&key,"increment 2","team",&sources,
        BackupKind::Incremental,Some(&first.id)).unwrap();
    assert_eq!(backup_chain::details(&store,&key,&second.id).unwrap().chain.len(),3);
    assert!(store.delete_backup(&key,&full.id,&full.id).is_err());

    // User approval is required for each unprotect/delete operation.
    store.unprotect_backup(&key,&first.id,&first.id).unwrap();
    assert!(store.delete_backup(&key,&first.id,&first.id).is_err());
    let moved=store.delete_backup_rebase(&key,&first.id,&first.id).unwrap();
    assert_eq!(moved,1);
    assert!(store.load(&key,&first.id).is_err());
    assert_eq!(store.load(&key,&second.id).unwrap().parent_id.as_deref(),Some(full.id.as_str()));
    assert!(backup_chain::verify_chain(&store,&key,&second.id,true).unwrap().healthy);

    store.unprotect_backup(&key,&full.id,&full.id).unwrap();
    assert_eq!(store.delete_backup_rebase(&key,&full.id,&full.id).unwrap(),1);
    let rebased=store.load(&key,&second.id).unwrap();
    assert_eq!(rebased.kind,BackupKind::Full);
    assert!(rebased.parent_id.is_none());
    assert_eq!(backup_chain::details(&store,&key,&second.id).unwrap().chain.len(),1);
    assert!(backup_chain::verify_chain(&store,&key,&second.id,true).unwrap().healthy);
    let destination=tmp.path().join("restored");
    store.restore(&key,&second.id,None,&destination).unwrap();
    assert_eq!(fs::read(destination.join("team/documents/added.bin")).unwrap(),
        b"new third generation");
    assert!(!destination.join("team/documents/removed.bin").exists());
}

#[test]
fn incremental_requires_a_real_verified_parent_and_same_namespace(){
    let tmp=tempdir().unwrap();
    let (recipient,key)=credentials(tmp.path());
    let root=tmp.path().join("store");
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let data=tmp.path().join("data");fs::create_dir(&data).unwrap();
    fs::write(data.join("data.txt"),b"some data").unwrap();
    let sources=[Source{namespace:"files".into(),directory:data}];
    let full=store.backup(&key,"full",&sources).unwrap();
    assert!(store.backup_in_namespace_mode(&key,"missing","",&sources,
        BackupKind::Incremental,None).is_err());
    assert!(store.backup_in_namespace_mode(&key,"mismatch","other",&sources,
        BackupKind::Incremental,Some(&full.id)).is_err());
    assert!(store.backup_in_namespace_mode(&key,"invalid","",&sources,
        BackupKind::Incremental,Some("deadbeef")).is_err());
    assert!(store.backup_in_namespace_mode(&key,"not full","",&sources,
        BackupKind::Full,Some(&full.id)).is_err());
}