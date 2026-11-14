//! Cross-platform, fail-closed acceptance of the *compiled CLI* and age helper.
//! Invoked on real macOS/Windows/Debian CI runners, not only on Ubuntu.
//! This does not replace authenticated live GitHub/Dropbox restore testing.
use serde_json::{json, Value};
use std::{fs, path::Path, process::{Command, Output}};
use tempfile::tempdir;

fn call(exe: &Path, args: &[&str]) -> Output {
    Command::new(exe).args(args).output().unwrap_or_else(|error| {
        panic!("Cannot execute {}: {error}", exe.display())
    })
}
fn passed(exe: &Path, args: &[&str]) -> Output {
    let output = call(exe, args);
    assert!(
        output.status.success(),
        "Failed {:?}: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn denied(exe: &Path, args: &[&str]) {
    let output = call(exe, args);
    assert!(
        !output.status.success(),
        "Unsafe command succeeded {:?}: {}",
        args, String::from_utf8_lossy(&output.stdout)
    );
}
fn utf8(path: &Path) -> &str {
    path.to_str().expect("Temporary acceptance paths must be UTF-8")
}

#[test]
fn native_cli_roundtrip_and_protected_logical_backup() {
    // Never silently skip the test when the age helper is unavailable: it is
    // required for a functioning installed backup/restore application.
    let cli = Path::new(env!("CARGO_BIN_EXE_multi-backup-native"));
    let temp = tempdir().unwrap();
    let key = temp.path().join("age-identity.txt");
    let keygen = Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("age-keygen must be installed in the acceptance runner");
    assert!(keygen.status.success(), "age-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr));
    let raw = fs::read_to_string(&key).unwrap();
    let recipient = raw.lines().find_map(|line| line.strip_prefix("# public key: "))
        .expect("age-keygen public recipient header").trim().to_owned();
    assert!(recipient.starts_with("age1"));

    let src = temp.path().join("original");
    fs::create_dir_all(src.join("nested")).unwrap();
    let bytes = b"multi-backup-acceptance\n\x00binary\xff\n";
    fs::write(src.join("nested").join("payload.bin"), bytes).unwrap();

    // Original archive format: encrypted streaming create/verify/restore,
    // no overwrite, wrong-key rejection and SHA-256 sidecar tamper rejection.
    let archive = temp.path().join("source.tar.gz.age");
    passed(cli, &["archive", "create", "--source", utf8(&src),
        "--output", utf8(&archive), "--recipient", &recipient,
        "--prefix", "acceptance"]);
    assert!(archive.is_file());
    let sidecar = temp.path().join("source.tar.gz.age.sha256");
    assert!(sidecar.is_file());
    passed(cli, &["archive", "verify", "--archive", utf8(&archive),
        "--identity", utf8(&key)]);
    let restore = temp.path().join("restored-archive");
    passed(cli, &["archive", "extract", "--archive", utf8(&archive),
        "--identity", utf8(&key), "--destination", utf8(&restore)]);
    assert_eq!(fs::read(restore.join("acceptance/nested/payload.bin")).unwrap(), bytes);
    denied(cli, &["archive", "extract", "--archive", utf8(&archive),
        "--identity", utf8(&key), "--destination", utf8(&restore)]);

    let wrong_key = temp.path().join("wrong-age-identity.txt");
    let wrong = Command::new("age-keygen").arg("-o").arg(&wrong_key).output().unwrap();
    assert!(wrong.status.success());
    let denied_dest = temp.path().join("wrong-key-destination");
    denied(cli, &["archive", "extract", "--archive", utf8(&archive),
        "--identity", utf8(&wrong_key), "--destination", utf8(&denied_dest)]);
    assert!(!denied_dest.exists(), "Wrong-key restore published plaintext");
    fs::write(&sidecar, format!("{}  source.tar.gz.age\n", "0".repeat(64))).unwrap();
    denied(cli, &["archive", "verify", "--archive", utf8(&archive),
        "--identity", utf8(&key)]);

    // New one-logical-backup format: namespace, verified contents, encrypted
    // hold marker, explicit deletion refusal, fresh-directory restore.
    let store = temp.path().join("store");
    passed(cli, &["storage", "init", "--root", utf8(&store),
        "--recipient", &recipient]);
    let sources = temp.path().join("sources.json");
    fs::write(&sources, serde_json::to_vec(&json!([
        {"namespace":"github","directory":utf8(&src)}
    ])).unwrap()).unwrap();
    let snapshot_out = passed(cli, &["storage", "backup",
        "--root", utf8(&store), "--identity", utf8(&key),
        "--label", "native acceptance", "--sources-json", utf8(&sources),
        "--top-namespace", "Lab/Example"]);
    let snapshot: Value = serde_json::from_slice(&snapshot_out.stdout).unwrap();
    let id = snapshot["id"].as_str().expect("Backup ID from CLI JSON");
    assert_eq!(id.len(), 64);
    passed(cli, &["storage", "verify", "--root", utf8(&store),
        "--identity", utf8(&key), "--id", id]);
    passed(cli, &["storage", "protect-backup", "--root", utf8(&store),
        "--identity", utf8(&key), "--id", id]);
    denied(cli, &["storage", "delete-backup", "--root", utf8(&store),
        "--identity", utf8(&key), "--id", id, "--confirm-id", id]);
    passed(cli, &["storage", "retention-preview", "--root", utf8(&store),
        "--identity", utf8(&key), "--days", "30"]);
    let restored_store = temp.path().join("restored-store");
    passed(cli, &["storage", "restore", "--root", utf8(&store),
        "--identity", utf8(&key), "--id", id,
        "--destination", utf8(&restored_store)]);
    assert_eq!(fs::read(restored_store.join("Lab/Example/github/nested/payload.bin")).unwrap(), bytes);
    denied(cli, &["storage", "restore", "--root", utf8(&store),
        "--identity", utf8(&key), "--id", id,
        "--destination", utf8(&restored_store)]);
}