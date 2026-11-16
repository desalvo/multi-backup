//! Actual compiled Rust CLI vault roundtrip, no external account or network.
use serde_json::Value;
use std::{fs, path::Path, process::{Command,Output}};
use tempfile::tempdir;

fn run(args:&[&str],success:bool)->Output{
    let result=Command::new(env!("CARGO_BIN_EXE_multi-backup-native"))
        .args(args).output().expect("compiled Rust CLI required");
    assert_eq!(result.status.success(),success,
        "args {:?}\nstdout {}\nstderr {}",args,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr));
    result
}
fn strpath(path:&Path)->&str{path.to_str().unwrap()}
#[test]
fn true_age_archive_roundtrip_and_no_overwrite(){
    let temp=tempdir().unwrap();
    let source=temp.path().join("source");
    let vault=temp.path().join("independent-vault");
    let key=temp.path().join("identity");
    let dest=temp.path().join("restored");
    fs::create_dir_all(source.join("sub")).unwrap();
    fs::create_dir(&vault).unwrap();
    let payload=b"Immutability preview \0 \xff \x01";
    fs::write(source.join("sub/file.bin"),payload).unwrap();
    let output=Command::new("age-keygen").arg("-o").arg(&key).output().unwrap();
    assert!(output.status.success());
    let identity=fs::read_to_string(&key).unwrap();
    let recipient=identity.lines().find_map(|line|line.strip_prefix("# public key: "))
        .unwrap().trim();

    run(&["vault-store","init","--root",strpath(&vault),
        "--recipient",recipient],true);
    run(&["vault-store","init","--root",strpath(&vault),
        "--recipient",recipient],false);
    let status=run(&["vault-store","status","--root",strpath(&vault)],true);
    let policy:Value=serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(policy["external_worm_verified"],false);
    assert_eq!(policy["delete_supported"],false);
    let backup=run(&["vault-store","backup","--root",strpath(&vault),
        "--source",strpath(&source)],true);
    let meta:Value=serde_json::from_slice(&backup.stdout).unwrap();
    let id=meta["id"].as_str().unwrap();
    assert_eq!(id.len(),32);
    assert_eq!(meta["external_worm_verified"],false);
    let list=run(&["vault-store","list","--root",strpath(&vault)],true);
    let items:Value=serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(items.as_array().unwrap().len(),1);
    assert_eq!(items[0]["id"],id);
    run(&["vault-store","verify","--root",strpath(&vault),"--id",id,
        "--identity",strpath(&key)],true);
    run(&["vault-store","restore","--root",strpath(&vault),"--id",id,
        "--identity",strpath(&key),"--destination",strpath(&dest)],true);
    assert_eq!(fs::read(dest.join(format!("multi-backup/vault/{id}/sub/file.bin"))).unwrap(),
        payload);
    run(&["vault-store","restore","--root",strpath(&vault),"--id",id,
        "--identity",strpath(&key),"--destination",strpath(&dest)],false);

    let wrong=temp.path().join("wrong-identity");
    assert!(Command::new("age-keygen").arg("-o").arg(&wrong).status().unwrap().success());
    run(&["vault-store","verify","--root",strpath(&vault),"--id",id,
        "--identity",strpath(&wrong)],false);
    run(&["vault-store","restore","--root",strpath(&vault),
        "--id","../../escape","--identity",strpath(&key),
        "--destination",strpath(&temp.path().join("escape"))],false);
    run(&["vault-store","backup","--root",strpath(&vault),
        "--source",strpath(&vault)],false);
    let object=vault.join("snapshots").join(format!("{id}.tar.gz.age"));
    let mut bytes=fs::read(&object).unwrap();
    bytes[20]^=0x60;
    fs::write(&object,bytes).unwrap();
    run(&["vault-store","verify","--root",strpath(&vault),
        "--id",id,"--identity",strpath(&key)],false);
    run(&["vault-store","list","--root",strpath(&vault)],false);
}
