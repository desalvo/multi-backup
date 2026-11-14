//! Rust-native, streaming and fail-closed reader/writer for the original
//! Python \`tarfile.open(w|gz)\` + age-recipient archive wire format.
//! No decrypted complete archive is ever staged on disk.
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tar::{Archive, Builder, EntryType, Header};
use tempfile::{Builder as TempBuilder, NamedTempFile};
use walkdir::WalkDir;

pub type ArchiveResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_MEMBERS: usize = 500_000;
const MAX_UNCOMPRESSED: u64 = 100 * 1024 * 1024 * 1024;
const MAX_INPUT_FILES: usize = 500_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Member {
    pub path: String,
    pub bytes: u64,
    pub directory: bool,
}

fn err(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    message.into().into()
}

/// Disallow platform-dependent separators, drive letters, absolute paths,
/// traversal, hidden path semantics, control characters and duplicate entries.
pub fn safe_path(name: &str) -> ArchiveResult<PathBuf> {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\')
        || name.contains('\\') || name.contains('\0')
        || name.chars().any(|c| c.is_control())
    {
        return Err(err("Unsafe archive member path"));
    }
    let normalized = name.trim_end_matches('/');
    if normalized.is_empty() || normalized.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
        return Err(err("Unsafe archive member component"));
    }
    if normalized.split('/').next().unwrap_or("").contains(':') {
        return Err(err("Unsafe drive-qualified archive path"));
    }
    let path = PathBuf::from(normalized);
    if path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(err("Unsafe native archive path component"));
    }
    Ok(path)
}

