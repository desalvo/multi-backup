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
    let chunk=&first.entries[0].chunks[0].id;
    let chunk_file=root.join(".multi-backup-v1/chunks")
        .join(&chunk[..2]).join(&chunk[2..4]).join(format!("{chunk}.age"));
    let mut ciphertext=fs::read(&chunk_file).unwrap();
    ciphertext[ciphertext.len()/2]^=0x80;
    fs::write(&chunk_file,ciphertext).unwrap();
    let target=workspace.path().join("restored");
    assert_eq!(store.restore(&identity,&first.id,None,&target).unwrap(),2);
    assert_eq!(fs::read(target.join("github/README.txt")).unwrap(),b"Backup repository contents");
    assert_eq!(fs::read(target.join("ci/run.json")).unwrap(),b"GitHub Actions metadata");
    assert!(root.join(".multi-backup-v1/quarantine").exists());
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
