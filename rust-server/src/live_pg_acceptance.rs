//! Actual PostgreSQL 16 state-machine acceptance. These tests are ignored in
//! ordinary unit CI and explicitly invoked by the dedicated ephemeral PG job.
//! The loopback fixture is NOT a production TLS, ingress or Kubernetes test.
use crate::{agent_registry as agent,cluster::live_pg_ci_fixture};
use multi_backup_core::{
    agent_protocol as wire,
    cluster_placement::{StorageLocation,StorageVisibility,SourceTransport},
    execution_policy::ExecutionLimits,
};
use axum::http::HeaderMap;
use base64::{engine::general_purpose::STANDARD as B64,Engine as _};
use serde_json::json;
use uuid::Uuid;

#[test]
#[ignore = "requires disposable genuine PostgreSQL 16"]
fn real_pg_agent_approval_replay_profile_pull_receipt_and_revoke() {
    let db=live_pg_ci_fixture();
    let id=format!("agent{}",Uuid::new_v4().simple());
    let (private,public)=wire::device_keypair();
    let created=agent::register(&db,&agent::Enrollment{
        agent_id:id.clone(),owner_user:"operator".into(),
        device_public_key:public,
    }).unwrap();
    let token=created["token_once"].as_str().unwrap();
    assert_eq!(token.len(),64);
    assert!(!agent::list(&db).unwrap().to_string().contains(token));
    let headers=|n:u64,verb:&str,route:&str,meta:&[u8],body:&[u8]| {
        let proof=wire::sign(&private,verb,route,&id,"operator",n,
            wire::now(),&wire::hex_sha(body),meta).unwrap();
        let mut h=HeaderMap::new();
        h.insert("authorization",format!("Bearer {token}").parse().unwrap());
        h.insert("x-mb-agent-proof",
            B64.encode(serde_json::to_vec(&proof).unwrap()).parse().unwrap());
        h.insert("x-mb-agent-metadata",B64.encode(meta).parse().unwrap());
        h
    };
    let first=headers(1,"GET","/api/agents/policy",b"",b"");
    assert!(agent::authenticate(&db,&first,"GET","/api/agents/policy").is_err());
    assert!(agent::set_approval(&db,&id,true,false).unwrap());
    let verified=agent::authenticate(&db,&first,"GET","/api/agents/policy").unwrap();
    assert_eq!(agent::policy(&db,&verified).unwrap()["approved"],true);
    assert!(agent::policy(&db,&verified).is_err());
    let mut wrong=first.clone();
    wrong.insert("authorization",format!("Bearer {}", "Z".repeat(64)).parse().unwrap());
    assert!(agent::authenticate(&db,&wrong,"GET","/api/agents/policy").is_err());

    let hash=wire::hex_sha(b"reviewed profile");
    assert!(agent::approve_profile(&db,&agent::ProfileApproval{
        agent_id:id.clone(),profile_id:"test-volume".into(),
        profile_sha256:hash.clone(),provider:"afs".into(),
    }).unwrap());
    let lease=agent::enqueue_pull(&db,&agent::PullJob{
        agent_id:id.clone(),profile_id:"test-volume".into(),
        profile_sha256:hash.clone(),
    }).unwrap();
    let poll=headers(2,"GET","/api/agents/poll",b"",b"");
    let verified=agent::authenticate(&db,&poll,"GET","/api/agents/poll").unwrap();
    assert!(agent::poll(&db,&verified).unwrap()["jobs"].as_array().unwrap()
        .iter().any(|j|j["job_id"]==lease));
    assert!(agent::poll(&db,&verified).is_err());

    // This is SQL receipt authorization, NOT real encrypted payload transport.
    let payload=b"receipt-state-fixture";
    let digest=wire::hex_sha(payload);
    let meta=serde_json::to_vec(&json!({
        "backup_id":digest,"profile_id":"test-volume",
        "profile_sha256":hash,"provider":"afs","cell":"example.com",
        "volume":"user.test","encrypted_bytes":payload.len(),
        "recipient":format!("age1{}","q".repeat(32)),
        "pull_job_id":lease,
    })).unwrap();
    let upload=headers(3,"POST","/api/agents/upload",&meta,payload);
    let verified=agent::authenticate(&db,&upload,"POST","/api/agents/upload").unwrap();
    let manifest=agent::validate_upload(&meta).unwrap();
    assert!(agent::preflight_upload(&db,&verified,&manifest).unwrap());
    let mut unapproved=manifest.clone();
    unapproved.profile_sha256="f".repeat(64);
    assert!(!agent::preflight_upload(&db,&verified,&unapproved).unwrap());
    agent::commit_upload(&db,&verified,&manifest,"/tmp/ci-receipt-only",
        &digest,payload.len() as u64).unwrap();
    assert!(agent::commit_upload(&db,&verified,&manifest,"/tmp/ci-replay",
        &digest,payload.len() as u64).is_err());
    let receipts:i64=db.connect().unwrap().query_one(
        "SELECT COUNT(*) FROM mb_agent_uploads WHERE agent_id=$1",&[&id])
        .unwrap().get(0);
    assert_eq!(receipts,1);
    assert!(agent::set_approval(&db,&id,false,true).unwrap());
    let revoked=headers(4,"GET","/api/agents/policy",b"",b"");
    assert!(agent::authenticate(&db,&revoked,"GET","/api/agents/policy").is_err());
    assert!(!agent::set_approval(&db,&id,true,false).unwrap());
}

