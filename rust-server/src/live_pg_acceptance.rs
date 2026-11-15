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