fn ensure_archive(path: &Path) -> ArchiveResult<()> {
    if !path.to_string_lossy().ends_with(".tar.gz.age") {
        return Err(err("Expected a .tar.gz.age encrypted archive"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(err("Archive is not an ordinary file"));
    }
    Ok(())
}

fn verify_sidecar(path: &Path) -> ArchiveResult<()> {
    let sidecar = PathBuf::from(format!("{}.sha256", path.display()));
    if !sidecar.exists() {
        return Ok(());
    }
    if !fs::symlink_metadata(&sidecar)?.file_type().is_file() {
        return Err(err("Unsafe checksum sidecar"));
    }
    let value = fs::read_to_string(&sidecar)?;
    let digest = value.split_whitespace().next().unwrap_or("");
    if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(err("Invalid SHA-256 sidecar"));
    }
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    if format!("{:x}", hasher.finalize()) != digest.to_ascii_lowercase() {
        return Err(err("Encrypted archive SHA-256 mismatch"));
    }
    Ok(())
}

fn start_decrypt(archive: &Path, identity: &Path) -> ArchiveResult<Child> {
    ensure_archive(archive)?;
    verify_sidecar(archive)?;
    if !fs::symlink_metadata(identity)?.file_type().is_file() {
        return Err(err("Age identity must be a regular, non-symlinked file"));
    }
    let child = Command::new("age").arg("--decrypt").arg("-i")
        .arg(identity).arg(archive)
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    Ok(child)
}

fn with_reader<T>(
    path: &Path,
    identity: &Path,
    visitor: impl FnOnce(&mut Archive<GzDecoder<std::process::ChildStdout>>) -> ArchiveResult<T>,
) -> ArchiveResult<T> {
    let mut child = start_decrypt(path, identity)?;
    let result = (|| -> ArchiveResult<T> {
        let stdout = child.stdout.take().ok_or_else(|| err("Missing age output stream"))?;
        let mut archive = Archive::new(GzDecoder::new(stdout));
        let output = visitor(&mut archive)?;
        // tar can stop on the end-of-archive marker before age has authenticated
        // its final chunk. Drain *all* bytes to make age verify its whole stream.
        let mut remaining = archive.into_inner().into_inner();
        io::copy(&mut remaining, &mut io::sink())?;
        drop(remaining);
        let status = child.wait_with_output()?;
        if !status.status.success() {
            return Err(err(format!("age decryption failed: {}",
                String::from_utf8_lossy(&status.stderr)
                    .chars().take(360).collect::<String>())));
        }
        Ok(output)
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn scan<R: Read>(
    archive: &mut Archive<R>,
    mut on_member: impl FnMut(&mut tar::Entry<'_, R>, &Member, &Path) -> ArchiveResult<()>,
) -> ArchiveResult<Vec<Member>> {
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut all = Vec::new();
    for item in archive.entries()? {
        let mut entry = item?;
        if all.len() >= MAX_MEMBERS { return Err(err("Archive member limit exceeded")); }
        let name = std::str::from_utf8(&entry.path_bytes())
            .map_err(|_| err("Archive member path is not valid UTF-8"))?
            .to_string();
        let path = safe_path(&name)?;
        let key = path.to_string_lossy().to_string();
        if !names.insert(key) { return Err(err("Duplicate archive member")); }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(err("Unsupported link, device or special archive member"));
        }
        let size = entry.size();
        total = total.checked_add(size).ok_or_else(|| err("Archive byte count overflow"))?;
        if total > MAX_UNCOMPRESSED { return Err(err("Archive extraction byte limit exceeded")); }
        let member = Member { path: name, bytes: size, directory: kind.is_dir() };
        on_member(&mut entry, &member, &path)?;
        all.push(member);
    }
    Ok(all)
}

pub fn list(path: &Path, identity: &Path) -> ArchiveResult<Vec<Member>> {
    with_reader(path, identity, |archive| scan(archive, |entry, member, _| {
        if !member.directory {
            let actual = io::copy(entry, &mut io::sink())?;
            if actual != member.bytes { return Err(err("Short archive member")); }
        }
        Ok(())
    }))
}

pub fn verify(path: &Path, identity: &Path) -> ArchiveResult<usize> {
    Ok(list(path, identity)?.len())
}

fn assert_safe_parent(root: &Path, path: &Path) -> ArchiveResult<()> {
    let relative = path.strip_prefix(root)?;
    let mut parent = root.to_path_buf();
    for component in relative.components() {
        parent.push(component);
        if let Ok(meta) = fs::symlink_metadata(&parent) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(err("Archive would cross a symlink or non-directory"));
            }
        } else {
            fs::create_dir(&parent)?;
        }
    }
    Ok(())
}

pub fn extract(archive: &Path, identity: &Path, destination: &Path) -> ArchiveResult<Vec<Member>> {
    // Never touch an existing restore destination, even an empty directory.
    if fs::symlink_metadata(destination).is_ok() {
        return Err(err("Restore destination already exists"));
    }
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let canonical_parent = parent.canonicalize()?;
    let name = destination.file_name().ok_or_else(|| err("Invalid restore destination"))?;
    if name.to_string_lossy().contains(':') || name == "." || name == ".." {
        return Err(err("Invalid restore destination name"));
    }
    let final_path = canonical_parent.join(name);
    if fs::symlink_metadata(&final_path).is_ok() {
        return Err(err("Restore destination already exists"));
    }
    let staging = TempBuilder::new().prefix(".multi-backup-rust-restore-")
        .tempdir_in(&canonical_parent)?;
    let result = with_reader(archive, identity, |archive| {
        scan(archive, |entry, member, relative| {
            let dest = staging.path().join(relative);
            let parent = dest.parent().ok_or_else(|| err("Invalid archive member parent"))?;
            assert_safe_parent(staging.path(), parent)?;
            if member.directory {
                if dest.exists() {
                    if !fs::symlink_metadata(&dest)?.is_dir() {
                        return Err(err("File/directory collision"));
                    }
                } else {
                    fs::create_dir(&dest)?;
                }
            } else {
                let mut file = OpenOptions::new().write(true).create_new(true).open(&dest)?;
                let actual = io::copy(entry, &mut file)?;
                if actual != member.bytes {
                    return Err(err("Truncated archive member"));
                }
                file.sync_all()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dest, fs::Permissions::from_mode(0o600))?;
                }
            }
            Ok(())
        })
    })?;
    // Publish the fully decrypted and checked restore atomically.
    fs::rename(staging.path(), &final_path)?;
    std::mem::forget(staging);
    Ok(result)
}