#[test]
#[ignore = "requires disposable genuine PostgreSQL 16"]
fn real_pg_two_replica_affinity_capacity_lease_and_fencing(){
    let db=live_pg_ci_fixture();
    let tag=Uuid::new_v4().simple().to_string();
    let a=format!("a{tag}");let b=format!("b{tag}");
    let local=format!("local{tag}");let shared=format!("shared{tag}");
    let jlocal=format!("jl{tag}");let j1=format!("j1{tag}");let j2=format!("j2{tag}");
    db.register_storage(&StorageLocation{
        storage_id:local.clone(),backend:"posix".into(),
        visibility:StorageVisibility::NodeLocal,owner_replica:Some(a.clone()),
    }).unwrap();
    db.register_storage(&StorageLocation{
        storage_id:shared.clone(),backend:"posix".into(),
        visibility:StorageVisibility::Shared,owner_replica:None,
    }).unwrap();
    let eligible=vec![local.clone(),shared.clone()];
    db.heartbeat_replica(&a,1,&eligible).unwrap();
    db.heartbeat_replica(&b,1,&eligible).unwrap();
    let limits=ExecutionLimits::default();
    db.enqueue(&jlocal,"posix",&local,SourceTransport::ServerLocal,
        &limits,&json!({}),0).unwrap();
    assert!(db.claim(&b,60).unwrap().is_none());
    let local_lease=db.claim(&a,60).unwrap().unwrap();
    assert_eq!(local_lease.job_id,jlocal);
    assert!(db.is_current(&a,&jlocal,&local_lease.token).unwrap());
    assert!(!db.is_current(&b,&jlocal,&local_lease.token).unwrap());
    assert!(db.finish(&a,&jlocal,&local_lease.token,true,&json!({})).unwrap());

    for name in [&j1,&j2]{
        db.enqueue(name,"posix",&shared,SourceTransport::ServerLocal,
            &limits,&json!({}),0).unwrap();
    }
    let first=db.claim(&a,60).unwrap().unwrap();
    assert!(db.claim(&a,60).unwrap().is_none(),"Capacity=1 must be serialized");
    let second=db.claim(&b,60).unwrap().unwrap();
    assert_ne!(first.job_id,second.job_id);
    assert!(db.finish(&a,&first.job_id,&first.token,true,&json!({})).unwrap());
    let mut sql=db.connect().unwrap();
    sql.execute("UPDATE mb_cluster_jobs SET lease_until=now()-interval '3 seconds' WHERE job_id=$1",
        &[&second.job_id]).unwrap();
    assert!(!db.is_current(&b,&second.job_id,&second.token).unwrap());
    assert!(!db.finish(&b,&second.job_id,&second.token,true,&json!({})).unwrap());
    assert!(db.quarantine_expired().unwrap()>=1);
    assert!(db.claim(&a,60).unwrap().is_none(),"No auto-requeue after expiry");
    assert!(db.recover_quarantined(&second.job_id,&b,second.attempts,false,true).is_err());
    assert!(!db.recover_quarantined(&second.job_id,&b,second.attempts,true,true).unwrap(),
        "Cannot recover while the old node is live");
    sql.execute("UPDATE mb_cluster_replicas SET last_seen=now()-interval '3 minutes' WHERE replica_id=$1",
        &[&b]).unwrap();
    assert!(db.recover_quarantined(&second.job_id,&b,second.attempts,true,true).unwrap());
    let recovered=db.claim(&a,60).unwrap().unwrap();
    assert_eq!(recovered.job_id,second.job_id);
    assert_ne!(recovered.token,second.token);
    assert!(!db.is_current(&b,&second.job_id,&second.token).unwrap());
    assert!(db.finish(&a,&recovered.job_id,&recovered.token,true,&json!({})).unwrap());
}


