//! End-to-end encrypted chunk deduplication and parity fault injection.
//! Requires age and age-keygen installed (provided by the Production Gate).
use multi_backup_core::chunk_store::{Source,Store,DEFAULT_CHUNK};
use std::{fs,process::Command};
use tempfile::tempdir;

fn credentials(dir:&std::path::Path)->Option<(String,std::path::PathBuf)> {
    if Command::new("age").arg("--version").output().is_err(){return None;}
    let key=dir.join("identity.txt");
    let status=Command::new("age-keygen").arg("-o").arg(&key).status().ok()?;
    if !status.success(){return None;}
    let text=fs::read_to_string(&key).ok()?;
    let recipient=text.lines().find_map(|line|
        line.strip_prefix("# public key: ").map(str::trim))?.to_owned();
    Some((recipient,key))
}
#[test]
fn logical_backup_dedup_and_corruption_recovery() {
    let workspace=tempdir().unwrap();
    let Some((recipient,identity))=credentials(workspace.path()) else {return};
    let root=workspace.path().join("backup");
    let github=workspace.path().join("source-repository");
    let ci=workspace.path().join("source-ci");
    fs::create_dir(&github).unwrap();fs::create_dir(&ci).unwrap();
    fs::write(github.join("README.txt"),b"Backup repository contents").unwrap();
    fs::write(ci.join("run.json"),b"GitHub Actions metadata").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&identity).unwrap();
    let sources=vec![
        Source{namespace:"github".into(),directory:github.clone()},
        Source{namespace:"ci".into(),directory:ci.clone()}
    ];
    let first=store.backup(&identity,"one logical cycle",&sources).unwrap();
    assert_eq!(first.entries.len(),2);
    let second=store.backup(&identity,"second logical cycle",&sources).unwrap();
    assert_eq!(store.list(&identity).unwrap().len(),2);
    assert_eq!(first.entries[0].chunks[0].id,second.entries[0].chunks[0].id);
    assert!(store.verify(&identity,&first.id).unwrap().healthy);
    let chunk=&first.entries[0].chunks[0].id;
    let chunk_file=root.join(".multi-backup-v1/chunks")
        .join(&chunk[..2]).join(&chunk[2..4]).join(format!("{chunk}.age"));
    let mut ciphertext=fs::read(&chunk_file).unwrap();
    let midpoint=ciphertext.len()/2;
    ciphertext[midpoint]^=0x80;
    fs::write(&chunk_file,ciphertext).unwrap();
    let damaged=store.verify(&identity,&first.id).unwrap();
    assert!(!damaged.healthy);
    assert!(!damaged.issues.is_empty());
    let target=workspace.path().join("restored");
    assert_eq!(store.restore(&identity,&first.id,None,&target).unwrap(),2);
    assert_eq!(fs::read(target.join("github/README.txt")).unwrap(),b"Backup repository contents");
    assert_eq!(fs::read(target.join("ci/run.json")).unwrap(),b"GitHub Actions metadata");
    assert!(root.join(".multi-backup-v1/quarantine").exists());
    assert!(store.verify(&identity,&first.id).unwrap().healthy);
    assert!(store.restore(&identity,&first.id,None,&target).is_err());
}
#[test]
fn three_missing_data_shards_are_not_silently_restored() {
    let workspace=tempdir().unwrap();
    let Some((recipient,identity))=credentials(workspace.path()) else {return};
    let root=workspace.path().join("data");
    let source=workspace.path().join("original");fs::create_dir(&source).unwrap();
    for i in 0..3 {fs::write(source.join(format!("{i}.txt")),format!("unique {i}")).unwrap();}
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&identity).unwrap();
    let backup=store.backup(&identity,"three items",&[
        Source{namespace:"github".into(),directory:source}]).unwrap();
    for item in &backup.entries {
        let id=&item.chunks[0].id;
        fs::remove_file(root.join(".multi-backup-v1/chunks").join(&id[..2])
            .join(&id[2..4]).join(format!("{id}.age"))).unwrap();
    }
    assert!(store.restore(&identity,&backup.id,None,&workspace.path().join("not-restored")).is_err());
}

