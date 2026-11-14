//! Shared wire contract for native agents. Two independent factors:
//! a high-entropy per-agent bearer token and an approved Ed25519 device key.
//! Every request is bound to method/path/body checksum/metadata, sequence and
//! timestamp. Server persists the last accepted sequence across restarts.
//! TLS 1.2+/trusted certificate validation is mandatory at transport layer.
use base64::{engine::general_purpose::STANDARD as B64,Engine as _};
use ed25519_dalek::{Signature,SigningKey,VerifyingKey,Signer,Verifier};
use rand::rngs::OsRng;
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::time::{SystemTime,UNIX_EPOCH};

pub const CLOCK_SKEW_SECONDS:i64=300;
pub const MAX_AGENT_ID:usize=96;
pub const MAX_METADATA_BYTES:usize=16*1024;

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProof{
    pub agent_id:String,
    pub user:String,
    pub sequence:u64,
    pub unix_seconds:i64,
    pub body_sha256:String,
    pub metadata_sha256:String,
    pub signature:String,
}
pub fn now()->i64{SystemTime::now().duration_since(UNIX_EPOCH)
    .map(|d|d.as_secs() as i64).unwrap_or(0)}
pub fn hex_sha(payload:&[u8])->String{hex::encode(Sha256::digest(payload))}
pub fn token_sha(token:&str)->String{hex_sha(token.as_bytes())}
pub fn device_keypair()->([u8;32],String){
    let sk=SigningKey::generate(&mut OsRng);
    (sk.to_bytes(),B64.encode(sk.verifying_key().to_bytes()))
}
pub fn valid_id(value:&str)->bool{
    !value.is_empty()&&value.len()<=MAX_AGENT_ID&&
    value.bytes().all(|v|v.is_ascii_alphanumeric()||b"._-".contains(&v))
}
pub fn valid_user(value:&str)->bool{
    !value.is_empty()&&value.len()<=254&&
    value.bytes().all(|v|v.is_ascii_alphanumeric()||b"@._-+".contains(&v))
}
fn canonical(method:&str,path:&str,proof:&AgentProof)->Result<String,String>{
    if !valid_id(&proof.agent_id)||!valid_user(&proof.user)||
        proof.sequence==0||proof.unix_seconds<=0||
        !(method=="GET"||method=="POST")||
        !path.starts_with("/api/agents/")||path.len()>512||
        path.bytes().any(|b|b<=32||b>=127)||
        proof.body_sha256.len()!=64||proof.metadata_sha256.len()!=64||
        !proof.body_sha256.bytes().all(|b|b.is_ascii_hexdigit())||
        !proof.metadata_sha256.bytes().all(|b|b.is_ascii_hexdigit()){
        return Err("Invalid signed agent envelope".into());
    }
    Ok(format!("multi-backup-agent-v1\n{method}\n{path}\n{}\n{}\n{}\n{}\n{}\n{}",
        proof.agent_id,proof.user,proof.sequence,proof.unix_seconds,
        proof.body_sha256,proof.metadata_sha256))
}
pub fn sign(private:&[u8;32],method:&str,path:&str,id:&str,user:&str,
            sequence:u64,unix_seconds:i64,body_sha:&str,metadata:&[u8])
    ->Result<AgentProof,String>{
    if metadata.len()>MAX_METADATA_BYTES{return Err("Too much agent metadata".into());}
    let mut proof=AgentProof{
        agent_id:id.into(),user:user.into(),sequence,unix_seconds,
        body_sha256:body_sha.into(),metadata_sha256:hex_sha(metadata),
        signature:String::new(),
    };
    let text=canonical(method,path,&proof)?;
    proof.signature=B64.encode(SigningKey::from_bytes(private).sign(text.as_bytes()).to_bytes());
    Ok(proof)
}
pub fn verify(public:&str,proof:&AgentProof,method:&str,path:&str,
              metadata:&[u8],last_sequence:u64,time:i64)
    ->Result<(),String>{
    if proof.sequence<=last_sequence || proof.unix_seconds.saturating_sub(time).abs()>CLOCK_SKEW_SECONDS{
        return Err("Expired or replayed agent request".into());
    }
    if metadata.len()>MAX_METADATA_BYTES||proof.metadata_sha256!=hex_sha(metadata){
        return Err("Agent metadata authentication failed".into());
    }
    let keybytes=B64.decode(public).map_err(|_|"Malformed agent public key")?;
    let array:[u8;32]=keybytes.as_slice().try_into().map_err(|_|"Invalid device key length")?;
    let key=VerifyingKey::from_bytes(&array).map_err(|_|"Invalid device public key")?;
    let sig=B64.decode(&proof.signature).map_err(|_|"Malformed device signature")?;
    let signature=Signature::from_slice(&sig).map_err(|_|"Invalid device signature")?;
    let payload=canonical(method,path,proof)?;
    key.verify(payload.as_bytes(),&signature).map_err(|_|"Device signature rejected".into())
}
pub fn valid_tls_endpoint(value:&str)->bool{
    let Ok(url)=url::Url::parse(value) else {return false};
    url.scheme()=="https"&&url.username().is_empty()&&url.password().is_none()&&
    url.host_str().is_some()&&url.fragment().is_none()&&url.query().is_none()&&
    url.path()=="/"
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn approve_id_and_reject_replay_or_tamper(){
        let (private,public)=device_keypair();
        let now=now();
        let metadata=b"volume=user.a;backup=abc";
        let signed=sign(&private,"POST","/api/agents/upload","worker1","alice",1,
            now,&hex_sha(b"ciphertext"),metadata).unwrap();
        verify(&public,&signed,"POST","/api/agents/upload",metadata,0,now).unwrap();
        assert!(verify(&public,&signed,"POST","/api/agents/upload",metadata,1,now).is_err());
        assert!(verify(&public,&signed,"POST","/api/agents/upload",b"other",0,now).is_err());
        assert!(verify(&public,&signed,"GET","/api/agents/upload",metadata,0,now).is_err());
        assert!(verify(&public,&signed,"POST","/api/agents/upload",metadata,0,now+301).is_err());
        let (_other,wrong)=device_keypair();
        assert!(verify(&wrong,&signed,"POST","/api/agents/upload",metadata,0,now).is_err());
    }
    #[test]fn deny_weak_endpoint_or_identity(){
        assert!(!valid_tls_endpoint("http://backup.example/"));
        assert!(!valid_tls_endpoint("https://user:password@backup.example/"));
        assert!(valid_tls_endpoint("https://backup.example/"));
        assert!(!valid_id("../foo"));
    }
}