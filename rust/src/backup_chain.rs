//! Full/incremental logical lineage, chain verification and exact byte metrics.
//! Every snapshot contains a complete encrypted manifest of chunk references;
//! incrementals reuse deduplicated chunks, not fragile delta-only payloads.
use crate::chunk_store::{age, BackupKind, Entry, LogicalBackup, Store};
use serde::Serialize;
use std::{collections::{HashMap, HashSet}, path::Path};

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Serialize)]
pub struct SnapshotMetrics {
    pub backup_id: String,
    pub kind: BackupKind,
    pub file_count: usize,
    /// Size of the complete file tree, before compression.
    pub uncompressed_bytes: u64,
    /// Sum of zstd payload sizes for all referenced chunks, before dedup.
    pub compressed_bytes: u64,
    /// Unique referenced zstd payload sizes, excluding encryption overhead.
    pub deduplicated_compressed_bytes: u64,
    /// Chunks in this snapshot that were absent in the direct predecessor.
    pub incremental_new_compressed_bytes: u64,
    pub added_or_modified_files: usize,
    pub deleted_files: usize,
}

#[derive(Debug, Serialize)]
pub struct BackupDetails {
    pub backup_id: String,
    pub full_reference_id: String,
    pub previous_increment_id: Option<String>,
    pub kind: BackupKind,
    pub snapshot: SnapshotMetrics,
    /// Ordered from root full to the selected increment; includes root stats.
    pub chain: Vec<SnapshotMetrics>,
}

#[derive(Debug, Serialize)]
pub struct ChainVerification {
    pub backup_id: String,
    pub full_reference_id: String,
    pub full_chain_verified: bool,
    pub verified_ids: Vec<String>,
    pub healthy: bool,
    pub issues: Vec<String>,
}

/// An incremental is individually restorable, but its lineage must remain
/// anchored in a full, non-cyclic, authenticated manifest.
pub fn ancestors(store: &Store, key: &Path, id: &str) -> Res<Vec<LogicalBackup>> {
    let mut seen = HashSet::new();
    let mut current = id.to_owned();
    let mut chain = Vec::new();
    loop {
        if !seen.insert(current.clone()) || chain.len() >= 10_000 {
            return Err("Cycle or excessive depth in incremental backup chain".into());
        }
        let manifest = store.load(key, &current)?;
        let next = match manifest.kind {
            BackupKind::Full => None,
            BackupKind::Incremental => Some(
                manifest.parent_id.as_deref().ok_or("Incremental has no predecessor")?.to_owned()
            ),
        };
        chain.push(manifest);
        if let Some(parent) = next {
            current = parent;
        } else {
            break;
        }
    }
    chain.reverse();
    // Mixing unrelated logical namespaces in a backup chain is never safe.
    let root_ns = &chain[0].top_namespace;
    if chain.iter().any(|m| &m.top_namespace != root_ns) {
        return Err("Incremental chain crosses unrelated logical namespaces".into());
    }
    Ok(chain)
}

fn chunks<'a>(entries: &'a [Entry]) -> impl Iterator<Item=&'a crate::chunk_store::ChunkRef> {
    entries.iter().flat_map(|e| e.chunks.iter())
}

