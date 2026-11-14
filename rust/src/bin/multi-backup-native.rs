//! Standalone native Rust CLI for existing Multi Backup encrypted archives.
//! This is intentionally separate from the legacy Python backup CLI while
//! interoperability is tested. Never accepts a silent plaintext archive mode.
use multi_backup_core::{archive, checkpoint_audit, dropbox_hash, journal, vault};
use zeroize::Zeroizing;
use std::io::{self, Read};
use std::{collections::BTreeMap, env, path::Path, process};

fn usage() -> &'static str {
    "Multi Backup Rust archive CLI (EUPL-1.2)\n\
     Usage:\n\
       multi-backup-native --version\n\
       multi-backup-native archive create --source DIR --output FILE.tar.gz.age --recipient age1... [--prefix multi-backup]\n\
       multi-backup-native archive list --archive FILE.tar.gz.age --identity AGE_KEY [--json]\n\
       multi-backup-native archive verify --archive FILE.tar.gz.age --identity AGE_KEY\n\
       multi-backup-native archive extract --archive FILE.tar.gz.age --identity AGE_KEY --destination NEW_DIR\n\
       multi-backup-native checkpoint audit --file ACTIVE.json [--json]\n\
       multi-backup-native journal start --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal resume --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal record --directory STATE_DIR --config CONFIG.json --step STEP --file ARCHIVE\n\
       multi-backup-native journal finish --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native dropbox hash --file LOCAL_FILE\n\
       multi-backup-native vault verify --file PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native vault create --file NEW.agevault [--password-stdin]\n\
       multi-backup-native vault export --file SOURCE.agevault --destination RECOVERY.agevault [--password-stdin]\n\
       multi-backup-native vault import --file RECOVERY.agevault --destination NEW.agevault [--password-stdin]\n\
       multi-backup-native vault change-password --file PROTECTED.agevault [--password-stdin]\n\
     Safety: explicit identity required, encrypted output only, no overwriting.\n"
}