#[test]
#[ignore = "requires disposable genuine PostgreSQL 16"]
fn real_pg_remote_queue_is_never_misclaimed_by_local_worker(){
    let db=live_pg_ci_fixture();
    let tag=Uuid::new_v4().simple().to_string();
    let node=format!("worker{tag}");
    let storage=format!("shared{tag}");
    let remote=format!("remote{tag}");
    let local=format!("local{tag}");
    db.register_storage(&StorageLocation{
        storage_id:storage.clone(),backend:"posix".into(),
        visibility:StorageVisibility::Shared,owner_replica:None,
    }).unwrap();
    db.heartbeat_replica(&node,1,&[storage.clone()]).unwrap();
    let limits=ExecutionLimits::default();
    // These jobs are reserved for a separately authorized SSH/agent
    // dispatcher; a filesystem worker must not claim-and-fail them.
    db.enqueue(&remote,"posix",&storage,SourceTransport::ManagedAgentPull,
        &limits,&json!({"profile_id":"authorized-later"}),100).unwrap();
    assert!(db.claim(&node,60).unwrap().is_none());
    let status=db.status().unwrap();
    assert!(status["jobs"].as_array().unwrap().iter().any(|j|
        j["id"]==remote && j["status"]=="queued" && j["attempts"]==0));
    db.enqueue(&local,"posix",&storage,SourceTransport::ServerLocal,
        &limits,&json!({}),0).unwrap();
    let lease=db.claim(&node,60).unwrap().unwrap();
    assert_eq!(lease.job_id,local);
    assert!(db.finish(&node,&local,&lease.token,true,&json!({})).unwrap());
    // No automatic downgrade of the high-priority remote job to local.
    assert!(db.claim(&node,60).unwrap().is_none());
}

