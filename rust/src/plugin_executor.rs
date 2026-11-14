//! Explicit, compiled plugin dispatcher for verified *local* filesystem jobs.
//! Only POSIX->POSIX is enabled as a development preview. Unsupported
//! combinations must fail closed: the catalog is not an executable driver.
//! This layer will be extended with native per-provider adapters.
use crate::{chunk_store::{LogicalBackup, Source, Store}, plugins::{self, Mode}};
use std::{fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn regular_directory(path: &Path) -> Result<std::path::PathBuf> {
    let kind = fs::symlink_metadata(path)?.file_type();
    if !kind.is_dir() || kind.is_symlink() {
        return Err("Plugin path must be an existing real directory (not a symlink)".into());
    }
    Ok(fs::canonicalize(path)?)
}

fn regular_identity(identity: &Path) -> Result<std::path::PathBuf> {
    let kind = fs::symlink_metadata(identity)?.file_type();
    if !kind.is_file() || kind.is_symlink() {
        return Err("Plugin age identity must be an existing ordinary non-symlink file".into());
    }
    Ok(fs::canonicalize(identity)?)
}

fn local_pair(mode: Mode, input: &str, output: &str) -> Result<()> {
    // Validate registered directional roles, not just string equality.
    plugins::plan(mode, input, output)?;
    if input != "posix" || output != "posix" {
        return Err(format!("No native generic executor for {input} -> {output}; only POSIX -> POSIX preview is enabled").into());
    }
    Ok(())
}

/// Back up one real, local directory into a previously initialized encrypted
/// chunk store. Never recurse into the store or capture an age private key.
/// Source snapshots/ACLs/xattrs are NOT yet production-qualified.
pub fn backup(input: &str, output: &str, source: &Path, root: &Path,
              identity: &Path, label: &str, namespace: &str,
              top_namespace: &str) -> Result<LogicalBackup> {
    local_pair(Mode::Backup, input, output)?;
    let source = regular_directory(source)?;
    let root = regular_directory(root)?;
    let identity = regular_identity(identity)?;
    if source.starts_with(&root) || root.starts_with(&source) {
        return Err("Source and encrypted store cannot contain one another".into());
    }
    if identity.starts_with(&source) {
        return Err("Refusing to back up age private identity from source tree".into());
    }
    if label.trim().is_empty() {
        return Err("Backup requires a nonempty label".into());
    }
    let store = Store::open(&root, &identity)?;
    let snapshot = store.backup_in_namespace(&identity, label, top_namespace,
        &[Source { namespace: namespace.to_owned(), directory: source }])?;
    if !store.verify(&identity, &snapshot.id)?.healthy {
        return Err("Created backup did not pass encrypted integrity verification".into());
    }
    Ok(snapshot)
}

/// Restore into a new local directory only, after verifying the encrypted
/// store. The restore parent must exist and cannot be part of the store.
pub fn restore(input: &str, output: &str, root: &Path, identity: &Path,
               id: &str, destination: &Path, selection: Option<&str>) -> Result<usize> {
    local_pair(Mode::Restore, input, output)?;
    let root = regular_directory(root)?;
    let identity = regular_identity(identity)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err("Refusing to overwrite an existing plugin restore destination".into());
    }
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty())
        .ok_or("Restore must specify an explicit destination parent")?;
    let parent = regular_directory(parent)?;
    let name = destination.file_name().ok_or("Invalid restore directory name")?;
    if name == "." || name == ".." {
        return Err("Invalid restore directory name".into());
    }
    let full_destination = parent.join(name);
    if full_destination.starts_with(&root) || root.starts_with(&full_destination) {
        return Err("Restore destination and encrypted store may not overlap".into());
    }
    if fs::symlink_metadata(&full_destination).is_ok() {
        return Err("Refusing to overwrite an existing plugin restore destination".into());
    }
    let store = Store::open(&root, &identity)?;
    if !store.verify(&identity, id)?.healthy {
        return Err("Refusing to restore a backup with failed integrity verification".into());
    }
    store.restore(&identity, id, selection, &full_destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unimplemented_providers_refuse_execution() {
        assert!(local_pair(Mode::Backup, "github", "posix").is_err());
        assert!(local_pair(Mode::Backup, "nfs", "posix").is_err());
        assert!(local_pair(Mode::Backup, "posix", "anti-ransomware-vault").is_err());
        assert!(local_pair(Mode::Restore, "posix", "github").is_err());
        assert!(local_pair(Mode::Backup, "posix", "posix").is_ok());
    }
}
