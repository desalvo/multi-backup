//! Real PostgreSQL + MariaDB container CI integration for native Rust CLI.
//! This is a LIVE database test when MULTI_BACKUP_LIVE_DATABASE_CI=1.
//! Normal core CI skips it (no implicit external host/network required).
use multi_backup_core::chunk_store::{Store,DEFAULT_CHUNK};
use serde_json::{json,Value};
use std::{fs,os::unix::fs::PermissionsExt,path::Path,process::{Command,Output}};
use tempfile::tempdir;
fn strpath(p:&Path)->&str{p.to_str().unwrap()}
fn run(mut cmd:Command)->Output{
    let out=cmd.output().expect("Database binary unavailable");
    assert!(out.status.success(),"stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    out
}
fn pg(db:&str,sql:&str)->String{
    let mut c=Command::new("psql");
    c.args(["-h","127.0.0.1","-p","5432","-U","postgres","-X",
            "-A","-t","-v","ON_ERROR_STOP=1","-d",db,"-c",sql]);
    c.env("PGPASSWORD","test_pg_password");
    String::from_utf8(run(c).stdout).unwrap().trim().to_owned()
}
fn maria(db:&str,sql:&str)->String{
    let mut c=Command::new("mariadb");
    c.args(["--protocol=tcp","-h","127.0.0.1","-P","3306",
            "-u","root","-N","-B"]);
    c.env("MYSQL_PWD","test_maria_password");
    if !db.is_empty(){c.arg(db);}
    c.arg("--execute").arg(sql);
    String::from_utf8(run(c).stdout).unwrap().trim().to_owned()
}
fn cli(args:&[&str],should_pass:bool)->Output{
    let o=Command::new(env!("CARGO_BIN_EXE_multi-backup-native"))
        .args(args).output().unwrap();
    assert_eq!(o.status.success(),should_pass,"args={args:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&o.stdout),String::from_utf8_lossy(&o.stderr));
    o
}
fn private(path:&Path,body:&[u8]){
    fs::write(path,body).unwrap();
    fs::set_permissions(path,fs::Permissions::from_mode(0o600)).unwrap();
}
fn exercise(engine:&str){
    let dir=tempdir().unwrap();
    let key=dir.path().join("age-key");
    run({let mut cmd=Command::new("age-keygen");
        cmd.args(["-o",strpath(&key)]);cmd});
    let keytext=fs::read_to_string(&key).unwrap();
    let recipient=keytext.lines().find_map(|l|l.strip_prefix("# public key: "))
        .unwrap().trim().to_owned();
    let storepath=dir.path().join("store");
    Store::init(&storepath,&recipient,DEFAULT_CHUNK).unwrap();
    let profilepath=dir.path().join("profile.json");
    let targetpath=dir.path().join("target.json");
    let credentials=dir.path().join("credentials");
    let (port,src,dst,expected)=match engine{
        "postgresql"=>{
            private(&credentials,b"127.0.0.1:5432:*:postgres:test_pg_password\n");
            pg("postgres","CREATE DATABASE mb_pg_original");
            pg("mb_pg_original","CREATE TABLE inventory (id INTEGER PRIMARY KEY, name TEXT NOT NULL)");
            pg("mb_pg_original","INSERT INTO inventory (id,name) SELECT n, 'record-' || n::text FROM generate_series(1,1000) n");
            ("5432","mb_pg_original","mb_pg_recovered","1000|500500")
        },
        "mysql"=>{
            private(&credentials,b"[client]\npassword=test_maria_password\n");
            maria("","CREATE DATABASE mb_my_original CHARACTER SET utf8mb4");
            maria("mb_my_original","CREATE TABLE inventory (id INTEGER PRIMARY KEY, name VARCHAR(255) NOT NULL) ENGINE=InnoDB");
            for n in 1..=1000{
                maria("mb_my_original",&format!("INSERT INTO inventory VALUES ({n}, 'record-{n}')"));
            }
            ("3306","mb_my_original","mb_my_recovered","1000\t500500")
        },
        _=>panic!("unknown engine"),
    };
    let source=json!({"engine":engine,"host":"127.0.0.1",
        "port":port.parse::<u16>().unwrap(),"user":if engine=="postgresql"{"postgres"}else{"root"},
        "database":src,"credentials_file":credentials});
    let mut target=source.clone();
    target["database"]=json!(dst);
    private(&profilepath,&serde_json::to_vec(&source).unwrap());
    private(&targetpath,&serde_json::to_vec(&target).unwrap());
    let source_rows=if engine=="postgresql"{
        pg(src,"SELECT id::text || '|' || name FROM inventory ORDER BY id")
    }else{maria(src,"SELECT CONCAT(id,'|',name) FROM inventory ORDER BY id")};
    let output=cli(&["plugins","database-backup","--profile-json",strpath(&profilepath),
        "--root",strpath(&storepath),"--identity",strpath(&key),
        "--label","live-db-full","--json"],true);
    let info:Value=serde_json::from_slice(&output.stdout).unwrap();
    let id=info["id"].as_str().unwrap().to_owned();
    let store=Store::open(&storepath,&key).unwrap();
    assert!(store.verify(&key,&id).unwrap().healthy);
    assert_eq!(store.list(&key).unwrap().len(),1);
    cli(&["plugins","database-restore-new","--profile-json",strpath(&targetpath),
        "--root",strpath(&storepath),"--identity",strpath(&key),
        "--id",&id,"--confirm-target","wrong","--json"],false);
    let exists=if engine=="postgresql"{
        pg("postgres","SELECT COUNT(*) FROM pg_database WHERE datname='mb_pg_recovered'")
    }else{maria("","SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name='mb_my_recovered'")};
    assert_eq!(exists,"0");
    let confirmation=format!("{engine}:127.0.0.1:{port}:{dst}");
    cli(&["plugins","database-restore-new","--profile-json",strpath(&targetpath),
        "--root",strpath(&storepath),"--identity",strpath(&key),
        "--id",&id,"--confirm-target",&confirmation,"--json"],true);
    let count=if engine=="postgresql"{
        pg(dst,"SELECT COUNT(*), SUM(id) FROM inventory")
    }else{maria(dst,"SELECT COUNT(*), SUM(id) FROM inventory")};
    assert_eq!(count,expected,"Database restore content mismatch");
    let recovered_rows=if engine=="postgresql"{
        pg(dst,"SELECT id::text || '|' || name FROM inventory ORDER BY id")
    }else{maria(dst,"SELECT CONCAT(id,'|',name) FROM inventory ORDER BY id")};
    assert_eq!(recovered_rows,source_rows,"Complete row-by-row recovery mismatch");
    cli(&["plugins","database-restore-new","--profile-json",strpath(&targetpath),
        "--root",strpath(&storepath),"--identity",strpath(&key),
        "--id",&id,"--confirm-target",&confirmation,"--json"],false);
    let again=if engine=="postgresql"{
        pg(dst,"SELECT COUNT(*), SUM(id) FROM inventory")
    }else{maria(dst,"SELECT COUNT(*), SUM(id) FROM inventory")};
    assert_eq!(again,expected,"Existing restore target changed after denied overwrite");
    assert!(store.verify(&key,&id).unwrap().healthy);
    // With an explicitly wrong credential, a live dump must fail closed
    // and must not commit a second encrypted logical snapshot.
    let invalid=dir.path().join("bad-credentials");
    if engine=="postgresql"{
        private(&invalid,b"127.0.0.1:5432:*:postgres:wrong_password\n");
    }else{private(&invalid,b"[client]\npassword=wrong_password\n");}
    let mut bad=source.clone();
    bad["credentials_file"]=json!(invalid);
    let badpath=dir.path().join("bad-profile.json");
    private(&badpath,&serde_json::to_vec(&bad).unwrap());
    cli(&["plugins","database-backup","--profile-json",strpath(&badpath),
        "--root",strpath(&storepath),"--identity",strpath(&key),
        "--label","bad-credential-must-not-commit","--json"],false);
    assert_eq!(store.list(&key).unwrap().len(),1);
    println!("PASSED real {engine} create-only encrypted roundtrip; 1000 rows verified byte-for-byte; invalid credentials fail closed");
}
#[test]fn real_postgres_full_restore(){
    if std::env::var("MULTI_BACKUP_LIVE_DATABASE_CI").ok().as_deref()!=Some("1"){return;}
    exercise("postgresql");
}
#[test]fn real_mariadb_full_restore(){
    if std::env::var("MULTI_BACKUP_LIVE_DATABASE_CI").ok().as_deref()!=Some("1"){return;}
    exercise("mysql");
}
