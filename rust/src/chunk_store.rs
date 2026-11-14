//! Opt-in v1 content-addressed store: one logical backup manifest, encrypted chunks.
//! This does NOT modify existing tar.gz.age archives. Chunks use HMAC-SHA256 IDs.
use std::{collections::HashSet, fs::{self, File, OpenOptions}, io::{Read, Write},
    path::{Component, Path, PathBuf}, process::{Command, Stdio}};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Serialize, Deserialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use walkdir::WalkDir;
type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type HmacSha = Hmac<Sha256>;
const SUBDIR: &str = ".multi-backup-v1";
pub const DEFAULT_CHUNK: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkRef { pub id: String, pub sha256: String, pub bytes: usize }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry { pub path: String, pub bytes: u64, pub chunks: Vec<ChunkRef> }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogicalBackup {
    pub version: u32, pub id: String, pub label: String, pub entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Config { version: u32, recipient: String, chunk_bytes: usize, secret: String }
pub struct Source { pub namespace: String, pub directory: PathBuf }
pub struct Store { root: PathBuf, cfg: Config, key: [u8;32] }
fn sha(data: &[u8]) -> String { hex::encode(Sha256::digest(data)) }
pub(crate) fn safe_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase())
}
fn safe_path(s: &str) -> bool {
    if s.len()>4096 || s.is_empty() || s.chars().any(|c|
        c.is_control() || r#"\:*?"<>|"#.contains(c)) { return false; }
    s.split('/').all(|v| {
        if v.is_empty() || v=="." || v==".." || v.ends_with(' ') || v.ends_with('.') {
            return false;
        }
        let part=v.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(part.as_str(),"CON"|"PRN"|"AUX"|"NUL"|"COM1"|"COM2"|"COM3"|
            "COM4"|"COM5"|"COM6"|"COM7"|"COM8"|"COM9"|"LPT1"|"LPT2"|
            "LPT3"|"LPT4"|"LPT5"|"LPT6"|"LPT7"|"LPT8"|"LPT9")
    })
}
pub(crate) fn dir(path: &Path) -> Res<()> {
    if path.exists() {
        if !fs::symlink_metadata(path)?.file_type().is_dir() { return Err("Unsafe directory".into()); }
    } else { fs::create_dir_all(path)?; }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn write_new(path: &Path, payload: &[u8]) -> Res<bool> {
    dir(path.parent().ok_or("Missing parent")?)?;
    let mut temp = NamedTempFile::new_in(path.parent().ok_or("Missing parent")?)?;
    temp.write_all(payload)?;
    temp.as_file().sync_all()?;
    match temp.persist_noclobber(path) {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.error.into()),
    }
}
pub(crate) fn age(payload: &[u8], recipient: Option<&str>, identity: Option<&Path>) -> Res<Vec<u8>> {
    if recipient.is_some() == identity.is_some() { return Err("Age needs recipient XOR identity".into()); }
    let mut cmd = Command::new("age");
    if let Some(recipient) = recipient {
        if !recipient.starts_with("age1") || recipient.len() > 128 { return Err("Invalid recipient".into()); }
        cmd.arg("-r").arg(recipient);
    } else { cmd.arg("-d").arg("-i").arg(identity.ok_or("Missing identity")?); }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped())
        .stderr(Stdio::null()).spawn()?;
    let mut input = child.stdin.take().ok_or("Age stdin missing")?;
    let buffer = payload.to_vec();
    let producer = std::thread::spawn(move || -> std::io::Result<()> {
        let status = input.write_all(&buffer);
        drop(input);
        status
    });
    let result = child.wait_with_output()?;
    producer.join().map_err(|_| "Age input thread panicked")??;
    if !result.status.success() { return Err("Age operation failed authentication".into()); }
    Ok(result.stdout)
}
impl Store {
    fn base(root: &Path) -> PathBuf { root.join(SUBDIR) }
    fn manifest_path(&self, id: &str) -> Res<PathBuf> {
        if !safe_hash(id) { return Err("Unsafe manifest ID".into()); }
        Ok(self.root.join("manifests").join(format!("{id}.json.age")))
    }
    fn chunk_path(&self, id: &str) -> Res<PathBuf> {
        if !safe_hash(id) { return Err("Unsafe chunk ID".into()); }
        Ok(self.root.join("chunks").join(&id[..2]).join(&id[2..4]).join(format!("{id}.age")))
    }
    fn id(&self, bytes: &[u8]) -> Res<String> {
        let mut mac = HmacSha::new_from_slice(&self.key)?;
        mac.update(b"multi-backup-v1-chunk\0");
        mac.update(bytes);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }
    pub fn init(root: &Path, recipient: &str, chunk_bytes: usize) -> Res<()> {
        if !(1024 * 1024..=16 * 1024 * 1024).contains(&chunk_bytes) {
            return Err("Chunk size must be 1..16 MiB".into());
        }
        dir(root)?;
        let private = Self::base(root);
        dir(&private)?;
        let mut key = [0u8;32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        let cfg = Config {version: 1, recipient: recipient.to_owned(), chunk_bytes,
            secret: hex::encode(key)};
        let ciphertext = age(&serde_json::to_vec(&cfg)?, Some(recipient), None)?;
        if !write_new(&private.join("config.json.age"), &ciphertext)? {
            return Err("Storage already initialized".into());
        }
        Ok(())
    }
    pub fn open(root: &Path, identity: &Path) -> Res<Self> {
        let private = Self::base(root);
        if !fs::symlink_metadata(&private)?.file_type().is_dir() {
            return Err("Storage path must not be a symlink".into());
        }
        let conf = private.join("config.json.age");
        if !fs::symlink_metadata(&conf)?.file_type().is_file() {
            return Err("Configuration must not be a symlink".into());
        }
        let decoded = age(&fs::read(conf)?, None, Some(identity))?;
        let cfg: Config = serde_json::from_slice(&decoded)?;
        if cfg.version != 1 || !(1024 * 1024..=16 * 1024 * 1024).contains(&cfg.chunk_bytes) {
            return Err("Unsupported store schema".into());
        }
        let decoded_key = hex::decode(&cfg.secret)?;
        let key: [u8;32] = decoded_key.try_into().map_err(|_| "Invalid store key")?;
        Ok(Self {root: private, cfg, key})
    }
    fn read_chunk(&self, identity: &Path, chunk: &ChunkRef) -> Res<Vec<u8>> {
        if !safe_hash(&chunk.id) || !safe_hash(&chunk.sha256) ||
            chunk.bytes > self.cfg.chunk_bytes { return Err("Invalid chunk reference".into()); }
        let path = self.chunk_path(&chunk.id)?;
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err("Chunk missing or unsafe symlink".into());
        }
        let compressed = age(&fs::read(path)?, None, Some(identity))?;
        let decoder = zstd::stream::Decoder::new(compressed.as_slice())?;
        let mut plain = Vec::new();
        decoder.take((self.cfg.chunk_bytes + 1) as u64).read_to_end(&mut plain)?;
        if plain.len() != chunk.bytes || sha(&plain) != chunk.sha256 ||
            self.id(&plain)? != chunk.id { return Err("Corrupt chunk".into()); }
        Ok(plain)
    }
    fn save_chunk(&self, identity: &Path, bytes: &[u8]) -> Res<ChunkRef> {
        let chunk = ChunkRef {id: self.id(bytes)?, sha256: sha(bytes), bytes: bytes.len()};
        let path = self.chunk_path(&chunk.id)?;
        if path.exists() {
            if self.read_chunk(identity, &chunk)? != bytes {
                return Err("Existing chunk failed checksum".into());
            }
        } else {
            let compressed = zstd::stream::encode_all(bytes, 3)?;
            let encrypted = age(&compressed, Some(&self.cfg.recipient), None)?;
            if !write_new(&path, &encrypted)? &&
                self.read_chunk(identity, &chunk)? != bytes {
                return Err("Concurrent immutable chunk conflict".into());
            }
        }
        Ok(chunk)
    }
    /// A single manifest covers ALL GitHub/CI/metadata/Dropbox virtual namespaces.
    pub fn backup(&self, identity: &Path, label: &str, sources: &[Source]) -> Res<LogicalBackup> {
        if label.len() > 256 || sources.is_empty() { return Err("Invalid label/sources".into()); }
        let lock = OpenOptions::new().create(true).write(true).open(self.root.join(".lock"))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        let mut entries = Vec::new();
        let mut visited = HashSet::new();
        for src in sources {
            if src.namespace.is_empty() || src.namespace.len() > 80 ||
                !src.namespace.bytes().all(|b|
                   b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return Err("Unsafe namespace".into());
            }
            if !fs::symlink_metadata(&src.directory)?.file_type().is_dir() {
                return Err("Source must be a directory, not symlink".into());
            }
            for file in WalkDir::new(&src.directory).follow_links(false).max_depth(64) {
                let file = file?;
                if file.file_type().is_dir() { continue; }
                if !file.file_type().is_file() { return Err("Symlink/special file rejected".into()); }
                let relative = file.path().strip_prefix(&src.directory)?;
                let mut components = Vec::new();
                for part in relative.components() {
                    if let Component::Normal(name) = part {
                        components.push(name.to_str().ok_or("Non UTF-8 path")?.to_string());
                    } else { return Err("Unsafe relative component".into()); }
                }
                let vpath = format!("{}/{}", src.namespace, components.join("/"));
                if !safe_path(&vpath) || !visited.insert(vpath.clone()) {
                    return Err("Unsafe/duplicate logical file".into());
                }
                let mut input = File::open(file.path())?;
                let mut chunks = Vec::new();
                let mut bytes = 0u64;
                loop {
                    let mut block = Vec::new();
                    (&mut input).take(self.cfg.chunk_bytes as u64).read_to_end(&mut block)?;
                    if block.is_empty() { break; }
                    bytes = bytes.checked_add(block.len() as u64).ok_or("File too large")?;
                    chunks.push(self.save_chunk(identity, &block)?);
                }
                entries.push(Entry {path: vpath, bytes, chunks});
            }
        }
        entries.sort_by(|a,b|a.path.cmp(&b.path));
        let mut random = [0u8;32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let snapshot = LogicalBackup {version: 1, id: hex::encode(random),
            label: label.to_owned(), entries};
        let encrypted = age(&serde_json::to_vec(&snapshot)?, Some(&self.cfg.recipient), None)?;
        if !write_new(&self.manifest_path(&snapshot.id)?, &encrypted)? {
            return Err("Manifest ID conflict".into());
        }
        fs2::FileExt::unlock(&lock)?;
        self.protect(identity, &snapshot.id)?;
        Ok(snapshot)
    }
    pub fn load(&self, identity: &Path, id: &str) -> Res<LogicalBackup> {
        let path = self.manifest_path(id)?;
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err("Unsafe manifest".into());
        }
        let data = age(&fs::read(path)?, None, Some(identity))?;
        let value: LogicalBackup = serde_json::from_slice(&data)?;
        if value.version != 1 || value.id != id { return Err("Manifest identity mismatch".into()); }
        let mut names = HashSet::new();
        for item in &value.entries {
            if !safe_path(&item.path) || !names.insert(&item.path) { return Err("Unsafe manifest path".into()); }
            if item.chunks.iter().try_fold(0u64, |s,c|
               s.checked_add(c.bytes as u64)) != Some(item.bytes) {
                return Err("Invalid file length in manifest".into());
            }
        }
        Ok(value)
    }
    pub fn list(&self, identity: &Path) -> Res<Vec<LogicalBackup>> {
        let folder = self.root.join("manifests");
        if !folder.exists() { return Ok(Vec::new()); }
        let mut snapshots = Vec::new();
        for item in fs::read_dir(folder)? {
            let path = item?.path();
            if let Some(id) = path.file_name().and_then(|n|n.to_str())
                .and_then(|n|n.strip_suffix(".json.age")) {
                snapshots.push(self.load(identity, id)?);
            }
        }
        snapshots.sort_by(|a,b|a.id.cmp(&b.id));
        Ok(snapshots)
    }
    /// Restore a file, folder, or whole logical backup into a fresh directory.
    pub fn restore(&self, identity: &Path, id: &str, selection: Option<&str>,
                   dest: &Path) -> Res<usize> {
        let selected: Vec<String> = selection.iter().map(|s|s.to_string()).collect();
        self.restore_many(identity,id,&selected,dest)
    }
    /// Multi-select file manager restore, combining arbitrary files/folders in
    /// the SAME logical backup, with a single atomic fresh destination.
    pub fn restore_many(&self, identity: &Path, id: &str, selections:&[String],
                   dest: &Path) -> Res<usize> {
        if dest.exists(){return Err("Never overwrite a restore target".into());}
        if selections.len()>1000||selections.iter().any(|s|!safe_path(s)){
            return Err("Invalid file/folder selection".into());
        }
        self.repair(identity,id)?;
        let manifest=self.load(identity,id)?;
        let parent=dest.parent().ok_or("Missing parent")?;
        if !parent.is_dir(){return Err("Restore parent missing".into());}
        let stage=tempfile::Builder::new().prefix(".mb-restore-").tempdir_in(parent)?;
        let mut count=0;
        for file in &manifest.entries {
            if !selections.is_empty()&&!selections.iter().any(|sel|
                file.path==*sel||file.path.starts_with(&format!("{sel}/"))){
                continue;
            }
            let target=stage.path().join(&file.path);
            dir(target.parent().ok_or("Invalid target")?)?;
            let mut output=OpenOptions::new().write(true).create_new(true).open(target)?;
            for chunk in &file.chunks {
                output.write_all(&self.read_chunk(identity,chunk)?)?;
            }
            output.sync_all()?;
            count+=1;
        }
        if count==0 && !selections.is_empty() {
            return Err("No matching selected files or folders".into());
        }
        fs::rename(stage.path(),dest)?;
        Ok(count)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_and_unclean_virtual_names() {
        for s in ["../passwd", "github//repo", "github/./repo",
                  "github/../../etc", "github\\evil", "/root"] {
            assert!(!safe_path(s), "{s}");
        }
        assert!(safe_path("github/user/repository/refs/heads/main"));
        assert!(!safe_hash("invalid"));
        assert!(safe_hash(&"a".repeat(64)));
    }
    #[test]
    fn keyed_name_does_not_disclose_content_sha256() {
        let st = Store {root: PathBuf::new(),
            cfg: Config {version:1, recipient: "age1notused".into(),
                chunk_bytes: DEFAULT_CHUNK, secret: "".into()},
            key: [1u8;32]};
        let a = st.id(b"same").unwrap();
        assert_eq!(a, st.id(b"same").unwrap());
        assert_ne!(a, sha(b"same"));
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Protection {
    id: String,
    members: Vec<ChunkRef>,
    stripe: crate::parity::Stripe,
}
impl Store {
    fn read_protection(&self, identity: &Path) -> Res<Vec<Protection>> {
        let folder = self.root.join("stripes");
        if !folder.exists() { return Ok(Vec::new()); }
        let mut found = Vec::new();
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|x|x.to_str()).ok_or("Unsafe stripe filename")?;
            if let Some(id) = name.strip_suffix(".json.age") {
                if !safe_hash(id) || !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err("Unsafe stripe metadata".into());
                }
                let plain = age(&fs::read(&path)?,None,Some(identity))?;
                let stripe: Protection = serde_json::from_slice(&plain)?;
                if stripe.id != id || stripe.members.is_empty() ||
                    stripe.members.len() > crate::parity::DATA ||
                    stripe.stripe.info.len() != crate::parity::TOTAL ||
                    stripe.stripe.shard_size == 0 ||
                    stripe.stripe.shard_size > 32 * 1024 * 1024 {
                    return Err("Invalid authenticated stripe metadata".into());
                }
                found.push(stripe);
            }
        }
        Ok(found)
    }
    /// Create 8+2 stripes over age ciphertext. Parity itself is encrypted to
    /// the same age recipient, and stripe metadata is independently encrypted.
    /// Existing protected chunk IDs are reused without creating duplicate parity.
    pub fn protect(&self, identity: &Path, backup_id: &str) -> Res<usize> {
        let backup = self.load(identity, backup_id)?;
        let previous = self.read_protection(identity)?;
        let protected: HashSet<String> = previous.iter().flat_map(|s|
            s.members.iter().map(|m|m.id.clone())).collect();
        let mut unique = HashSet::new();
        let mut pending = Vec::new();
        for entry in &backup.entries {
            for item in &entry.chunks {
                if !protected.contains(&item.id) && unique.insert(item.id.clone()) {
                    self.read_chunk(identity, item)?; // verify before computing parity
                    pending.push(item.clone());
                }
            }
        }
        let mut count = 0;
        for group in pending.chunks(crate::parity::DATA) {
            let ciphertext: Vec<Vec<u8>> = group.iter()
                .map(|item| -> Res<Vec<u8>> { Ok(fs::read(self.chunk_path(&item.id)?)?) })
                .collect::<Res<Vec<_>>>()?;
            let (meta, shards) = crate::parity::encode(&ciphertext)?;
            let mut nonce = [0u8;32];
            rand::rngs::OsRng.fill_bytes(&mut nonce);
            let id = hex::encode(nonce);
            let folder = self.root.join("parity").join(&id[..2]).join(&id);
            for offset in 0..crate::parity::PARITY {
                let encrypted = age(&shards[crate::parity::DATA+offset],
                    Some(&self.cfg.recipient),None)?;
                if !write_new(&folder.join(format!("{}.age",offset)),&encrypted)? {
                    return Err("Parity filename conflict".into());
                }
            }
            let index = Protection {id:id.clone(), members:group.to_vec(),stripe:meta};
            let protected_index = age(&serde_json::to_vec(&index)?,
                Some(&self.cfg.recipient),None)?;
            if !write_new(&self.root.join("stripes").join(format!("{id}.json.age")),
                &protected_index)? {return Err("Stripe metadata conflict".into());}
            count += 1;
        }
        Ok(count)
    }
    fn decode_cipher(&self, identity: &Path, reference: &ChunkRef,
                     encrypted: &[u8]) -> Res<()> {
        let compressed = age(encrypted,None,Some(identity))?;
        let decoder = zstd::stream::Decoder::new(compressed.as_slice())?;
        let mut plain = Vec::new();
        decoder.take((self.cfg.chunk_bytes+1) as u64).read_to_end(&mut plain)?;
        if plain.len()!=reference.bytes || sha(&plain)!=reference.sha256 ||
           self.id(&plain)?!=reference.id {
            return Err("Recovered ciphertext does not match authenticated manifest".into());
        }
        Ok(())
    }
    /// Detect corruption by the authenticated plaintext checksum; recover from
    /// at most two absent/corrupted shards per stripe. Move damaged ciphertext
    /// into quarantine before installing a fully verified reconstructed chunk.
    pub fn repair(&self, identity: &Path, backup_id: &str) -> Res<usize> {
        let backup = self.load(identity, backup_id)?;
        let protections = self.read_protection(identity)?;
        let mut unique = HashSet::new();
        let mut count = 0;
        for file in &backup.entries {
            for item in &file.chunks {
                if !unique.insert(item.id.clone()) {continue;}
                if self.read_chunk(identity,item).is_ok() {continue;}
                let protection = protections.iter().find(|s|
                    s.members.iter().any(|m| m.id==item.id))
                    .ok_or("Corrupt chunk has no parity protection")?;
                let mut shards = Vec::with_capacity(crate::parity::TOTAL);
                for member in &protection.members {
                    shards.push(fs::read(self.chunk_path(&member.id)?).ok());
                }
                while shards.len()<crate::parity::DATA {shards.push(Some(vec![0;
                    protection.stripe.shard_size]));}
                let folder = self.root.join("parity")
                    .join(&protection.id[..2]).join(&protection.id);
                for p in 0..crate::parity::PARITY {
                    let path = folder.join(format!("{p}.age"));
                    let contents = fs::read(path).ok().and_then(|buf|
                        age(&buf,None,Some(identity)).ok());
                    shards.push(contents);
                }
                let recovered = crate::parity::recover(&protection.stripe,&shards)?;
                let pos = protection.members.iter()
                    .position(|m|m.id==item.id).ok_or("Missing stripe member")?;
                let original_ciphertext = &recovered[pos];
                self.decode_cipher(identity,item,original_ciphertext)?;
                let target = self.chunk_path(&item.id)?;
                let mut old = None;
                if target.exists() {
                    let mut nonce=[0u8;16];
                    rand::rngs::OsRng.fill_bytes(&mut nonce);
                    let quarantine = self.root.join("quarantine")
                        .join(format!("{}-{}.age",item.id,hex::encode(nonce)));
                    dir(quarantine.parent().ok_or("Unsafe quarantine")?)?;
                    fs::rename(&target,&quarantine)?;
                    old=Some(quarantine);
                }
                if let Err(error) = write_new(&target,original_ciphertext) {
                    if let Some(ref old_path)=old {let _=fs::rename(old_path,&target);}
                    return Err(error);
                }
                self.read_chunk(identity,item)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Read-only integrity and redundancy health results, suitable for audit and CI.
#[derive(Debug, serde::Serialize)]
pub struct HealthReport {
    pub backup_id: String,
    pub logical_files: usize,
    pub logical_bytes: u64,
    pub unique_chunks: usize,
    pub unique_plaintext_bytes: u64,
    pub protected_chunks: usize,
    pub healthy: bool,
    pub issues: Vec<String>,
}
impl Store {
    /// Verify every referenced encrypted chunk, age authentication, keyed ID,
    /// plaintext checksum and all associated parity shards. Never repair here.
    pub fn verify(&self, identity: &Path, id: &str) -> Res<HealthReport> {
        let manifest = self.load(identity, id)?;
        let mut issues = Vec::new();
        let mut hashes = HashSet::new();
        let mut unique_plaintext_bytes = 0u64;
        let mut logical_bytes = 0u64;
        for entry in &manifest.entries {
            logical_bytes = logical_bytes.checked_add(entry.bytes)
                .ok_or("Logical byte count overflow")?;
            for chunk in &entry.chunks {
                if !hashes.insert(chunk.id.clone()) { continue; }
                unique_plaintext_bytes = unique_plaintext_bytes
                    .checked_add(chunk.bytes as u64)
                    .ok_or("Unique byte count overflow")?;
                if let Err(e) = self.read_chunk(identity, chunk) {
                    issues.push(format!("Chunk {} verification failed: {}", chunk.id, e));
                }
            }
        }
        let protections = self.read_protection(identity)?;
        let mut covered = HashSet::new();
        for group in &protections {
            if !group.members.iter().any(|m| hashes.contains(&m.id)) { continue; }
            for member in &group.members {
                if !covered.insert(member.id.clone()) {
                    issues.push(format!("Chunk {} has duplicate parity protection",member.id));
                }
            }
            let folder = self.root.join("parity")
                .join(&group.id[..2]).join(&group.id);
            for i in 0..crate::parity::PARITY {
                let path = folder.join(format!("{i}.age"));
                let validation: Res<Vec<u8>> = (|| {
                    let meta = fs::symlink_metadata(&path)?;
                    if !meta.file_type().is_file() {return Err("Unsafe parity file".into());}
                    age(&fs::read(&path)?,None,Some(identity))
                })();
                let slot=crate::parity::DATA + i;
                match validation {
                    Ok(bytes) if bytes.len()==group.stripe.shard_size &&
                        sha(&bytes)==group.stripe.info[slot].sha256 => {}
                    _ => issues.push(format!("Parity {}:{} failed integrity",group.id,i)),
                }
            }
        }
        for id in &hashes {
            if !covered.contains(id) {
                issues.push(format!("Chunk {id} lacks redundancy"));
            }
        }
        Ok(HealthReport {
            backup_id: id.into(),
            logical_files: manifest.entries.len(),
            logical_bytes, unique_chunks: hashes.len(),
            unique_plaintext_bytes, protected_chunks: covered.len(),
            healthy: issues.is_empty(),
            issues
        })
    }
}

impl Store {
    pub(crate) fn recipient(&self)->&str{&self.cfg.recipient}
    pub(crate) fn portable_objects(&self,key:&Path,id:&str)
        ->Res<std::collections::BTreeMap<String,PathBuf>>{
        let backup=self.load(key,id)?;
        let mut referenced=HashSet::new();
        for entry in &backup.entries {
            for chunk in &entry.chunks {referenced.insert(chunk.id.clone());}
        }
        let mut objects=std::collections::BTreeMap::new();
        objects.insert("config.json.age".to_string(),self.root.join("config.json.age"));
        objects.insert(format!("manifests/{id}.json.age"),self.manifest_path(id)?);
        for group in self.read_protection(key)? {
            if !group.members.iter().any(|m|referenced.contains(&m.id)){continue;}
            for member in &group.members {referenced.insert(member.id.clone());}
            objects.insert(format!("stripes/{}.json.age",group.id),
                self.root.join("stripes").join(format!("{}.json.age",group.id)));
            for i in 0..crate::parity::PARITY {
                objects.insert(format!("parity/{}/{}/{i}.age",
                    &group.id[..2],group.id),
                    self.root.join("parity").join(&group.id[..2]).join(&group.id)
                       .join(format!("{i}.age")));
            }
        }
        for id in referenced {
            objects.insert(format!("chunks/{}/{}/{}.age",&id[..2],&id[2..4],id),
                self.chunk_path(&id)?);
        }
        Ok(objects)
    }
}

/// User-selected legacy encrypted archives that belong to ONE logical cycle.
pub struct LegacySource {pub namespace:String,pub archive:PathBuf}
impl Store {
    /// Non-destructive streaming migration from old age/tar.gz into zstd+age
    /// immutable chunks and one unified manifest; the old files stay intact.
    pub fn import_legacy(&self,identity:&Path,label:&str,
                         sources:&[LegacySource])->Res<LogicalBackup>{
        if sources.is_empty()||label.len()>256 {return Err("Invalid legacy migration".into());}
        let lock=OpenOptions::new().create(true).write(true)
            .open(self.root.join(".lock"))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        let mut entries=Vec::new();
        let mut seen=HashSet::new();
        for source in sources {
            if source.namespace.is_empty()||source.namespace.len()>80||
               !source.namespace.bytes().all(|b|
                 b.is_ascii_alphanumeric()||b==b'_'||b==b'-'){
                return Err("Unsafe legacy namespace".into());
            }
            crate::archive::visit_files(&source.archive,identity,|reader,member|{
                let path=format!("{}/{}",source.namespace,member.path.trim_end_matches('/'));
                if !safe_path(&path)||!seen.insert(path.clone()){
                    return Err("Unsafe or duplicate legacy path".into());
                }
                let mut remaining=member.bytes;
                let mut chunks=Vec::new();
                while remaining>0 {
                    let take=remaining.min(self.cfg.chunk_bytes as u64) as usize;
                    let mut bytes=vec![0u8;take];
                    reader.read_exact(&mut bytes)?;
                    chunks.push(self.save_chunk(identity,&bytes)?);
                    remaining-=take as u64;
                }
                entries.push(Entry {path,bytes:member.bytes,chunks});
                Ok(())
            })?;
        }
        entries.sort_by(|a,b|a.path.cmp(&b.path));
        let mut nonce=[0u8;32];rand::rngs::OsRng.fill_bytes(&mut nonce);
        let snapshot=LogicalBackup{version:1,id:hex::encode(nonce),
            label:label.to_string(),entries};
        let encrypted=age(&serde_json::to_vec(&snapshot)?,
            Some(&self.cfg.recipient),None)?;
        if !write_new(&self.manifest_path(&snapshot.id)?,&encrypted)? {
            return Err("Legacy migration manifest conflict".into());
        }
        self.protect(identity,&snapshot.id)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(snapshot)
    }
}