fn checked_sources(source: &Path) -> ArchiveResult<Vec<(PathBuf, PathBuf, bool, u64)>> {
    let meta = fs::symlink_metadata(source)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(err("Source must be an ordinary directory"));
    }
    let mut all = Vec::new();
    let mut bytes = 0u64;
    for item in WalkDir::new(source).follow_links(false).sort_by_file_name() {
        let entry = item?;
        if entry.path() == source { continue; }
        let kind = entry.file_type();
        if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
            return Err(err("Symlinks, sockets and special source files are forbidden"));
        }
        let size = if kind.is_file() { entry.metadata()?.len() } else { 0 };
        bytes = bytes.checked_add(size).ok_or_else(|| err("Source size overflow"))?;
        if bytes > MAX_UNCOMPRESSED || all.len() >= MAX_INPUT_FILES {
            return Err(err("Source exceeds archive limits"));
        }
        let relative = entry.path().strip_prefix(source)?.to_path_buf();
        safe_path(&relative.to_string_lossy().replace('\\', "/"))?;
        all.push((entry.path().to_path_buf(), relative, kind.is_dir(), size));
    }
    Ok(all)
}

pub fn create(source: &Path, destination: &Path, recipient: &str, prefix: &str)
    -> ArchiveResult<usize>
{
    if !recipient.starts_with("age1") || recipient.len() < 24 ||
        !recipient.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
        return Err(err("Invalid age recipient"));
    }
    if !destination.to_string_lossy().ends_with(".tar.gz.age") {
        return Err(err("Output name must end in .tar.gz.age"));
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err(err("Refusing to overwrite existing backup"));
    }
    let prefix = safe_path(prefix)?;
    let sources = checked_sources(source)?;
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = NamedTempFile::new_in(parent)?;
    let mut child = Command::new("age").arg("-r").arg(recipient)
        .stdin(Stdio::piped()).stdout(Stdio::from(temporary.reopen()?))
        .stderr(Stdio::piped()).spawn()?;
    let result = (|| -> ArchiveResult<()> {
        let stdin = child.stdin.take().ok_or_else(|| err("Cannot create encrypted stream"))?;
        let gzip = GzEncoder::new(stdin, Compression::default());
        let mut tar = Builder::new(gzip);
        for (path, relative, is_dir, size) in &sources {
            // Revalidate after scanning: do not follow a replaced symlink.
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() ||
               (if *is_dir { !metadata.is_dir() } else { !metadata.is_file() }) ||
               metadata.len() != *size && !is_dir {
                return Err(err("Source mutated while archiving"));
            }
            let member = prefix.join(relative);
            let mut header = Header::new_gnu();
            header.set_entry_type(if *is_dir { EntryType::Directory } else { EntryType::Regular });
            header.set_size(*size);
            header.set_mode(if *is_dir { 0o700 } else { 0o600 });
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            if *is_dir {
                tar.append_data(&mut header, &member, io::empty())?;
            } else {
                tar.append_data(&mut header, &member, File::open(path)?)?;
            }
        }
        let encoder = tar.into_inner()?;
        let stdin = encoder.finish()?;
        drop(stdin);
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return result.map(|_| sources.len());
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(err(format!("age encryption failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars().take(360).collect::<String>())));
    }
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(temporary.path())?, &mut hasher)?;
    let digest = format!("{:x}", hasher.finalize());
    temporary.persist_noclobber(destination).map_err(|e| e.error)?;
    let mut sidecar = NamedTempFile::new_in(parent)?;
    writeln!(sidecar, "{}  {}", digest, destination.file_name()
        .unwrap_or_default().to_string_lossy())?;
    sidecar.as_file().sync_all()?;
    let sidecar_path = PathBuf::from(format!("{}.sha256", destination.display()));
    sidecar.persist_noclobber(&sidecar_path).map_err(|e| e.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o600))?;
        fs::set_permissions(&sidecar_path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(sources.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_and_links_have_no_safe_member_path() {
        for s in ["", "/", "/etc/passwd", "../x", "a/../../b", "a//b",
                  "./x", "a/./b", "C:/windows/system32", "a\\..\\x",
                  "\\\\server\\share", "x\0y"] {
            assert!(safe_path(s).is_err(), "{s:?} was accepted");
        }
        assert_eq!(safe_path("multi-backup/repositories/foo.git/HEAD").unwrap(),
                   PathBuf::from("multi-backup/repositories/foo.git/HEAD"));
    }
    #[test]
    fn directory_member_trailing_slash_accepted() {
        assert!(safe_path("multi-backup/some/repo/").is_ok());
    }
    #[test]
    fn output_names_are_encrypted_and_no_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("src");
        fs::create_dir(&source).unwrap();
        let dest = tmp.path().join("bad.tar.gz");
        assert!(create(&source, &dest, "age1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "multi-backup").is_err());
    }
}