#[test]
fn zip64_portable_roundtrip_and_tamper_detection(){
    let tmp=tempdir().unwrap();
    let Some((recipient,identity))=credentials(tmp.path()) else {return};
    let root=tmp.path().join("source-store");
    let data=tmp.path().join("provider-data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("README.txt"),"Portable verified encrypted logical backup").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&identity).unwrap();
    let snapshot=store.backup(&identity,"portable",&[
        Source{namespace:"github".into(),directory:data}]).unwrap();
    let bundle=tmp.path().join("portable-package");
    let parts=multi_backup_core::portable::export(
        &store,&identity,&snapshot.id,&bundle,1).unwrap();
    assert_eq!(parts.len(),1);
    let imported_root=tmp.path().join("imported-store");
    let imported_id=multi_backup_core::portable::import(&bundle,&identity,&imported_root).unwrap();
    assert_eq!(imported_id,snapshot.id);
    let imported=Store::open(&imported_root,&identity).unwrap();
    assert!(imported.verify(&identity,&imported_id).unwrap().healthy);
    let restored=tmp.path().join("restored-import");
    imported.restore(&identity,&imported_id,None,&restored).unwrap();
    assert_eq!(fs::read(restored.join("github/README.txt")).unwrap(),
        b"Portable verified encrypted logical backup");
    let mut zip=fs::read(&parts[0]).unwrap();
    let half=zip.len()/2; zip[half]^=0x20;
    fs::write(&parts[0],zip).unwrap();
    assert!(multi_backup_core::portable::import(&bundle,&identity,
        &tmp.path().join("tampered-import")).is_err());
}

#[test]
fn one_logical_backup_multi_selection_restores_cross_provider_files(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else{return};
    let store_dir=tmp.path().join("store");let github=tmp.path().join("git");
    let ci=tmp.path().join("ci");let dropbox=tmp.path().join("db");
    for d in [&github,&ci,&dropbox]{fs::create_dir(d).unwrap();}
    fs::write(github.join("git.txt"),b"repo").unwrap();
    fs::write(ci.join("ci.txt"),b"workflow").unwrap();
    fs::write(dropbox.join("file.txt"),b"cloud").unwrap();
    Store::init(&store_dir,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&store_dir,&key).unwrap();
    let backup=store.backup(&key,"unified",&[
        Source{namespace:"github".into(),directory:github},
        Source{namespace:"ci".into(),directory:ci},
        Source{namespace:"dropbox".into(),directory:dropbox}]).unwrap();
    assert_eq!(backup.entries.len(),3);
    let selected=vec!["github".into(),"dropbox/file.txt".into()];
    let dest=tmp.path().join("multi-restore");
    assert_eq!(store.restore_many(&key,&backup.id,&selected,&dest).unwrap(),2);
    assert_eq!(fs::read(dest.join("github/git.txt")).unwrap(),b"repo");
    assert_eq!(fs::read(dest.join("dropbox/file.txt")).unwrap(),b"cloud");
    assert!(!dest.join("ci/ci.txt").exists());
    assert!(store.restore_many(&key,&backup.id,&selected,&dest).is_err());
    let unsafe_selected=vec!["../etc".into()];
    assert!(store.restore_many(&key,&backup.id,&unsafe_selected,
        &tmp.path().join("unsafe")).is_err());
}

#[test]
fn streaming_legacy_age_tar_migration_unifies_independent_archives(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else{return};
    let original_git=tmp.path().join("legacy-git");
    let original_ci=tmp.path().join("legacy-ci");
    fs::create_dir(&original_git).unwrap();fs::create_dir(&original_ci).unwrap();
    fs::write(original_git.join("main.txt"),b"Git archive payload").unwrap();
    fs::write(original_ci.join("run.txt"),b"CI archive payload").unwrap();
    let git_archive=tmp.path().join("git.tar.gz.age");
    let ci_archive=tmp.path().join("ci.tar.gz.age");
    multi_backup_core::archive::create(&original_git,&git_archive,&recipient,"legacy").unwrap();
    multi_backup_core::archive::create(&original_ci,&ci_archive,&recipient,"legacy").unwrap();
    let destination=tmp.path().join("new-store");
    Store::init(&destination,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&destination,&key).unwrap();
    let logical=store.import_legacy(&key,"combined legacy cycle",&[
        multi_backup_core::chunk_store::LegacySource{namespace:"github".into(),archive:git_archive.clone()},
        multi_backup_core::chunk_store::LegacySource{namespace:"ci".into(),archive:ci_archive.clone()}
    ]).unwrap();
    assert_eq!(logical.entries.len(),2);
    assert!(store.verify(&key,&logical.id).unwrap().healthy);
    let fresh=tmp.path().join("migrated-restore");
    store.restore(&key,&logical.id,None,&fresh).unwrap();
    assert_eq!(fs::read(fresh.join("github/legacy/main.txt")).unwrap(),b"Git archive payload");
    assert_eq!(fs::read(fresh.join("ci/legacy/run.txt")).unwrap(),b"CI archive payload");
    assert!(git_archive.exists()&&ci_archive.exists());
}

#[test]
fn encrypted_store_relocation_is_atomic_and_nondestructive(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else {return};
    let source=tmp.path().join("source-storage");
    let input=tmp.path().join("source-files");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("recover.txt"),b"content-addressed relocation data").unwrap();
    Store::init(&source,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&source,&key).unwrap();
    let snap=store.backup(&key,"copy cycle",&[Source {
        namespace:"github".into(), directory:input
    }]).unwrap();
    let dest=tmp.path().join("moved-storage");
    let report=store.relocate(&key,&dest,true).unwrap();
    assert!(report.dry_run&&!report.verified&&report.ciphertext_bytes>0);
    assert!(!dest.exists());
    let moved=store.relocate(&key,&dest,false).unwrap();
    assert!(moved.verified&&!moved.dry_run);
    assert_eq!(moved.logical_backups,1);
    assert!(source.exists());
    let copied=Store::open(&dest,&key).unwrap();
    assert!(copied.verify(&key,&snap.id).unwrap().healthy);
    let output=tmp.path().join("moved-restore");
    copied.restore(&key,&snap.id,None,&output).unwrap();
    assert_eq!(fs::read(output.join("github/recover.txt")).unwrap(),
        b"content-addressed relocation data");
    assert!(store.relocate(&key,&dest,false).is_err());
}

