//! Opt-in v1 content-addressed store: one logical backup manifest, encrypted chunks.
//! This does NOT modify existing tar.gz.age archives. Chunks use HMAC-SHA256 IDs.
use std::{collections::{HashMap, HashSet}, fs::{self, File, OpenOptions}, io::{Read, Write},
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum BackupKind { #[default] Full, Incremental }
impl BackupKind {
    pub fn parse(value:&str)->Result<Self,String>{
        match value {"full"=>Ok(Self::Full),"incremental"=>Ok(Self::Incremental),
            _=>Err("Backup mode must be full or incremental".into())}
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogicalBackup {
    pub version: u32, pub id: String, pub label: String, pub entries: Vec<Entry>,
    /// A live status flag loaded from a separate encrypted marker; never
    /// trust the stored manifest itself to authorize deletion.
    #[serde(default, skip_serializing)]
    pub protected: bool,
    /// Missing in pre-r037 manifests; unknown legacy age is never expired.
    #[serde(default)]
    pub created_unix: u64,
    /// Optional nested namespace at the logical backup level.
    #[serde(default)]
    pub top_namespace: String,
    /// Full snapshots and incrementals both have complete, self-contained
    /// chunk-reference inventories. Incrementals reuse immutable chunks.
    #[serde(default)]
    pub kind: BackupKind,
    #[serde(default)]
    pub parent_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Config { version: u32, recipient: String, chunk_bytes: usize, secret: String }
pub struct Source { pub namespace: String, pub directory: PathBuf }
pub struct Store { root: PathBuf, cfg: Config, key: [u8;32] }
#[derive(Serialize,Deserialize)]
struct BackupHold { schema: u8, id: String, protected: bool }
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
    fn hold_path(&self, id: &str) -> Res<PathBuf> {
        if !safe_hash(id) { return Err("Unsafe protected backup ID".into()); }
        Ok(self.root.join("protected").join(format!("{id}.json.age")))
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
    /// On-demand integrity checked block read for read-only mounts.
    pub fn read_chunk_ref(&self,identity:&Path,chunk:&ChunkRef)->Res<Vec<u8>>{
        self.read_chunk(identity,chunk)
    }
    /// Exact zstd size before age encryption; verifies plaintext integrity.
    pub(crate) fn compressed_chunk_bytes(&self,key:&Path,chunk:&ChunkRef)->Res<u64>{
        self.read_chunk(key,chunk)?;
        let path=self.chunk_path(&chunk.id)?;
        let metadata=fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file(){return Err("Unsafe encrypted chunk".into());}
        Ok(age(&fs::read(path)?,None,Some(key))?.len() as u64)
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
        self.backup_in_namespace(identity,label,"",sources)
    }
    pub fn backup_in_namespace(&self,identity:&Path,label:&str,
                               top_namespace:&str,sources:&[Source])
        ->Res<LogicalBackup> {
        self.backup_in_namespace_mode(identity,label,top_namespace,sources,
                                      BackupKind::Full,None)
    }
    pub fn backup_in_namespace_mode(&self,identity:&Path,label:&str,
                               top_namespace:&str,sources:&[Source],
                               kind:BackupKind,parent_id:Option<&str>)
        ->Res<LogicalBackup> {
        self.backup_in_namespace_mode_guard(identity,label,top_namespace,
            sources,kind,parent_id,&|_|Ok(()))
    }
    /// A distributed worker calls this guard before every file/chunk and
    /// immediately before manifest publication. A lost lease cannot START
    /// publishing another manifest; expired leases must also be quarantined
    /// instead of automatically reassigned. This is a cooperative safety
    /// boundary; complete write fencing still needs live failover testing.
    pub fn backup_in_namespace_mode_guard(&self,identity:&Path,label:&str,
                               top_namespace:&str,sources:&[Source],
                               kind:BackupKind,parent_id:Option<&str>,
                               live:&dyn Fn(bool)->Result<(),String>)
        ->Res<LogicalBackup> {
        live(false)?;
        if !top_namespace.is_empty() && !crate::backup_policy::safe_namespace(top_namespace) {
            return Err("Unsafe top namespace".into());
        }
        if label.len() > 256 || sources.is_empty() { return Err("Invalid label/sources".into()); }
        let lock = OpenOptions::new().create(true).write(true).open(self.root.join(".lock"))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        match (kind,parent_id){
            (BackupKind::Full,None)=>{},
            (BackupKind::Incremental,Some(parent))=>{
                if !safe_hash(parent){return Err("Invalid incremental parent ID".into());}
                let previous=self.load(identity,parent)?;
                if previous.top_namespace!=top_namespace{
                    return Err("Incremental cannot cross logical namespace".into());
                }
                // Detect invalid/missing ancestor or a forged cycle before
                // committing new lineage. Referenced chunks are independent.
                crate::backup_chain::ancestors(self,identity,parent)?;
            }
            _=>return Err("Full backup cannot have parent; incremental requires parent".into())
        }
        let mut entries = Vec::new();
        let mut visited = HashSet::new();
        for src in sources {
            live(false)?;
            if !crate::backup_policy::safe_namespace(&src.namespace) {
                return Err("Unsafe source namespace".into());
            }
            if !fs::symlink_metadata(&src.directory)?.file_type().is_dir() {
                return Err("Source must be a directory, not symlink".into());
            }
            for file in WalkDir::new(&src.directory).follow_links(false).max_depth(64) {
                live(false)?;
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
                let relative_path=format!("{}/{}",src.namespace,components.join("/"));
                let vpath=if top_namespace.is_empty(){relative_path}
                    else{format!("{top_namespace}/{relative_path}")};
                if !safe_path(&vpath) || !visited.insert(vpath.clone()) {
                    return Err("Unsafe/duplicate logical file".into());
                }
                let mut input = File::open(file.path())?;
                let mut chunks = Vec::new();
                let mut bytes = 0u64;
                loop {
                    live(false)?;
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
            label: label.to_owned(), entries, protected:false,
            created_unix:std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).map_or(0,|v|v.as_secs()),
            top_namespace:top_namespace.to_owned(),
            kind,parent_id:parent_id.map(str::to_owned)};
        // Fail closed if the lease was lost during a long-running scan.
        live(true)?;
        let encrypted = age(&serde_json::to_vec(&snapshot)?, Some(&self.cfg.recipient), None)?;
        live(true)?;
        if !write_new(&self.manifest_path(&snapshot.id)?, &encrypted)? {
            return Err("Manifest ID conflict".into());
        }
        // Keep the store lock until parity for the new manifest is committed:
        // garbage collection must never observe a valid but unprotected backup.
        self.protect(identity, &snapshot.id)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(snapshot)
    }
    /// Stream one provider-owned logical object directly into immutable,
    /// age-encrypted and parity-protected chunks. No raw plaintext dump is
    /// staged on disk. The producer's finalizer MUST prove successful
    /// completion before an authenticated manifest can be published.
    /// On interrupted reads or nonzero producer exit, orphan encrypted
    /// chunks may remain for later guarded GC, but no successful backup.
    pub fn backup_stream<R:Read,F:FnOnce()->Result<(),String>>(
        &self,identity:&Path,label:&str,top_namespace:&str,
        logical_path:&str,source:&mut R,kind:BackupKind,
        parent_id:Option<&str>,finish:F)->Res<LogicalBackup>{
        if label.trim().is_empty()||label.len()>256||
           !safe_path(logical_path)||
           (!top_namespace.is_empty()&&
             !crate::backup_policy::safe_namespace(top_namespace)){
            return Err("Invalid streamed backup label or namespace".into());
        }
        let full_path=if top_namespace.is_empty(){
            logical_path.to_owned()
        }else{format!("{top_namespace}/{logical_path}")};
        if !safe_path(&full_path){return Err("Invalid streamed virtual path".into());}
        let lock=OpenOptions::new().create(true).write(true)
            .open(self.root.join(".lock"))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        match (kind,parent_id){
            (BackupKind::Full,None)=>{},
            (BackupKind::Incremental,Some(parent))=>{
                if !safe_hash(parent){return Err("Invalid stream parent".into());}
                let previous=self.load(identity,parent)?;
                if previous.top_namespace!=top_namespace||
                    previous.entries.len()!=1||
                    previous.entries[0].path!=full_path {
                    return Err("Stream increment must match the same source object and namespace".into());
                }
                crate::backup_chain::ancestors(self,identity,parent)?;
            }
            _=>return Err("Incremental stream needs a parent; full stream must not have one".into())
        }
        let mut chunks=Vec::new();
        let mut total=0u64;
        loop{
            // take() retries interrupted/short pipe reads internally up to the
            // exact chunk boundary without needing seek or plaintext staging.
            let mut data=Vec::new();
            (&mut *source).take(self.cfg.chunk_bytes as u64)
                .read_to_end(&mut data)?;
            if data.is_empty(){break;}
            total=total.checked_add(data.len() as u64)
                .ok_or("Stream size overflow")?;
            chunks.push(self.save_chunk(identity,&data)?);
        }
        // A short or interrupted command must not create an apparently
        // complete backup. Verification happens BEFORE the manifest write.
        finish().map_err(|e|format!("Source stream did not finish successfully: {e}"))?;
        if total==0{return Err("Refusing an empty provider dump".into());}
        let mut nonce=[0u8;32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let snapshot=LogicalBackup{
            version:1,id:hex::encode(nonce),label:label.to_owned(),
            entries:vec![Entry{path:full_path,bytes:total,chunks}],
            protected:false,
            created_unix:std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0,|x|x.as_secs()),
            top_namespace:top_namespace.to_owned(),kind,
            parent_id:parent_id.map(str::to_owned)
        };
        let encrypted=age(&serde_json::to_vec(&snapshot)?,
            Some(&self.cfg.recipient),None)?;
        if !write_new(&self.manifest_path(&snapshot.id)?,&encrypted)?{
            return Err("Stream manifest collision".into());
        }
        self.protect(identity,&snapshot.id)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(snapshot)
    }

    pub fn load(&self, identity: &Path, id: &str) -> Res<LogicalBackup> {
        let path = self.manifest_path(id)?;
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err("Unsafe manifest".into());
        }
        let data = age(&fs::read(path)?, None, Some(identity))?;
        let mut value: LogicalBackup = serde_json::from_slice(&data)?;
        if value.version != 1 || value.id != id { return Err("Manifest identity mismatch".into()); }
        if !value.top_namespace.is_empty() &&
           !crate::backup_policy::safe_namespace(&value.top_namespace) {
            return Err("Invalid manifest top namespace".into());
        }
        match (value.kind,value.parent_id.as_deref()){
            (BackupKind::Full,None)=>{},
            (BackupKind::Incremental,Some(parent))
                if safe_hash(parent)&&parent!=id=>{},
            _=>return Err("Invalid encrypted backup chain metadata".into())
        }
        value.protected = self.is_backup_protected(identity,id)?;
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
        // In-flight restore is a live reader; hold the GC/purge lock for its
        // entire verified chunk stream and atomic new-directory publication.
        let _reader_guard=self.app_lock()?;
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
    fn distributed_guard_aborts_before_any_new_manifest_or_chunk(){
        let root=tempfile::tempdir().unwrap();
        let store=Store{
            root:root.path().join(".multi-backup-v1"),
            cfg:Config{version:1,recipient:"age1dummy".into(),
                chunk_bytes:DEFAULT_CHUNK,secret:String::new()},
            key:[7u8;32]
        };
        let source=Source{namespace:"files".into(),
            directory:root.path().join("nonexistent")};
        let mut checks=0;
        let failure=store.backup_in_namespace_mode_guard(
            Path::new("/missing-identity"),"test","",&[source],
            BackupKind::Full,None,&|finalize|{
                assert!(!finalize,"A dead lease must be denied before commit");
                Err("lost lease".into())
            });
        checks+=1;
        assert!(failure.is_err());
        assert_eq!(checks,1);
        assert!(!store.root.join("manifests").exists());
        assert!(!store.root.join("chunks").exists());
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
    /// Authenticated atomic repack commit: successor stripe takes precedence
    /// only after its 8+2 parity has been durably written and checked.
    #[serde(default)]
    supersedes: Option<String>,
}
impl Store {
    fn read_protection_all(&self, identity: &Path) -> Res<Vec<Protection>> {
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

    /// An authenticated new stripe index is the atomic commit point.
    /// Before that index exists, its old stripe stays authoritative.
    /// After a crash following commit, both indexes may exist until GC;
    /// readers deterministically choose the verified replacement.
    fn read_protection(&self, identity: &Path) -> Res<Vec<Protection>> {
        let all=self.read_protection_all(identity)?;
        let by_id:std::collections::HashMap<&str,&Protection>=all.iter()
            .map(|s|(s.id.as_str(),s)).collect();
        if by_id.len()!=all.len() {return Err("Duplicate parity stripe ID".into());}
        let mut superseded=HashSet::new();
        for stripe in &all {
            if let Some(old_id)=&stripe.supersedes {
                if !safe_hash(old_id)||old_id==&stripe.id {
                    return Err("Invalid parity repack predecessor".into());
                }
                if !superseded.insert(old_id.to_owned()) {
                    return Err("Concurrent repack replacements for one stripe".into());
                }
                // After GC the predecessor's obsolete index can be absent.
                // While present, require the replacement to retain exactly a
                // strict subset of authenticated original chunk references.
                if let Some(predecessor)=by_id.get(old_id.as_str()) {
                    if stripe.members.len()>=predecessor.members.len() ||
                       stripe.members.iter().any(|live|
                           !predecessor.members.iter().any(|m|m.id==live.id &&
                               m.sha256==live.sha256&&m.bytes==live.bytes)){
                        return Err("Repack must retain a strict subset of original members".into());
                    }
                }
                let base=self.root.join("parity")
                    .join(&stripe.id[..2]).join(&stripe.id);
                for index in 0..crate::parity::PARITY {
                    let path=base.join(format!("{index}.age"));
                    let meta=fs::symlink_metadata(&path)?;
                    if !meta.file_type().is_file(){return Err("Unsafe repacked parity shard".into());}
                    let bytes=age(&fs::read(path)?,None,Some(identity))?;
                    let slot=crate::parity::DATA+index;
                    if bytes.len()!=stripe.stripe.shard_size ||
                       sha(&bytes)!=stripe.stripe.info[slot].sha256{
                        return Err("Repacked parity shard failed integrity".into());
                    }
                }
            }
        }
        // Chains of successive repacks are allowed, but not cycles.
        for s in &all {
            let mut seen=HashSet::new();
            let mut cursor=s.supersedes.as_deref();
            while let Some(id)=cursor {
                if id==s.id || !seen.insert(id) {
                    return Err("Cyclic parity repack lineage".into());
                }
                cursor=by_id.get(id).and_then(|x|x.supersedes.as_deref());
            }
        }
        Ok(all.into_iter().filter(|s|!superseded.contains(&s.id)).collect())
    }
    /// Prepare a smaller 8+2 stripe containing only surviving chunks.
    /// Writes encrypted parity first, authenticates both shards, and commits
    /// a superseding encrypted index LAST. No old object is deleted here.
    /// A crash before the index leaves the original stripe authoritative;
    /// a crash after commit leaves both on disk until grace-qualified GC.
    fn repack_mixed_stripes_locked(&self,identity:&Path,
                                   live:&HashSet<String>)->Res<usize> {
        let stripes=self.read_protection(identity)?;
        let mut count=0usize;
        for old in &stripes {
            let remaining:Vec<ChunkRef>=old.members.iter().filter(|m|
                live.contains(&m.id)).cloned().collect();
            if remaining.is_empty()||remaining.len()==old.members.len(){
                continue;
            }
            let ciphertexts:Vec<Vec<u8>>=remaining.iter().map(|m|->Res<Vec<u8>>{
                self.read_chunk(identity,m)?;
                Ok(fs::read(self.chunk_path(&m.id)?)?)
            }).collect::<Res<_>>()?;
            let (meta,shards)=crate::parity::encode(&ciphertexts)?;
            let mut random=[0u8;32];
            rand::rngs::OsRng.fill_bytes(&mut random);
            let new_id=hex::encode(random);
            let folder=self.root.join("parity").join(&new_id[..2]).join(&new_id);
            for parity_index in 0..crate::parity::PARITY{
                let plaintext=&shards[crate::parity::DATA+parity_index];
                let encrypted=age(plaintext,Some(&self.cfg.recipient),None)?;
                let destination=folder.join(format!("{parity_index}.age"));
                if !write_new(&destination,&encrypted)?{
                    return Err("Repack parity collision".into());
                }
                let checked=age(&fs::read(&destination)?,None,Some(identity))?;
                if checked!=*plaintext{
                    return Err("Repacked parity verification failed".into());
                }
            }
            #[cfg(unix)] {
                File::open(&folder)?.sync_all()?;
                File::open(folder.parent().ok_or("Parity directory missing")?)?
                    .sync_all()?;
            }
            let successor=Protection{
                id:new_id.clone(),members:remaining,stripe:meta,
                supersedes:Some(old.id.clone())
            };
            let index=self.root.join("stripes").join(format!("{new_id}.json.age"));
            let authenticated=age(&serde_json::to_vec(&successor)?,
                                  Some(&self.cfg.recipient),None)?;
            if !write_new(&index,&authenticated)?{
                return Err("Repack index collision".into());
            }
            #[cfg(unix)] File::open(index.parent().ok_or("Index parent missing")?)?
                .sync_all()?;
            // All live data must still verify under the newly active index.
            // Keeping the old stripe until a later GC provides crash recovery.
            if !self.read_protection(identity)?.iter().any(|s|s.id==new_id){
                return Err("Parity repack commit was not discoverable".into());
            }
            count+=1;
        }
        Ok(count)
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
            let index = Protection {id:id.clone(),members:group.to_vec(),stripe:meta,
                supersedes:None};
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
                // The Reed-Solomon stripe authenticates zero-PADDED age
                // ciphertexts. A short valid member is not an erasure:
                // check its recorded original length, then pad to shard_size.
                // Previously all differently sized chunks were treated as
                // missing shards, defeating repair for normal POSIX trees.
                for (position,member) in protection.members.iter().enumerate() {
                    let ciphertext=fs::read(self.chunk_path(&member.id)?).ok()
                        .and_then(|mut bytes|{
                            if bytes.len()!=protection.stripe.info[position].length ||
                               bytes.len()>protection.stripe.shard_size {
                                return None;
                            }
                            bytes.resize(protection.stripe.shard_size,0);
                            Some(bytes)
                        });
                    shards.push(ciphertext);
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
        if backup.protected {
            objects.insert(format!("protected/{id}.json.age"),self.hold_path(id)?);
        }
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
            label:label.to_string(),entries,protected:false,
            created_unix:0,top_namespace:String::new(),
            kind:BackupKind::Full,parent_id:None};
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

/// Atomic and non-destructive storage location conversion for v1.
/// The source remains unchanged; only independently authenticated encrypted
/// objects are copied. Each referenced logical backup must be healthy.
#[derive(Debug,Serialize)]
pub struct RelocationReport {
    pub logical_backups: usize,
    pub encrypted_objects: usize,
    pub ciphertext_bytes: u64,
    pub verified: bool,
    pub dry_run: bool,
}
impl Store {
    pub fn relocate(&self,identity:&Path,destination:&Path,dry_run:bool)
        ->Res<RelocationReport> {
        if destination.exists(){return Err("Refusing overwrite of destination".into());}
        let parent=destination.parent().ok_or("Missing destination parent")?;
        if !fs::symlink_metadata(parent)?.file_type().is_dir(){
            return Err("Destination parent must be an existing real directory".into());
        }
        let backups=self.list(identity)?;
        let mut objects=std::collections::BTreeMap::new();
        objects.insert("config.json.age".to_string(),
            self.root.join("config.json.age"));
        for backup in &backups {
            let checked=self.verify(identity,&backup.id)?;
            if !checked.healthy {
                return Err(format!("Backup {} is not healthy; relocation blocked",
                    backup.id).into());
            }
            for (rel,path) in self.portable_objects(identity,&backup.id)? {
                if !crate::portable::safe_object(&rel) {
                    return Err("Malformed object address".into());
                }
                objects.insert(rel,path);
            }
        }
        let mut total=0u64;
        let mut stamps=Vec::with_capacity(objects.len());
        for (rel,path) in &objects {
            if !crate::portable::safe_object(rel) ||
               !fs::symlink_metadata(path)?.file_type().is_file() {
                return Err("Unsafe storage object".into());
            }
            let (size,hash)=crate::portable::hash_file(path)?;
            total=total.checked_add(size).ok_or("Storage size overflow")?;
            stamps.push((rel.clone(),size,hash));
        }
        let mut report=RelocationReport{logical_backups:backups.len(),
            encrypted_objects:objects.len(),ciphertext_bytes:total,
            verified:false,dry_run};
        if dry_run {return Ok(report);}
        let stage=tempfile::Builder::new().prefix(".multi-backup-copy-")
            .tempdir_in(parent)?;
        let base=stage.path().join(SUBDIR);
        dir(&base)?;
        for (rel,size,expected) in stamps {
            let source=objects.get(&rel).ok_or("Lost source object")?;
            let target=base.join(&rel);
            dir(target.parent().ok_or("Invalid object parent")?)?;
            let actual=fs::copy(source,&target)?;
            if actual!=size {return Err("Storage changed during relocation".into());}
            let (copied,digest)=crate::portable::hash_file(&target)?;
            if copied!=size||digest!=expected {
                return Err("Object SHA-256 mismatch during relocation".into());
            }
        }
        let verified=Store::open(stage.path(),identity)?;
        let copied=verified.list(identity)?;
        if copied.len()!=backups.len() {
            return Err("Relocation changed logical backup count".into());
        }
        for backup in &backups {
            let check=verified.verify(identity,&backup.id)?;
            if !check.healthy {return Err("Relocation parity or chunk verification failed".into());}
        }
        fs::rename(stage.path(),destination)?;
        report.verified=true;
        Ok(report)
    }
}

/// On-demand verified reader, never materializing a plaintext archive file.
struct LogicalReader<'a>{
    store:&'a Store,key:&'a Path,chunks:&'a[ChunkRef],
    index:usize,current:Vec<u8>,offset:usize
}
impl std::io::Read for LogicalReader<'_>{
    fn read(&mut self,dest:&mut[u8])->std::io::Result<usize>{
        if dest.is_empty(){return Ok(0);}
        let mut n=0usize;
        while n<dest.len(){
            if self.offset>=self.current.len(){
                if self.index>=self.chunks.len(){break;}
                self.current=self.store.read_chunk_ref(self.key,&self.chunks[self.index])
                    .map_err(|e|std::io::Error::new(std::io::ErrorKind::InvalidData,e.to_string()))?;
                self.offset=0;self.index+=1;
                if self.current.is_empty(){continue;}
            }
            let take=(self.current.len()-self.offset).min(dest.len()-n);
            dest[n..n+take].copy_from_slice(&self.current[self.offset..self.offset+take]);
            self.offset+=take;n+=take;
        }
        Ok(n)
    }
}
impl Store {
    /// Reverse conversion v1 -> tar.gz.age for legacy clients. Stream
    /// chunks directly through tar and gzip into age without plaintext staging.
    pub fn export_legacy(&self,key:&Path,id:&str,out:&Path)->Res<usize>{
        if !out.to_string_lossy().ends_with(".tar.gz.age"){
            return Err("Requires legacy .tar.gz.age destination".into());
        }
        if out.exists(){return Err("Refusing overwrite of existing archive".into());}
        let parent=out.parent().ok_or("Missing output parent")?;
        if !fs::symlink_metadata(parent)?.file_type().is_dir(){
            return Err("Output parent is unsafe".into());
        }
        if !self.verify(key,id)?.healthy{return Err("Logical backup is not healthy".into());}
        let snapshot=self.load(key,id)?;
        let tempfile=tempfile::Builder::new()
            .prefix(".mb-legacy-").suffix(".tar.gz.age").tempfile_in(parent)?;
        let ciphertext_out=tempfile.as_file().try_clone()?;
        let mut child=Command::new("age").arg("-r").arg(&self.cfg.recipient)
            .stdin(Stdio::piped()).stdout(Stdio::from(ciphertext_out))
            .stderr(Stdio::null()).spawn()?;
        let make=(||->Res<()>{
            let stdin=child.stdin.take().ok_or("Missing age stdin")?;
            let zip=flate2::write::GzEncoder::new(stdin,flate2::Compression::default());
            let mut tar=tar::Builder::new(zip);
            for file in &snapshot.entries{
                let entry_path=format!("multi-backup/{}",file.path);
                if !safe_path(&entry_path){return Err("Unsafe virtual archive name".into());}
                let mut header=tar::Header::new_gnu();
                header.set_size(file.bytes);
                header.set_mode(0o600);
                header.set_mtime(0);
                header.set_cksum();
                let source=LogicalReader{store:self,key,chunks:&file.chunks,
                    index:0,current:Vec::new(),offset:0};
                tar.append_data(&mut header,entry_path,source)?;
            }
            let encoder=tar.into_inner()?;
            drop(encoder.finish()?);
            Ok(())
        })();
        if let Err(error)=make {
            let _=child.kill();let _=child.wait();return Err(error);
        }
        if !child.wait()?.success(){return Err("Age stream encryption failed".into());}
        tempfile.as_file().sync_all()?;
        let files=crate::archive::verify(tempfile.path(),key)?;
        if files!=snapshot.entries.len(){return Err("Exported file count mismatch".into());}
        tempfile.persist_noclobber(out).map_err(|e|e.error)?;
        Ok(files)
    }
}


/// User-requested logical backup protection. Unlike Store::protect (8+2
/// parity), this holds the MANIFEST and all of its referenced chunks against
/// application-level manual deletion, retention and garbage collection.
///
/// This is NOT an immutable WORM/Object Lock boundary: a privileged OS/storage
/// operator can still modify underlying files. Real ransomware resistance
/// requires an independently permissioned append-only vault.
impl Store {
    fn app_lock(&self) -> Res<File> {
        let lock=self.root.join(".lock");
        match fs::symlink_metadata(&lock) {
            Ok(meta) if !meta.file_type().is_file() => return Err("Unsafe store lock".into()),
            Err(error) if error.kind()!=std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => (),
        }
        let handle=OpenOptions::new().create(true).write(true).open(&lock)?;
        if !handle.metadata()?.is_file(){return Err("Unsafe store lock".into());}
        fs2::FileExt::lock_exclusive(&handle)?;
        Ok(handle)
    }
    pub fn is_backup_protected(&self,identity:&Path,id:&str)->Res<bool>{
        let path=self.hold_path(id)?;
        match fs::symlink_metadata(&path){
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(false),
            Err(error)=>return Err(error.into()),
            Ok(meta) if !meta.file_type().is_file()||meta.len()>65536=>
                return Err("Unsafe protected backup marker".into()),
            Ok(_)=>{},
        }
        let raw=age(&fs::read(&path)?,None,Some(identity))?;
        let hold:BackupHold=serde_json::from_slice(&raw)?;
        if hold.schema!=1||hold.id!=id||!hold.protected{
            return Err("Invalid protected backup marker".into());
        }
        Ok(true)
    }
    pub fn protect_backup(&self,identity:&Path,id:&str)->Res<bool>{
        let lock=self.app_lock()?;
        self.load(identity,id)?; // require an existing, authenticated manifest
        if self.is_backup_protected(identity,id)?{return Ok(false);}
        let hold=BackupHold{schema:1,id:id.into(),protected:true};
        let encrypted=age(&serde_json::to_vec(&hold)?,Some(&self.cfg.recipient),None)?;
        if !write_new(&self.hold_path(id)?,&encrypted)?{
            return Err("Concurrent protected marker conflict".into());
        }
        fs2::FileExt::unlock(&lock)?;
        Ok(true)
    }
    /// Removing user protection ALWAYS requires explicit exact-ID confirmation.
    /// The caller must authorize unlocking; it cannot be implicitly invoked
    /// by expiry, schedule, cleanup or a restore operation.
    pub fn unprotect_backup(&self,identity:&Path,id:&str,confirm_id:&str)->Res<bool>{
        if id!=confirm_id||!safe_hash(id) {
            return Err("Confirm the complete backup ID to remove protection".into());
        }
        let lock=self.app_lock()?;
        self.load(identity,id)?;
        if !self.is_backup_protected(identity,id)?{return Ok(false);}
        fs::remove_file(self.hold_path(id)?)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(true)
    }
    /// Logical manifest deletion ONLY. Never remove shared chunks or stripes
    /// here; future GC must recheck all live manifests and protected holds
    /// with the SAME store lock, parity dependencies and a grace period.
    pub fn delete_backup(&self,identity:&Path,id:&str,confirm_id:&str)->Res<()>{
        if id!=confirm_id||!safe_hash(id) {
            return Err("Confirm the complete backup ID before deletion".into());
        }
        let lock=self.app_lock()?;
        let backup=self.load(identity,id)?;
        if backup.protected {
            return Err("Backup is PROTECTED. Explicitly remove protection first".into());
        }
        if self.list(identity)?.iter().any(|backup|
            backup.parent_id.as_deref()==Some(id)){
            return Err("Backup has dependent increments; use delete-rebase to preserve the chain".into());
        }
        let manifest=self.manifest_path(id)?;
        if !fs::symlink_metadata(&manifest)?.file_type().is_file(){
            return Err("Unsafe manifest deletion".into());
        }
        fs::remove_file(manifest)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(())
    }

    /// Rebase direct descendants before removing a full or incremental.
    /// Every snapshot already has a complete independent chunk-reference
    /// inventory, so no plaintext payload or shared chunk is ever modified.
    /// On failure before parent deletion, the parent remains available.
    /// Protected children cannot have their chain identity rewritten.
    pub fn delete_backup_rebase(&self,identity:&Path,id:&str,confirm_id:&str)
        ->Res<usize>{
        if id!=confirm_id||!safe_hash(id){
            return Err("Confirm the complete backup ID before chain rebase".into());
        }
        let lock=self.app_lock()?;
        let target=self.load(identity,id)?;
        if target.protected{
            return Err("Protected backup: remove protection explicitly first".into());
        }
        let children:Vec<_>=self.list(identity)?.into_iter()
            .filter(|backup|backup.parent_id.as_deref()==Some(id)).collect();
        for child in &children {
            if child.protected{
                return Err("Protected dependent prevents chain rebase".into());
            }
            let verification=crate::backup_chain::verify_chain(self,identity,&child.id,true)?;
            if !verification.healthy{
                return Err("Dependent increment failed integrity verification".into());
            }
        }
        let folder=self.root.join("manifests");
        for child in &children {
            let mut updated=child.clone();
            if target.kind==BackupKind::Full {
                updated.kind=BackupKind::Full;
                updated.parent_id=None;
            } else {
                updated.parent_id=target.parent_id.clone();
                if updated.parent_id.is_none(){
                    updated.kind=BackupKind::Full;
                }
            }
            let ciphertext=age(&serde_json::to_vec(&updated)?,
                Some(&self.cfg.recipient),None)?;
            let mut temp=NamedTempFile::new_in(&folder)?;
            temp.write_all(&ciphertext)?;
            temp.as_file().sync_all()?;
            let destination=self.manifest_path(&updated.id)?;
            if !fs::symlink_metadata(&destination)?.file_type().is_file(){
                return Err("Unsafe descendant manifest during rebase".into());
            }
            temp.persist(&destination).map_err(|error|error.error)?;
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&destination,fs::Permissions::from_mode(0o600))?;
            }
        }
        // Authenticate every rewritten successor under its NEW lineage
        // before the original ancestor can be removed.
        for child in &children {
            let checked=crate::backup_chain::verify_chain(
                self,identity,&child.id,true)?;
            if !checked.healthy {
                return Err("Rebased successor did not pass independent chain integrity verification".into());
            }
        }
        let manifest=self.manifest_path(id)?;
        if !fs::symlink_metadata(&manifest)?.file_type().is_file(){
            return Err("Unsafe manifest deletion".into());
        }
        fs::remove_file(manifest)?;
        fs2::FileExt::unlock(&lock)?;
        Ok(children.len())
    }
}


/// Read-only, conservative retention preview: decrypt and verify *every*
/// logical snapshot. Old snapshots lack timestamps and remain preserved.
/// Explicit user protection is always taken from the encrypted live marker,
/// not from a caller-provided JSON flag. No deletion or GC occurs here.
impl Store {
    pub fn preview_retention(&self,identity:&Path,
                             policy:&crate::backup_policy::Retention,now:u64)
        ->Res<crate::backup_policy::ExpiryPreview>{
        let backups=self.list(identity)?;
        let mut records=Vec::with_capacity(backups.len());
        for b in backups {
            let namespace=if b.top_namespace.is_empty(){"default".to_string()}
                          else{b.top_namespace};
            let verified=self.verify(identity,&b.id)?.healthy;
            records.push(crate::backup_policy::BackupStamp{
                id:b.id,namespace,created_unix:b.created_unix,
                verified,protected:b.protected,pinned:false,
                legal_hold:false,immutable_until:None,
            });
        }
        Ok(crate::backup_policy::expiry_preview(&records,policy,now)?)
    }
}


/// Conservative reference-counting GC: purge/rebase changes only manifests.
/// GC is the sole place where orphan ciphertext may be physically deleted.
/// Mixed live/dead 8+2 parity stripes are retained until a separate
/// crash-safe repack can replace them; no protection is silently weakened.
#[derive(Debug,serde::Serialize)]
pub struct GarbageCollectionReport {
    pub dry_run:bool,
    pub live_backups:usize,
    pub live_chunks:usize,
    pub orphan_chunk_candidates:usize,
    pub obsolete_stripe_candidates:usize,
    pub blocked_chunks_by_shared_parity:usize,
    pub mixed_stripes_to_repack:usize,
    pub repacked_stripes:usize,
    pub orphan_parity_directories:usize,
    pub deleted_orphan_parity_directories:usize,
    /// Old repack protection is retained until its new replacement ages past
    /// the grace boundary. Physical freeing may need a future GC cycle.
    pub reclaimable_bytes:u64,
    pub deleted_chunks:usize,
    pub deleted_stripes:usize,
    pub reclaimed_bytes:u64,
    pub grace_hours:u64,
}
impl Store {
    pub fn garbage_collect(&self,identity:&Path,grace_hours:u64,execute:bool)
        ->Res<GarbageCollectionReport>{
        if !(24..=24*365).contains(&grace_hours){
            return Err("GC grace must be 24 hours to 365 days".into());
        }
        // Every writer, including storage backups and manifest rebase, must
        // hold the same .lock. Re-evaluate all references under this lock.
        let _guard=self.app_lock()?;
        let backups=self.list(identity)?;
        let mut live=HashSet::new();
        for backup in &backups {
            // Fail closed for malformed chains, lost refs, corrupt data or
            // parity. No partial cleanup when live backup integrity is suspect.
            crate::backup_chain::ancestors(self,identity,&backup.id)?;
            let result=self.verify(identity,&backup.id)?;
            if !result.healthy {
                return Err(format!("GC blocked by unhealthy live backup {}",backup.id).into());
            }
            for entry in &backup.entries {
                for chunk in &entry.chunks {live.insert(chunk.id.clone());}
            }
        }
        // The preview is non-mutating. Execution first constructs and checks
        // durable replacement parity, never deleting an existing shard.
        let original=self.read_protection(identity)?;
        let pending_repack=original.iter().filter(|stripe|{
            let used=stripe.members.iter().filter(|m|live.contains(&m.id)).count();
            used>0 && used<stripe.members.len()
        }).count();
        let repacked=if execute {
            self.repack_mixed_stripes_locked(identity,&live)?
        }else{0};
        if execute {
            for backup in &backups {
                if !self.verify(identity,&backup.id)?.healthy {
                    return Err("Post-repack live backup failed redundancy verification".into());
                }
            }
        }
        let now=std::time::SystemTime::now();
        let min_age=std::time::Duration::from_secs(grace_hours*3600);
        let old_enough=|path:&Path|->Res<bool>{
            let md=fs::symlink_metadata(path)?;
            if !md.file_type().is_file(){return Err("GC refuses non-regular file".into());}
            Ok(now.duration_since(md.modified()?).is_ok_and(|age|age>=min_age))
        };
        let stripes=self.read_protection_all(identity)?;
        let active_ids:HashSet<String>=self.read_protection(identity)?
            .iter().map(|stripe|stripe.id.clone()).collect();
        let by_id:HashMap<&str,&Protection>=stripes.iter()
            .map(|stripe|(stripe.id.as_str(),stripe)).collect();
        let replacements:HashMap<&str,&str>=stripes.iter().filter_map(|stripe|
            stripe.supersedes.as_deref().map(|old|(old,stripe.id.as_str()))).collect();
        let mut retained_stripe_members=HashSet::new();
        let mut obsolete_stripes=Vec::new();
        let mut recoverable_bytes=0u64;
        for stripe in &stripes {
            let record=self.root.join("stripes").join(format!("{}.json.age",stripe.id));
            let referenced=stripe.members.iter().any(|item|live.contains(&item.id));
            let active=active_ids.contains(&stripe.id);
            let eligible_superseded=if !active {
                let replacement=replacements.get(stripe.id.as_str())
                    .ok_or("Superseded parity stripe has no successor")?;
                let successor=self.root.join("stripes")
                    .join(format!("{replacement}.json.age"));
                // Keep old data for at least one full grace period AFTER
                // durable replacement commit, not only since old object birth.
                let mature=old_enough(&successor)?;
                // Process oldest generations before deleting a replacement
                // which would otherwise strand its own old predecessor.
                let previous_gone=stripe.supersedes.as_deref()
                    .is_none_or(|old|!by_id.contains_key(old));
                mature&&previous_gone
            }else{false};
            if (active && referenced)||(!active&&!eligible_superseded)||
               !old_enough(&record)? {
                for item in &stripe.members {
                    retained_stripe_members.insert(item.id.clone());
                }
                continue;
            }
            let folder=self.root.join("parity").join(&stripe.id[..2]).join(&stripe.id);
            match fs::symlink_metadata(&folder){
                Ok(meta) if !meta.file_type().is_dir()=>
                    return Err("Unsafe obsolete parity folder".into()),
                Err(error) if error.kind()!=std::io::ErrorKind::NotFound=>
                    return Err(error.into()),
                _=>()
            }
            let mut files=vec![record];
            // A crash may have removed one obsolete shard after commit.
            // Missing retired shards are safe only for a no-longer-active
            // fully orphan stripe or a mature superseded old stripe.
            for i in 0..crate::parity::PARITY {
                let shard=folder.join(format!("{i}.age"));
                match fs::symlink_metadata(&shard) {
                    Err(error) if error.kind()==std::io::ErrorKind::NotFound=>continue,
                    Err(error)=>return Err(error.into()),
                    Ok(meta) if !meta.file_type().is_file()=>
                        return Err("Unsafe obsolete parity shard".into()),
                    Ok(_)=>{
                        if !old_enough(&shard)?{files.clear();break;}
                        files.push(shard);
                    }
                }
            }
            if files.is_empty() {
                for item in &stripe.members {
                    retained_stripe_members.insert(item.id.clone());
                }
                continue;
            }
            let mut bytes=0u64;
            for file in &files {
                bytes=bytes.checked_add(fs::symlink_metadata(file)?.len())
                    .ok_or("Parity GC size overflow")?;
            }
            recoverable_bytes=recoverable_bytes.checked_add(bytes)
                .ok_or("GC size overflow")?;
            obsolete_stripes.push((stripe.id.clone(),files,folder));
        }
        // A power loss before the replacement INDEX commit can leave only
        // prepared encrypted parity shards behind. Those folders do not form
        // a committed stripe; reclaim them only after their OWN grace period.
        let parity_root=self.root.join("parity");
        let mut orphan_parity=Vec::new();
        if parity_root.exists(){
            if !fs::symlink_metadata(&parity_root)?.file_type().is_dir(){
                return Err("Unsafe parity tree".into());
            }
            let known:HashSet<&str>=stripes.iter().map(|s|s.id.as_str()).collect();
            for item in WalkDir::new(&parity_root).follow_links(false)
                .min_depth(2).max_depth(2) {
                let item=item?;
                if !item.file_type().is_dir() {
                    return Err("Unexpected parity tree entry".into());
                }
                let folder=item.path().to_path_buf();
                let id=folder.file_name().and_then(|s|s.to_str())
                    .ok_or("Non-UTF8 parity directory")?;
                if !safe_hash(id) || folder!=parity_root.join(&id[..2]).join(id) {
                    return Err("Noncanonical parity directory".into());
                }
                if known.contains(id) {continue;}
                let meta=fs::symlink_metadata(&folder)?;
                if now.duration_since(meta.modified()?).map_or(true,|age|age<min_age){
                    continue;
                }
                let mut candidates=Vec::new();
                let mut bytes=0u64;
                let mut safe=true;
                for file in fs::read_dir(&folder)? {
                    let path=file?.path();
                    let name=path.file_name().and_then(|s|s.to_str())
                        .ok_or("Non-UTF8 orphan parity filename")?;
                    if name!="0.age"&&name!="1.age" {
                        return Err("Unexpected orphan parity content".into());
                    }
                    if !old_enough(&path)?{safe=false;break;}
                    bytes=bytes.checked_add(fs::symlink_metadata(&path)?.len())
                        .ok_or("Orphan parity size overflow")?;
                    candidates.push(path);
                }
                if safe {
                    recoverable_bytes=recoverable_bytes.checked_add(bytes)
                        .ok_or("GC size overflow")?;
                    orphan_parity.push((folder,candidates,bytes));
                }
            }
        }
        let root=self.root.join("chunks");
        let mut candidates=Vec::new();
        let mut blocked=0usize;
        if root.exists(){
            if !fs::symlink_metadata(&root)?.file_type().is_dir(){
                return Err("Invalid chunk store directory".into());
            }
            for item in WalkDir::new(&root).follow_links(false).min_depth(1).max_depth(3){
                let item=item?;
                if item.file_type().is_symlink(){
                    return Err("GC encountered a symlink in chunk storage".into());
                }
                if item.file_type().is_dir(){continue;}
                if !item.file_type().is_file(){
                    return Err("GC encountered a special chunk storage entry".into());
                }
                let path=item.path().to_path_buf();
                let file=path.file_name().and_then(|s|s.to_str())
                    .ok_or("Non-UTF8 chunk name")?;
                let id=file.strip_suffix(".age").ok_or("Unexpected chunk file")?;
                if !safe_hash(id){
                    return Err("Unsafe chunk object identifier".into());
                }
                if self.chunk_path(id)? != path{
                    return Err("Chunk does not match canonical object path".into());
                }
                if live.contains(id){continue;}
                if retained_stripe_members.contains(id){
                    blocked+=1;
                    continue;
                }
                if !old_enough(&path)? {continue;}
                let bytes=fs::symlink_metadata(&path)?.len();
                recoverable_bytes=recoverable_bytes.checked_add(bytes)
                    .ok_or("GC size overflow")?;
                candidates.push((path,bytes));
            }
        }
        let mut report=GarbageCollectionReport{
            dry_run:!execute,live_backups:backups.len(),live_chunks:live.len(),
            orphan_chunk_candidates:candidates.len(),
            obsolete_stripe_candidates:obsolete_stripes.len(),
            blocked_chunks_by_shared_parity:blocked,
            mixed_stripes_to_repack:pending_repack,
            repacked_stripes:repacked,
            orphan_parity_directories:orphan_parity.len(),
            deleted_orphan_parity_directories:0,
            reclaimable_bytes:recoverable_bytes,
            deleted_chunks:0,deleted_stripes:0,reclaimed_bytes:0,
            grace_hours,
        };
        if !execute{return Ok(report);}
        // Delete orphan stripe INDEX first: no live chunk depends on it.
        // Parity pieces are only removed for fully orphan, grace-qualified
        // groups; mixed groups are preserved intact for future compaction.
        for (_id,files,folder) in obsolete_stripes {
            let size=files.iter().try_fold(0u64,|sum,path|->Res<u64>{
                Ok(sum.checked_add(fs::symlink_metadata(path)?.len())
                    .ok_or("GC stripe size overflow")?)
            })?;
            for path in files {fs::remove_file(path)?;}
            if folder.exists(){fs::remove_dir(folder)?;}
            report.deleted_stripes+=1;
            report.reclaimed_bytes=report.reclaimed_bytes.checked_add(size)
                .ok_or("GC reclaimed size overflow")?;
        }
        for (folder,files,bytes) in orphan_parity {
            for file in files {fs::remove_file(file)?;}
            fs::remove_dir(folder)?;
            report.deleted_orphan_parity_directories+=1;
            report.reclaimed_bytes=report.reclaimed_bytes.checked_add(bytes)
                .ok_or("GC reclaimed size overflow")?;
        }
        for (path,bytes) in candidates {
            fs::remove_file(path)?;
            report.deleted_chunks+=1;
            report.reclaimed_bytes=report.reclaimed_bytes.checked_add(bytes)
                .ok_or("GC reclaimed size overflow")?;
        }
        Ok(report)
    }
}