fn metric(store:&Store,key:&Path,backup:&LogicalBackup,parent:Option<&LogicalBackup>)
    -> Res<SnapshotMetrics>
{
    let mut sizes: HashMap<String,u64> = HashMap::new();
    let mut compressed_bytes=0u64;
    let mut unique_bytes=0u64;
    let mut uncompressed_bytes=0u64;
    let old_ids:HashSet<&str> = parent.into_iter()
        .flat_map(|b|chunks(&b.entries)).map(|c|c.id.as_str()).collect();
    let mut new_bytes=0u64;
    for entry in &backup.entries {
        uncompressed_bytes=uncompressed_bytes.checked_add(entry.bytes)
            .ok_or("Uncompressed size overflow")?;
        for chunk in &entry.chunks {
            let (bytes,first) = match sizes.get(&chunk.id) {
                Some(value)=>(*value,false),
                None=>{
                    let value=store.compressed_chunk_bytes(key,chunk)?;
                    sizes.insert(chunk.id.clone(),value);
                    (value,true)
                }
            };
            compressed_bytes=compressed_bytes.checked_add(bytes).ok_or("Compressed size overflow")?;
            if first {
                unique_bytes=unique_bytes.checked_add(bytes).ok_or("Deduplicated size overflow")?;
                if !old_ids.contains(chunk.id.as_str()) {
                    new_bytes=new_bytes.checked_add(bytes).ok_or("Incremental size overflow")?;
                }
            }
        }
    }
    let current:HashMap<&str,&Entry>=backup.entries.iter()
        .map(|e|(e.path.as_str(),e)).collect();
    let old:HashMap<&str,&Entry>=parent.into_iter().flat_map(|b|b.entries.iter())
        .map(|e|(e.path.as_str(),e)).collect();
    let modified=current.iter().filter(|(path,entry)| {
        old.get(path).is_none_or(|old_entry|
            old_entry.bytes!=entry.bytes ||
            old_entry.chunks.len()!=entry.chunks.len() ||
            old_entry.chunks.iter().zip(&entry.chunks).any(|(a,b)|a.id!=b.id))
    }).count();
    let deleted=old.keys().filter(|path|!current.contains_key(*path)).count();
    Ok(SnapshotMetrics {
        backup_id:backup.id.clone(),kind:backup.kind,
        file_count:backup.entries.len(),uncompressed_bytes,
        compressed_bytes,deduplicated_compressed_bytes:unique_bytes,
        incremental_new_compressed_bytes:new_bytes,
        added_or_modified_files:modified,deleted_files:deleted,
    })
}

pub fn details(store:&Store,key:&Path,id:&str)->Res<BackupDetails>{
    let chain=ancestors(store,key,id)?;
    let full_reference_id=chain[0].id.clone();
    let mut snapshots=Vec::new();
    for (i,item) in chain.iter().enumerate() {
        snapshots.push(metric(store,key,item,if i==0 {None}else{Some(&chain[i-1])})?);
    }
    let selected=chain.last().ok_or("Empty backup chain")?;
    let current=snapshots.last().ok_or("No backup statistics")?;
    Ok(BackupDetails{
        backup_id:id.to_owned(),full_reference_id,
        previous_increment_id:selected.parent_id.clone(),
        kind:selected.kind,snapshot:SnapshotMetrics{
            backup_id:current.backup_id.clone(),kind:current.kind,
            file_count:current.file_count,uncompressed_bytes:current.uncompressed_bytes,
            compressed_bytes:current.compressed_bytes,
            deduplicated_compressed_bytes:current.deduplicated_compressed_bytes,
            incremental_new_compressed_bytes:current.incremental_new_compressed_bytes,
            added_or_modified_files:current.added_or_modified_files,
            deleted_files:current.deleted_files
        },chain:snapshots
    })
}

pub fn verify_chain(store:&Store,key:&Path,id:&str,full_chain:bool)->Res<ChainVerification>{
    let chain=ancestors(store,key,id)?;
    let full_reference_id=chain[0].id.clone();
    let mut issues=Vec::new();
    let mut verified_ids=Vec::new();
    let selected=if full_chain{&chain[..]}else{&chain[chain.len()-1..]};
    for snapshot in selected {
        let report=store.verify(key,&snapshot.id)?;
        verified_ids.push(snapshot.id.clone());
        issues.extend(report.issues.into_iter().map(|v|format!("{}: {v}",snapshot.id)));
    }
    Ok(ChainVerification{backup_id:id.into(),full_reference_id,
        full_chain_verified:full_chain,verified_ids,
        healthy:issues.is_empty(),issues})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn no_undocumented_full_chain_default() {
        assert_eq!(BackupKind::parse("full").unwrap(),BackupKind::Full);
        assert_eq!(BackupKind::parse("incremental").unwrap(),BackupKind::Incremental);
        assert!(BackupKind::parse("differential").is_err());
    }
}