#[test]
#[ignore = "requires disposable genuine PostgreSQL 16"]
fn real_pg_parallel_claims_and_signed_nonce_single_winner(){
    use std::sync::{Arc,Barrier};
    use std::thread;
    let db=live_pg_ci_fixture();
    let tag=Uuid::new_v4().simple().to_string();
    let node=format!("node{tag}");
    let store=format!("store{tag}");
    db.register_storage(&StorageLocation{
        storage_id:store.clone(),backend:"posix".into(),
        visibility:StorageVisibility::Shared,owner_replica:None,
    }).unwrap();
    db.heartbeat_replica(&node,1,&[store.clone()]).unwrap();
    let limits=ExecutionLimits::default();
    for n in 0..8{
        db.enqueue(&format!("job{tag}{n}"),"posix",&store,
            SourceTransport::ServerLocal,&limits,&json!({}),0).unwrap();
    }
    let barrier=Arc::new(Barrier::new(9));
    let workers=(0..8).map(|_|{
        let db=db.clone();
        let barrier=barrier.clone();
        let node=node.clone();
        thread::spawn(move||{
            barrier.wait();
            db.claim(&node,60).unwrap()
        })
    }).collect::<Vec<_>>();
    barrier.wait();
    let results=workers.into_iter().filter_map(|h|h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.len(),1,
        "Parallel PostgreSQL claims must not overbook capacity=1");
    let leased=&results[0];
    assert!(db.is_current(&node,&leased.job_id,&leased.token).unwrap());
    assert!(db.claim(&node,60).unwrap().is_none());
    assert!(db.finish(&node,&leased.job_id,&leased.token,true,
        &json!({})).unwrap());
    assert!(db.claim(&node,60).unwrap().is_some(),
        "A finished slot must become available again");

    let device=format!("replay{tag}");
    let (secret,public)=wire::device_keypair();
    agent::register(&db,&agent::Enrollment{
        agent_id:device.clone(),owner_user:"operator".into(),
        device_public_key:public,
    }).unwrap();
    assert!(agent::set_approval(&db,&device,true,false).unwrap());
    let signed=wire::sign(&secret,"GET","/api/agents/policy",&device,
        "operator",1,wire::now(),&wire::hex_sha(b""),b"").unwrap();
    let barrier=Arc::new(Barrier::new(9));
    let workers=(0..8).map(|_|{
        let db=db.clone();
        let barrier=barrier.clone();
        let proof=signed.clone();
        thread::spawn(move||{
            barrier.wait();
            agent::record_sequence(&db,&proof).unwrap()
        })
    }).collect::<Vec<_>>();
    barrier.wait();
    let accepted=workers.into_iter().map(|h|h.join().unwrap())
        .filter(|accepted|*accepted).count();
    assert_eq!(accepted,1,"Concurrent requests must never reuse a signed nonce");
}