#[test]
fn streaming_v1_to_legacy_and_back(){
    let temp=tempdir().unwrap();
    let Some((recipient,key))=credentials(temp.path()) else{return};
    let root=temp.path().join("store");let input=temp.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("source.txt"),b"verified export data").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let snapshot=store.backup(&key,"unified",&[
        Source{namespace:"github".into(),directory:input}]).unwrap();
    let exported=temp.path().join("converted.tar.gz.age");
    assert_eq!(store.export_legacy(&key,&snapshot.id,&exported).unwrap(),1);
    assert_eq!(multi_backup_core::archive::verify(&exported,&key).unwrap(),1);
    assert!(store.export_legacy(&key,&snapshot.id,&exported).is_err());
    let newroot=temp.path().join("imported");
    Store::init(&newroot,&recipient,DEFAULT_CHUNK).unwrap();
    let newstore=Store::open(&newroot,&key).unwrap();
    let migrated=newstore.import_legacy(&key,"roundtrip",&[
        multi_backup_core::chunk_store::LegacySource{
            namespace:"legacy".into(),archive:exported
        }]).unwrap();
    let recovered=temp.path().join("recovery");
    newstore.restore(&key,&migrated.id,None,&recovered).unwrap();
    assert_eq!(fs::read(recovered.join("legacy/multi-backup/github/source.txt")).unwrap(),
        b"verified export data");
}