fn options(args: &[String]) -> Result<(BTreeMap<&str, &str>, bool, bool), String> {
    let mut parsed = BTreeMap::new();
    let mut json = false;
    let mut password_stdin = false;
    let mut pos = 0;
    while pos < args.len() {
        if args[pos] == "--password-stdin" {
            if password_stdin { return Err("Duplicate password input flag".into()); }
            password_stdin = true;
            pos += 1;
            continue;
        }
        if args[pos] == "--json" {
            if json { return Err("Duplicate --json flag".into()); }
            json = true;
            pos += 1;
            continue;
        }
        let key = args[pos].as_str();
        if !key.starts_with("--") || pos + 1 >= args.len() {
            return Err(format!("Expected --flag value, found {key}"));
        }
        let value = args[pos + 1].as_str();
        if value.starts_with("--") || parsed.insert(key, value).is_some() {
            return Err(format!("Missing or duplicate value for {key}"));
        }
        pos += 2;
    }
    Ok((parsed, json, password_stdin))
}
fn required<'a>(args: &'a BTreeMap<&str, &str>, flag: &str) -> Result<&'a str, String> {
    args.get(flag).copied().ok_or_else(|| format!("Missing {flag}"))
}
fn accepted(args: &BTreeMap<&str, &str>, known: &[&str]) -> Result<(), String> {
    for key in args.keys() {
        if !known.contains(key) {
            return Err(format!("Unknown option {key}"));
        }
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("multi-backup-native 0.1.0-r019-dev");
        return Ok(());
    }
    if args.len() < 2 { return Err(usage().into()); }
    let family = args[0].as_str();
    let command = args[1].as_str();
    let (flags, json, password_stdin) = options(&args[2..])?;
    if family == "vault" {
        if json { return Err("--json is not supported for vault operations".into()); }
        let count = match command {
            "verify" | "create" => 1,
            "export" | "import" | "change-password" => 2,
            _ => return Err(usage().into()),
        };
        if command == "export" || command == "import" {
            accepted(&flags, &["--file", "--destination"])?;
        } else {
            accepted(&flags, &["--file"])?;
        }
        let secrets: Vec<Zeroizing<String>> = if password_stdin {
            let mut input = Zeroizing::new(Vec::new());
            io::stdin().take(4096).read_to_end(&mut *input)?;
            let text = std::str::from_utf8(&input)?;
            let lines: Vec<_> = text.lines().collect();
            if lines.len() != count {
                return Err("Expected the required number of passwords on stdin".into());
            }
            lines.iter().map(|line| Zeroizing::new(
                line.trim_end_matches('\r').to_string())).collect()
        } else {
            let mut supplied = Vec::new();
            for index in 0..count {
                let prompt = format!("Vault password {}: ", index + 1);
                supplied.push(Zeroizing::new(rpassword::prompt_password(prompt)?));
            }
            supplied
        };
        let file = Path::new(required(&flags, "--file")?);
        let recipient = match command {
            "verify" => vault::verify(file, &secrets[0])?,
            "create" => vault::create(file, &secrets[0])?,
            "export" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::export_recovery(file, to, &secrets[0], &secrets[1])?
            }
            "import" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::import_recovery(file, to, &secrets[0], &secrets[1])?
            }
            _ => vault::change_password(file, &secrets[0], &secrets[1])?,
        };
        println!("Vault {command} authenticated for recipient {recipient}; private identity not exported");
        return Ok(());
    }
    if password_stdin {
        return Err("--password-stdin is only accepted by vault operations".into());
    }
    if family == "journal" {
        if password_stdin { return Err("--password-stdin not valid for journals".into()); }
        if !["start", "resume", "record", "finish", "restart", "show"].contains(&command) {
            return Err(usage().into());
        }
        let known = if command == "record" {
            vec!["--directory","--config","--step","--file"]
        } else {
            vec!["--directory","--config"]
        };
        accepted(&flags, &known)?;
        let dir = Path::new(required(&flags, "--directory")?);
        let configuration: serde_json::Value = serde_json::from_reader(
            std::fs::File::open(required(&flags, "--config")?))?;
        let start_new = command == "start" || command == "restart";
        let mut state = journal::Journal::start(dir, &configuration,
                                    start_new, !start_new)?;
        match command {
            "record" => state.record(required(&flags, "--step")?,
                                    Path::new(required(&flags, "--file")?))?,
            "finish" => state.finish()?,
            _ => {},
        }
        if json {
            println!("{}", serde_json::to_string_pretty(state.status())?);
        } else {
            println!("Journal {}: cycle={} resumed={} steps={}",
                     command, state.cycle()?, state.resumed,
                     state.status()["steps"].as_object().map_or(0, |x| x.len()));
        }
        return Ok(());
    }
    if family == "checkpoint" && command == "audit" {
        accepted(&flags, &["--file"])?;
        let state = Path::new(required(&flags, "--file")?);
        let audit = checkpoint_audit::audit(state)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&audit)?);
        } else {
            println!("Checkpoint {}: {} verified, {} invalid",
                audit.cycle, audit.valid.len(), audit.invalid.len());
        }
        if !audit.invalid.is_empty() {
            return Err("Checkpoint contains missing or modified archives".into());
        }
        return Ok(());
    }
    if family == "dropbox" && command == "hash" {
        if json { return Err("--json not valid for dropbox hash".into()); }
        accepted(&flags, &["--file"])?;
        let file = Path::new(required(&flags, "--file")?);
        println!("{}", dropbox_hash::hash_file(file)?);
        return Ok(());
    }
    if family != "archive" { return Err(usage().into()); }
    match command {
        "create" => {
            accepted(&flags, &["--source", "--output", "--recipient", "--prefix"])?;
            if json { return Err("--json not valid for create".into()); }
            let source = Path::new(required(&flags, "--source")?);
            let output = Path::new(required(&flags, "--output")?);
            let recipient = required(&flags, "--recipient")?;
            let prefix = flags.get("--prefix").copied().unwrap_or("multi-backup");
            let count = archive::create(source, output, recipient, prefix)?;
            println!("Created {count} verified input entries in encrypted archive {}", output.display());
        },
        "list" | "verify" | "extract" => {
            let known = if command == "extract" {
                vec!["--archive", "--identity", "--destination"]
            } else {
                vec!["--archive", "--identity"]
            };
            accepted(&flags, &known)?;
            if json && command != "list" {
                return Err("--json valid only for list".into());
            }
            let path = Path::new(required(&flags, "--archive")?);
            let key = Path::new(required(&flags, "--identity")?);
            match command {
                "list" => {
                    let members = archive::list(path, key)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&members)?);
                    } else {
                        for item in members {
                            println!("{}\t{}\t{}",
                                if item.directory { "dir" } else { "file" },
                                item.bytes, item.path);
                        }
                    }
                }
                "verify" => {
                    let count = archive::verify(path, key)?;
                    println!("Verified {count} members, encrypted stream integrity checked");
                }
                _ => {
                    let destination = Path::new(required(&flags, "--destination")?);
                    let members = archive::extract(path, key, destination)?;
                    println!("Restored {} members safely to {}", members.len(), destination.display());
                }
            }
        },
        _ => return Err(usage().into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Multi Backup native archive error: {error}");
        process::exit(2);
    }
}