#[test]
#[ignore = "requires disposable genuine PostgreSQL 16"]
fn real_pg_monitor_token_issue_revoke_and_single_db_backup_lease(){
    let db=live_pg_ci_fixture();
    // After concurrent-safe auto-migration the real PG catalog must contain
    // all four schema versions exactly once, with SHA256 digests. Repeat the
    // schema initializer to assert that restarting another server replica
    // preserves the ledger rather than attempting a destructive rebuild.
    let before=db.connect().unwrap().query(
       "SELECT version, checksum_sha256 FROM mb_schema_migrations ORDER BY version",&[]).unwrap();
    assert_eq!(before.len(),4);
    for (idx,row) in before.iter().enumerate(){
        assert_eq!(row.get::<_,i64>(0),(idx+1) as i64);
        assert_eq!(row.get::<_,String>(1).len(),64);
    }
    let _another_replica=live_pg_ci_fixture();
    let after=db.connect().unwrap().query(
       "SELECT version, checksum_sha256 FROM mb_schema_migrations ORDER BY version",&[]).unwrap();
    assert_eq!(after.len(),4);
    for (a,b) in before.iter().zip(after.iter()) {
        assert_eq!(a.get::<_,i64>(0),b.get::<_,i64>(0));
        assert_eq!(a.get::<_,String>(1),b.get::<_,String>(1));
    }
    let id=format!("monitor-{}",Uuid::new_v4().simple());
    assert!(db.create_monitor_token("",30).is_err());
    assert!(db.create_monitor_token(&id,0).is_err());
    assert!(db.create_monitor_token(&id,366).is_err());
    let token=db.create_monitor_token(&id,7).unwrap();
    let raw=token["token"].as_str().unwrap();
    assert_eq!(raw.len(),70);
    assert!(raw.starts_with("mbmon_"));
    assert!(db.monitor_token_valid(raw).unwrap());
    assert!(!db.monitor_token_valid("bad").unwrap());
    assert!(!db.monitor_token_valid(&format!("mbmon_{}","F".repeat(64))).unwrap());
    let catalog=db.monitor_token_list().unwrap().to_string();
    assert!(!catalog.contains(raw),"token plaintext must never reach catalog");
    assert!(db.revoke_monitor_token(token["id"].as_str().unwrap()).unwrap());
    assert!(!db.monitor_token_valid(raw).unwrap());
    assert!(!db.revoke_monitor_token(token["id"].as_str().unwrap()).unwrap());

    // This tests the PostgreSQL lease state machine only. It does NOT claim
    // a real age/WORM backup, which has an independent integration test.
    db.configure_db_backups(false,1440,"s3_worm").unwrap();
    let replica="pg-live-dbbackup";
    let claimed=db.claim_db_backup(replica,true).unwrap().unwrap();
    assert!(db.renew_db_backup_lease(&claimed).unwrap(),
        "Only live current-lease writer can renew a long database dump");
    assert!(db.claim_db_backup(replica,true).unwrap().is_none());
    let (id,_token)=claimed.split_once(':').unwrap();
    let forged=format!("{id}:{}",Uuid::new_v4());
    assert!(!db.renew_db_backup_lease(&forged).unwrap(),
        "A writer with another token must never extend the real writer's lease");
    assert!(db.renew_db_backup_lease("invalid").is_err());
    assert!(db.finish_db_backup(&claimed,None,Some("ci_expected_failure")).unwrap());
    assert!(!db.renew_db_backup_lease(&claimed).unwrap(),
        "A completed writer can no longer renew a lease");
    assert!(!db.finish_db_backup(&claimed,None,Some("replay")).unwrap());

    let stale=db.claim_db_backup(replica,true).unwrap().unwrap();
    let stale_job=stale.split_once(':').unwrap().0.to_string();
    db.connect().unwrap().execute("UPDATE mb_internal_backup_policy
      SET lease_until=now()-interval '5 seconds'
      WHERE current_job=$1",&[&stale_job]).unwrap();
    assert!(!db.renew_db_backup_lease(&stale).unwrap(),
        "Expired lease cannot be revived by the same stale writer");
    assert!(!db.finish_db_backup(&stale,None,Some("late_success")).unwrap());
    assert!(db.claim_db_backup(replica,true).unwrap().is_none(),
        "Expired backup must enter manual reconciliation rather than retry");
    assert_eq!(db.db_backup_dashboard().unwrap()["lease_state"],"needs_reconciliation");
    db.connect().unwrap().execute("UPDATE mb_internal_backup_policy SET
      lease_state='idle',lease_until=NULL,lease_token=NULL,current_job=NULL,
      enabled=false,updated_at=now() WHERE singleton=TRUE",&[]).unwrap();
    let dashboard=db.db_backup_dashboard().unwrap();
    assert_eq!(dashboard["lease_state"],"idle");
    assert!(dashboard["history"].as_array().unwrap().iter()
        .any(|item|item["status"]=="failed"&&item["error_code"]=="ci_expected_failure"));
}


/// Real PostgreSQL16, Ed25519-authenticated agent upload handler and native
/// age ciphertext acceptance. It is deliberately a direct in-process HTTP
/// handler exercise, not a remote TLS/three-physical-machine certification.
#[test]
#[ignore = "requires disposable PostgreSQL16 plus age and age-keygen"]
fn real_pg_agent_encrypted_upload_no_clobber_and_restore(){
    use crate::{agent_api,security::SecurityPolicy,AppConfig,AppState};
    use axum::{body::Body,extract::State,http::StatusCode};
    use std::{collections::HashMap,fs,io::Write,path::PathBuf,
        process::{Command,Stdio},sync::{Arc,Mutex}};
    let db=live_pg_ci_fixture();
    let temp=tempfile::tempdir().unwrap();
    let private_spool=temp.path().join("upload");
    fs::create_dir(&private_spool).unwrap();
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_spool,fs::Permissions::from_mode(0o700)).unwrap();
    }
    // Ignored live tests run --test-threads=1. Do not change production env
    // or use a non-disposable destination here.
    std::env::set_var("MULTI_BACKUP_AGENT_UPLOAD_ROOT",&private_spool);
    let key=temp.path().join("identity.agekey");
    let gen=Command::new("age-keygen").arg("-o").arg(&key).output()
        .expect("Real age-keygen executable is mandatory");
    assert!(gen.status.success(),"age-keygen did not produce a test identity");
    let public=Command::new("age-keygen").arg("-y").arg(&key).output().unwrap();
    assert!(public.status.success());
    let recipient=String::from_utf8(public.stdout).unwrap().trim().to_owned();
    assert!(recipient.starts_with("age1"));
    let cipher=temp.path().join("out.age");
    let payload=b"Multi Backup real signed agent encrypted upload restore fixture\n";
    let mut writer=Command::new("age").arg("-r").arg(&recipient)
        .arg("-o").arg(&cipher)
        .stdin(Stdio::piped()).spawn().expect("age binary missing");
    writer.stdin.take().unwrap().write_all(payload).unwrap();
    assert!(writer.wait().unwrap().success());
    let ciphertext=fs::read(&cipher).unwrap();
    assert!(ciphertext.starts_with(b"age-encryption.org/v1"));
    let agent_id=format!("agent{}",Uuid::new_v4().simple());
    let (private,public_key)=wire::device_keypair();
    let registration=agent::register(&db,&agent::Enrollment{
        agent_id:agent_id.clone(),owner_user:"operator".into(),
        device_public_key:public_key,
    }).unwrap();
    let token=registration["token_once"].as_str().unwrap().to_owned();
    assert!(agent::set_approval(&db,&agent_id,true,false).unwrap());
    let profile_id=format!("profile{}",Uuid::new_v4().simple());
    let profile_hash=wire::hex_sha(b"approved dedicated age/AFS profile");
    assert!(agent::approve_profile(&db,&agent::ProfileApproval{
        agent_id:agent_id.clone(),profile_id:profile_id.clone(),
        profile_sha256:profile_hash.clone(),provider:"afs".into(),
    }).unwrap());
    let backup_id=wire::hex_sha(Uuid::new_v4().as_bytes());
    let metadata=serde_json::to_vec(&json!({
        "backup_id":backup_id,
        "profile_id":profile_id,
        "profile_sha256":profile_hash,
        "provider":"afs",
        "cell":"example.org",
        "volume":"user.test",
        "encrypted_bytes":ciphertext.len(),
        "recipient":recipient
    })).unwrap();
    let runtime=tokio::runtime::Builder::new_multi_thread()
        .enable_all().worker_threads(2).build().unwrap();
    let state=AppState(Arc::new(AppConfig{
        name:"ciadmin".into(),
        password_hash:String::new(),
        origin:"http://127.0.0.1:8080".into(),
        security:SecurityPolicy::from_env("http://127.0.0.1:8080").unwrap(),
        cloud_config:None,cloud_write_enabled:false,cloud_delete_enabled:false,cloud_recovery_enabled:false,
        root:PathBuf::from(temp.path()),
        identity:None,
        immutable_vault_root:None,
        s3_vault_config:None,
        exports:temp.path().join("exports"),
        audit:temp.path().join("audit.log"),
        smtp:None,dav:None,ldap:None,oidc:None,
        pending_oidc:Mutex::new(HashMap::new()),
        sessions:Mutex::new(HashMap::new()),
        failed_logins:Mutex::new(HashMap::new()),
        last_smtp_test:Mutex::new(None),
        cluster:Some(db.clone()),
        agent_proxy_key:None,
    }));
    let build_headers=|sequence|{
        let signed=wire::sign(&private,"POST","/api/agents/upload",
            &agent_id,"operator",sequence,wire::now(),
            &wire::hex_sha(&ciphertext),&metadata).unwrap();
        let mut headers=HeaderMap::new();
        headers.insert("authorization",format!("Bearer {token}").parse().unwrap());
        headers.insert("x-mb-agent-proof",
            B64.encode(serde_json::to_vec(&signed).unwrap()).parse().unwrap());
        headers.insert("x-mb-agent-metadata",B64.encode(&metadata).parse().unwrap());
        headers
    };
    let response=runtime.block_on(agent_api::upload(State(state.clone()),build_headers(1),
        Body::from(ciphertext.clone())));
    assert_eq!(response.status(),StatusCode::CREATED);
    let committed=private_spool.join(format!("{backup_id}.age"));
    assert_eq!(fs::read(&committed).unwrap(),ciphertext);
    let rows:i64=db.connect().unwrap().query_one(
        "SELECT COUNT(*) FROM mb_agent_uploads WHERE backup_id=$1",&[&backup_id])
        .unwrap().get(0);
    assert_eq!(rows,1,"Receipt must commit once in real PostgreSQL");
    let decrypted=Command::new("age").arg("-d").arg("-i")
        .arg(&key).arg(&committed).output().unwrap();
    assert!(decrypted.status.success());
    assert_eq!(decrypted.stdout,payload);

    // Exercise the SAME PostgreSQL-backed REST read-only monitoring API that
    // the native GUI consumes; reuse this real agent fixture, no duplicate CI.
    let issued=db.create_monitor_token(
        &format!("live-monitor-{}",Uuid::new_v4().simple()),7).unwrap();
    let monitor_token=issued["token"].as_str().unwrap().to_owned();
    let token_id=issued["id"].as_str().unwrap().to_owned();
    let monitor_headers=|secret:&str|{
        let mut headers=HeaderMap::new();
        headers.insert("authorization",format!("Bearer {secret}").parse().unwrap());
        headers
    };
    let anonymous=runtime.block_on(crate::monitoring::api_status(
        State(state.clone()),HeaderMap::new()));
    assert_eq!(anonymous.status(),StatusCode::UNAUTHORIZED);
    let invalid=runtime.block_on(crate::monitoring::api_agents(
        State(state.clone()),monitor_headers("invalid")));
    assert_eq!(invalid.status(),StatusCode::UNAUTHORIZED);
    let status=runtime.block_on(crate::monitoring::api_status(
        State(state.clone()),monitor_headers(&monitor_token)));
    assert_eq!(status.status(),StatusCode::OK);
    assert_eq!(status.headers()["cache-control"],"no-store");
    let bytes=runtime.block_on(axum::body::to_bytes(status.into_body(),1_048_576))
        .expect("Bounded REST JSON status");
    let live_status:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    assert_eq!(live_status["schema"],1);
    assert_eq!(live_status["read_only"],true);
    assert!(live_status["cluster"]["jobs"].is_array());
    assert!(live_status["cluster"]["replicas"].is_array());
    assert!(live_status["database"]["database"].is_object());

    let visible=runtime.block_on(crate::monitoring::api_agents(
        State(state.clone()),monitor_headers(&monitor_token)));
    assert_eq!(visible.status(),StatusCode::OK);
    assert_eq!(visible.headers()["cache-control"],"no-store");
    let bytes=runtime.block_on(axum::body::to_bytes(visible.into_body(),1_048_576))
        .expect("Bounded REST JSON agent counts");
    let counts:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    assert_eq!(counts["schema"],1);
    assert_eq!(counts["read_only"],true);
    assert!(counts["agents"]["total"].as_u64().unwrap()>=1);
    assert!(counts["agents"]["approved"].as_u64().unwrap()>=1);
    let exposed=counts.to_string();
    for secret in [&token, &monitor_token, &agent_id, &profile_hash] {
        assert!(!exposed.contains(secret),"Monitor response must hide identities and tokens");
    }
    assert!(db.revoke_monitor_token(&token_id).unwrap());
    let revoked_monitor=runtime.block_on(crate::monitoring::api_status(
        State(state.clone()),monitor_headers(&monitor_token)));
    assert_eq!(revoked_monitor.status(),StatusCode::UNAUTHORIZED);

    // Another valid signed device request with the SAME backup ID must get
    // conflict and never replace the committed age file or add a PG receipt.
    let duplicate=runtime.block_on(agent_api::upload(State(state.clone()),build_headers(2),
        Body::from(ciphertext.clone())));
    assert_eq!(duplicate.status(),StatusCode::CONFLICT);
    assert_eq!(fs::read(&committed).unwrap(),ciphertext);
    let rows:i64=db.connect().unwrap().query_one(
        "SELECT COUNT(*) FROM mb_agent_uploads WHERE backup_id=$1",&[&backup_id])
        .unwrap().get(0);
    assert_eq!(rows,1);
    assert!(agent::set_approval(&db,&agent_id,false,true).unwrap());
    let revoked=runtime.block_on(agent_api::upload(State(state),build_headers(3),
        Body::from(ciphertext)));
    assert_eq!(revoked.status(),StatusCode::UNAUTHORIZED);
    std::env::remove_var("MULTI_BACKUP_AGENT_UPLOAD_ROOT");
}
