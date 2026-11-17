//! Mandatory externally enforced software WORM vault.
 //! The local immutable_vault module is a convenience append-only API and
 //! CANNOT survive administrator/root compromise. In contrast this adapter
 //! delegates actual versioned COMPLIANCE Object Lock and verified age recovery
 //! to an INDEPENDENT S3-compatible authority, with pinned manifest version.
 //! Without the external service, no "ransomware-proof" state is advertised.
use crate::s3_vault;
use serde::Serialize;
use std::{path::Path,fs};
use url::Url;

type Result<T>=std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Debug,Serialize)]
pub struct Attestation {
    pub backend:String,
    pub encrypted:bool,
    pub externally_enforced_object_lock:bool,
    pub retention_days:i64,
    pub protected_manifest_version_required:bool,
    pub isolated_operator_credentials_recommended:bool,
    pub same_host_root_compromise_certified:bool,
    pub guarantee:&'static str,
}
#[derive(Debug,Serialize)]
pub struct Receipt {
    pub record:s3_vault::Record,
    pub attestation:Attestation,
}
fn allow(cfg:&s3_vault::Config)->Result<()>{
    cfg.validate()?;
    let url=Url::parse(&cfg.endpoint)?;
    if url.scheme()!="https" {
        // Test-only HTTP must be explicitly requested and loopback only,
        // on a hosted disposable CI runner; never a production credential path.
        if !cfg.allow_loopback_http_for_testing ||
           std::env::var("RUNNER_ENVIRONMENT").ok().as_deref()!=Some("github-hosted"){
            return Err("Vault WORM requires trusted HTTPS outside isolated CI".into());
        }
    }
    if cfg.retention_days<1 {
        return Err("A positive externally enforced WORM retention is mandatory".into());
    }
    Ok(())
}
fn evidence(cfg:&s3_vault::Config)->Attestation{
    Attestation{backend:cfg.provider.clone(),encrypted:true,
        externally_enforced_object_lock:true,
        retention_days:cfg.retention_days,
        protected_manifest_version_required:true,
        isolated_operator_credentials_recommended:true,
        same_host_root_compromise_certified:false,
        guarantee:"Remote versioned COMPLIANCE retention proved; host-side root, credentials, service operator and storage failure are separate threat boundaries"}
}
fn pinned(record:&s3_vault::Record)->Result<()> {
    if record.manifest_version.as_deref().unwrap_or("").is_empty() ||
       record.archive_version.is_empty() ||
       record.sha256.len()!=64 ||record.bytes==0 || record.files==0{
        return Err("Vault receipt lacks independently pinned protected manifest/archive evidence".into());
    }
    Ok(())
}
pub fn preflight(cfg:&s3_vault::Config)->Result<Attestation>{
    allow(cfg)?;
    s3_vault::preflight(cfg)?;
    Ok(evidence(cfg))
}
pub fn backup(cfg:&s3_vault::Config,source:&Path,
              identity:&Path)->Result<Receipt>{
    preflight(cfg)?;
    // The caller MUST demonstrate possession of the matching private age
    // identity before a backup is committed as a successful recoverable unit.
    let meta=fs::symlink_metadata(identity)?;
    if !meta.file_type().is_file() || meta.file_type().is_symlink() ||
       meta.len()==0 || meta.len()>65536 {
        return Err("Vault recovery identity must be a regular bounded file".into());
    }
    let record=s3_vault::backup(cfg,source)?;
    pinned(&record)?;
    let version=record.manifest_version.as_ref().ok_or("WORM manifest version missing")?;
    s3_vault::verify_at_version(cfg,&record.id,version,identity)?;
    Ok(Receipt{record,attestation:evidence(cfg)})
}
pub fn verify(cfg:&s3_vault::Config,id:&str,manifest_version:&str,
              identity:&Path)->Result<Receipt>{
    preflight(cfg)?;
    if manifest_version.is_empty(){return Err("Vault recovery requires escrowed manifest version ID".into());}
    let record=s3_vault::verify_at_version(cfg,id,manifest_version,identity)?;
    pinned(&record)?;
    Ok(Receipt{record,attestation:evidence(cfg)})
}
pub fn restore(cfg:&s3_vault::Config,id:&str,manifest_version:&str,
               identity:&Path,destination:&Path)->Result<Receipt>{
    preflight(cfg)?;
    if manifest_version.is_empty(){return Err("Vault restore requires pinned manifest version ID".into());}
    let record=s3_vault::restore_at_version(cfg,id,manifest_version,identity,destination)?;
    pinned(&record)?;
    Ok(Receipt{record,attestation:evidence(cfg)})
}
#[cfg(test)]
mod tests{
    use super::*;
    fn sample()->s3_vault::Config{
        s3_vault::Config{provider:"rustfs".to_owned(),
            endpoint:"http://127.0.0.1:9000".to_owned(),
            region:"us-east-1".to_owned(),bucket:"mb-test-worm".to_owned(),
            prefix:"backup".to_owned(),recipient:"age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq".to_owned(),
            retention_days:30,allow_loopback_http_for_testing:true}
    }
    #[test]fn insecure_test_endpoints_do_not_masquerade_as_production_worm(){
        let c=sample();
        if std::env::var("RUNNER_ENVIRONMENT").ok().as_deref()!=Some("github-hosted"){
            assert!(allow(&c).is_err());
        }
        let mut tls=c.clone();tls.endpoint="https://vault.example.test".to_owned();
        tls.allow_loopback_http_for_testing=false;
        assert!(allow(&tls).is_ok());
        tls.retention_days=0;
        assert!(allow(&tls).is_err());
        tls.retention_days=30;
        assert!(!evidence(&tls).same_host_root_compromise_certified);
        assert!(evidence(&tls).protected_manifest_version_required);
    }
    #[test]fn incomplete_receipts_never_publish_worm_verified(){
        let record=s3_vault::Record{schema:1,id:"0".repeat(32),
            archive_key:"backup/x".to_owned(),archive_version:"archive-v1".to_owned(),
            manifest_version:None,sha256:"a".repeat(64),bytes:10,files:1,
            retain_until:"2030-01-01T00:00:00Z".to_owned()};
        assert!(pinned(&record).is_err());
        let mut good=record;good.manifest_version=Some("manifest-v1".to_owned());
        assert!(pinned(&good).is_ok());
    }
}
