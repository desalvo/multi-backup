//! Compiled binary acceptance for the first genuine generic Rust plugin pair.
//! This must run with age-keygen/age, including on Windows and macOS runners.
use serde_json::Value;
use std::{fs, path::Path, process::{Command,Output}};
use tempfile::tempdir;

fn cli(args: &[&str], success: bool) -> Output {
    let exe = Path::new(env!("CARGO_BIN_EXE_multi-backup-native"));
    let out = Command::new(exe).args(args).output().expect("compiled CLI must run");
    assert_eq!(out.status.success(), success,
        "CLI {:?}: stdout={}, stderr={}",args,
        String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));
    out
}
fn path(p: &Path) -> &str { p.to_str().expect("test path UTF-8") }

#[test]
fn posix_pair_encrypts_verifies_and_restores_without_python() {
    let tmp=tempdir().unwrap();
    let key=tmp.path().join("identity.txt");
    let generated=Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("age-keygen helper required by native package");
    assert!(generated.status.success(),"age-keygen failed");
    let identity=fs::read_to_string(&key).unwrap();
    let recipient=identity.lines().find_map(|s|s.strip_prefix("# public key: "))
        .unwrap().trim();

    let source=tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("nested")).unwrap();
    let payload=b"Native plugin encrypted backup\0\xff\x10";
    fs::write(source.join("nested/data.bin"),payload).unwrap();
    fs::write(source.join("README.txt"),b"shared metadata").unwrap();
    let root=tmp.path().join("encrypted-store");
    cli(&["storage","init","--root",path(&root),"--recipient",recipient],true);

    let plan=cli(&["plugins","plan","--mode","backup","--input","posix",
        "--output","posix","--json"],true);
    let capability:Value=serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(capability["preview_executable"],true);
    assert_eq!(capability["executable"],false);

    let b=cli(&["plugins","backup","--input","posix","--output","posix",
        "--source",path(&source),"--root",path(&root),"--identity",path(&key),
        "--label","Rust native","--namespace","github","--top-namespace","Projects/Team",
        "--json"],true);
    let snapshot:Value=serde_json::from_slice(&b.stdout).unwrap();
    let id=snapshot["id"].as_str().unwrap();
    assert_eq!(id.len(),64);
    assert_eq!(snapshot["entries"].as_array().unwrap().len(),2);
    assert_eq!(snapshot["top_namespace"],"Projects/Team");
    cli(&["storage","verify","--root",path(&root),"--identity",path(&key),
        "--id",id],true);

    let restored=tmp.path().join("restored");
    cli(&["plugins","restore","--input","posix","--output","posix",
        "--root",path(&root),"--identity",path(&key),"--id",id,
        "--destination",path(&restored),"--selection","Projects/Team/github/nested/data.bin"],true);
    assert_eq!(fs::read(restored.join("Projects/Team/github/nested/data.bin")).unwrap(),payload);
    assert!(!restored.join("Projects/Team/github/README.txt").exists());
    cli(&["plugins","restore","--input","posix","--output","posix",
        "--root",path(&root),"--identity",path(&key),"--id",id,
        "--destination",path(&restored)],false);
    let all=tmp.path().join("full-restore");
    cli(&["plugins","restore","--input","posix","--output","posix",
        "--root",path(&root),"--identity",path(&key),"--id",id,
        "--destination",path(&all)],true);
    assert_eq!(fs::read(all.join("Projects/Team/github/README.txt")).unwrap(),b"shared metadata");

    // Corrupt exactly one encrypted chunk. Authenticated 8+2 parity repair
    // must restore the backup before atomically publishing the new target.
    let first_chunk=snapshot["entries"][0]["chunks"][0]["id"].as_str().unwrap();
    let block_path=root.join(".multi-backup-v1/chunks")
        .join(&first_chunk[..2]).join(&first_chunk[2..4])
        .join(format!("{first_chunk}.age"));
    let mut encrypted=fs::read(&block_path).unwrap();
    let midpoint=encrypted.len()/2;
    encrypted[midpoint]^=0x40;
    fs::write(&block_path,&encrypted).unwrap();
    let parity_restore=tmp.path().join("recovered-from-parity");
    cli(&["plugins","restore","--input","posix","--output","posix",
        "--root",path(&root),"--identity",path(&key),"--id",id,
        "--destination",path(&parity_restore)],true);
    assert_eq!(fs::read(parity_restore.join("Projects/Team/github/nested/data.bin")).unwrap(),payload);
    assert_eq!(fs::read(parity_restore.join("Projects/Team/github/README.txt")).unwrap(),b"shared metadata");

    // Unsupported generic adapters cannot fall back to a misleading POSIX copy.
    cli(&["plugins","backup","--input","nfs","--output","posix",
        "--source",path(&source),"--root",path(&root),"--identity",path(&key),
        "--label","no snapshot attestation"],false);
    // Attestation alone cannot fake an NFS/SMB mount: read-only kernel mount
    // evidence is independently required before any source bytes are read.
    for remote in ["nfs","smb","sshfs","cephfs","glusterfs","lustre"] {
        cli(&["plugins","backup","--input",remote,"--output","posix",
            "--source",path(&source),"--root",path(&root),"--identity",path(&key),
            "--label","not a real mount","--source-snapshot-attested"],false);
    }
    cli(&["plugins","backup","--input","posix","--output","anti-ransomware-vault",
        "--source",path(&source),"--root",path(&root),"--identity",path(&key),
        "--label","not allowed"],false);
    cli(&["plugins","backup","--input","posix","--output","posix",
        "--source",path(tmp.path()),"--root",path(&root),"--identity",path(&key),
        "--label","recursion"],false);
    let bad=tmp.path().join("bad");
    cli(&["plugins","restore","--input","posix","--output","posix",
        "--root",path(&root),"--identity",path(&key),"--id","not-an-id",
        "--destination",path(&bad)],false);
    assert!(!bad.exists());
}

#[test]
fn key_inside_source_and_uninitialized_store_are_rejected() {
    let tmp=tempdir().unwrap();
    let source=tmp.path().join("data");
    fs::create_dir(&source).unwrap();
    let key=source.join("private-age-key.txt");
    let generated=Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("age-keygen helper required");
    assert!(generated.status.success());
    let store=tmp.path().join("store");
    fs::create_dir(&store).unwrap();
    cli(&["plugins","backup","--input","posix","--output","posix",
        "--source",path(&source),"--root",path(&store),
        "--identity",path(&key),"--label","no private keys"],false);
}