#[test]
fn user_protected_backup_blocks_delete_and_survives_portable_roundtrip(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else{return};
    let root=tmp.path().join("protected-store");
    let data=tmp.path().join("data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("safe.txt"),b"protected and shared").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let sources=[Source{namespace:"secure/data".into(),directory:data}];
    // Legacy namespace support is single component; this test uses the
    // current explicitly supported short logical source name.
    let sources=[Source{namespace:"secure".into(),directory:sources[0].directory.clone()}];
    let protected=store.backup(&key,"hold", &sources).unwrap();
    let shared=store.backup(&key,"other", &sources).unwrap();
    assert_eq!(protected.entries[0].chunks[0].id,shared.entries[0].chunks[0].id);
    assert!(store.protect_backup(&key,&protected.id).unwrap());
    assert!(!store.protect_backup(&key,&protected.id).unwrap());
    assert!(store.is_backup_protected(&key,&protected.id).unwrap());
    assert!(store.list(&key).unwrap().iter().find(|v|v.id==protected.id).unwrap().protected);
    assert!(store.delete_backup(&key,&protected.id,&protected.id).is_err());
    assert!(store.unprotect_backup(&key,&protected.id,&shared.id).is_err());
    assert!(store.is_backup_protected(&key,&protected.id).unwrap());
    let bundle=tmp.path().join("bundle");
    multi_backup_core::portable::export(&store,&key,&protected.id,&bundle,2).unwrap();
    let target=tmp.path().join("imported");
    let imported=multi_backup_core::portable::import(&bundle,&key,&target).unwrap();
    let copy=Store::open(&target,&key).unwrap();
    assert_eq!(imported,protected.id);
    assert!(copy.is_backup_protected(&key,&imported).unwrap());
    assert!(copy.delete_backup(&key,&imported,&imported).is_err());
    assert!(store.unprotect_backup(&key,&protected.id,&protected.id).unwrap());
    assert!(!store.is_backup_protected(&key,&protected.id).unwrap());
    store.delete_backup(&key,&protected.id,&protected.id).unwrap();
    assert!(store.load(&key,&protected.id).is_err());
    assert!(store.verify(&key,&shared.id).unwrap().healthy);
    let destination=tmp.path().join("restored");
    assert_eq!(store.restore(&key,&shared.id,None,&destination).unwrap(),1);
}
#[test]
fn protected_marker_corruption_or_missing_confirmation_fails_closed(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else{return};
    let root=tmp.path().join("teststore");
    let data=tmp.path().join("input");
    fs::create_dir(&data).unwrap();fs::write(data.join("a"),b"1").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let backup=store.backup(&key,"test",&[Source{namespace:"test".into(),directory:data}]).unwrap();
    assert!(store.protect_backup(&key,&backup.id).unwrap());
    let marker=root.join(".multi-backup-v1/protected")
        .join(format!("{}.json.age",backup.id));
    fs::write(marker,b"invalid encrypted marker").unwrap();
    assert!(store.is_backup_protected(&key,&backup.id).is_err());
    assert!(store.delete_backup(&key,&backup.id,&backup.id).is_err());
    assert!(store.load(&key,&backup.id).is_err());
}


#[test]
fn nested_top_namespace_and_authenticated_expiration_respect_backup_hold(){
    let tmp=tempdir().unwrap();
    let Some((recipient,key))=credentials(tmp.path()) else{return};
    let root=tmp.path().join("nested-store");
    let dir=tmp.path().join("inputs");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("config.txt"),b"config snapshot").unwrap();
    Store::init(&root,&recipient,DEFAULT_CHUNK).unwrap();
    let store=Store::open(&root,&key).unwrap();
    let sources=[Source{namespace:"kubernetes/config".into(),directory:dir}];
    assert!(store.backup_in_namespace(&key,"bad","../secret",&sources).is_err());
    let held=store.backup_in_namespace(&key,"live","Roma1/Cluster/A",
                                        &sources).unwrap();
    assert_eq!(held.top_namespace,"Roma1/Cluster/A");
    assert!(held.created_unix>0);
    assert_eq!(held.entries[0].path,
        "Roma1/Cluster/A/kubernetes/config/config.txt");
    store.protect_backup(&key,&held.id).unwrap();
    let default=multi_backup_core::backup_policy::Retention::default();
    let future=held.created_unix+31*86400;
    let check=store.preview_retention(&key,&default,future).unwrap();
    assert!(check.expire.is_empty());
    assert_eq!(check.preserve[&held.id],"explicit_backup_protection");
    let recovered=tmp.path().join("recovered");
    store.restore(&key,&held.id,None,&recovered).unwrap();
    assert_eq!(fs::read(recovered.join(&held.entries[0].path)).unwrap(),
               b"config snapshot");
}
