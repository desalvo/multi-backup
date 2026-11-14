//! Reed-Solomon 8+2 protection over AGE CIPHERTEXT SHARDS, not plaintext.
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};
pub const DATA: usize = 8;
pub const PARITY: usize = 2;
pub const TOTAL: usize = DATA + PARITY;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Info { pub length: usize, pub sha256: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Stripe { pub shard_size: usize, pub info: Vec<Info> }
type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
fn sha(b: &[u8]) -> String { hex::encode(Sha256::digest(b)) }
/// Encrypt chunks first. Store the returned parity and metadata in authenticated objects.
pub fn encode(ciphertexts: &[Vec<u8>]) -> Res<(Stripe, Vec<Vec<u8>>)> {
    if ciphertexts.is_empty() || ciphertexts.len() > DATA { return Err("Invalid shard count".into()); }
    let max = ciphertexts.iter().map(Vec::len).max().unwrap_or(0);
    if max == 0 || max > 32 * 1024 * 1024 { return Err("Shard too large".into()); }
    let mut all = Vec::with_capacity(TOTAL);
    let mut info = Vec::with_capacity(TOTAL);
    for i in 0..DATA {
        let original = ciphertexts.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let mut buf = original.to_vec();
        buf.resize(max,0);
        info.push(Info {length: original.len(), sha256:sha(&buf)});
        all.push(buf);
    }
    for _ in 0..PARITY { all.push(vec![0;max]); }
    ReedSolomon::new(DATA,PARITY)?.encode(&mut all)?;
    for p in all.iter().skip(DATA) { info.push(Info {length:max,sha256:sha(p)}); }
    Ok((Stripe {shard_size:max, info}, all))
}
/// Treat every bad checksum as an erasure. Requires <=2 unavailable shards.
pub fn recover(meta: &Stripe, provided: &[Option<Vec<u8>>]) -> Res<Vec<Vec<u8>>> {
    if provided.len()!=TOTAL || meta.info.len()!=TOTAL ||
       meta.shard_size==0 || meta.shard_size > 32 * 1024 * 1024 {
        return Err("Invalid stripe".into());
    }
    let mut shards: Vec<Option<Vec<u8>>> = provided.iter().enumerate().map(|(i,v)|
        v.as_ref().filter(|b|b.len()==meta.shard_size &&
                         sha(b)==meta.info[i].sha256).cloned()).collect();
    if shards.iter().filter(|x|x.is_none()).count()>PARITY {
        return Err("Insufficient surviving shards".into());
    }
    ReedSolomon::new(DATA,PARITY)?.reconstruct(&mut shards)?;
    let mut out=Vec::new();
    for (i, shard) in shards.into_iter().enumerate() {
        let mut data = shard.ok_or("Missing reconstructed shard")?;
        if sha(&data)!=meta.info[i].sha256 { return Err("Bad reconstructed digest".into()); }
        if i<DATA {
            if meta.info[i].length>data.len(){ return Err("Invalid shard length".into()); }
            data.truncate(meta.info[i].length);
            out.push(data);
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_shards_repair(){ 
        let input: Vec<Vec<u8>> = (0..8).map(|i|vec![i as u8+1;1024+i*13]).collect();
        let (meta, encoded)=encode(&input).unwrap();
        let mut bad:Vec<Option<Vec<u8>>>=encoded.into_iter().map(Some).collect();
        bad[1].as_mut().unwrap()[0]^=127;
        bad[7]=None;
        assert_eq!(recover(&meta,&bad).unwrap(),input);
    }
    #[test]
    fn three_fail_closed(){
        let (meta,v)=encode(&[vec![1;1024]]).unwrap();
        let mut b:Vec<_>=v.into_iter().map(Some).collect();
        b[0]=None;b[1]=None;b[2]=None;
        assert!(recover(&meta,&b).is_err());
    }
}
