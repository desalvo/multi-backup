//! Explicit, compiled plugin dispatcher for verified *local* filesystem jobs.
//! POSIX->POSIX and read-only mounted NFS/SMB/SSHFS/GPFS sources to POSIX
//! are enabled as development previews. Untested roles always fail closed.
//! This layer will be extended with native per-provider adapters.
use crate::{chunk_store::{BackupKind, LogicalBackup, Source, Store}, plugins::{self, Mode, GpfsAccess}, mount_probe};
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

fn supported_pair(mode: Mode, input: &str, output: &str) -> Result<()> {
    // Verify registry roles before granting the narrow preview implementation.
    plugins::plan(mode, input, output)?;
    let supported = match mode {
        Mode::Backup => output == "posix" && matches!(input,
            "posix" | "nfs" | "smb" | "sshfs" | "gpfs"),
        Mode::Restore => input == "posix" && output == "posix",
    };
    if !supported {
        return Err(format!("No native generic executor for {input} -> {output}; operation denied").into());
    }
    Ok(())
}

/// Back up a local directory or an independently mounted, read-only snapshot.
/// External mounted sources require explicit operator attestation and exact
/// mount-type evidence; this does not itself prove a snapshot or ACL fidelity.
pub fn backup(input: &str, output: &str, source: &Path, root: &Path,
              identity: &Path, label: &str, namespace: &str,
              top_namespace: &str, snapshot_attested: bool,
              gpfs_access: Option<GpfsAccess>) -> Result<LogicalBackup> {
    backup_with_mode(input,output,source,root,identity,label,namespace,
        top_namespace,snapshot_attested,gpfs_access,BackupKind::Full,None)
}
pub fn backup_with_mode(input: &str, output: &str, source: &Path, root: &Path,
              identity: &Path, label: &str, namespace: &str,
              top_namespace: &str, snapshot_attested: bool,
              gpfs_access: Option<GpfsAccess>,kind:BackupKind,
              parent_id:Option<&str>) -> Result<LogicalBackup> {
    supported_pair(Mode::Backup, input, output)?;
    let source = regular_directory(source)?;
    if input == "posix" {
        if gpfs_access.is_some() || snapshot_attested {
            return Err("POSIX source does not accept remote mount/snapshot claims".into());
        }
    } else {
        if !snapshot_attested {
            return Err("Mounted source requires explicit --source-snapshot-attested".into());
        }
        let mount = mount_probe::probe(input, &source, gpfs_access)?;
        if mount.rw_mount_option {
            return Err("Mounted input must be read-only for native preview backup".into());
        }
    }
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
    let snapshot = store.backup_in_namespace_mode(&identity,label,top_namespace,
        &[Source { namespace: namespace.to_owned(), directory: source }],
        kind,parent_id)?;
    if !store.verify(&identity, &snapshot.id)?.healthy {
        return Err("Created backup did not pass encrypted integrity verification".into());
    }
    Ok(snapshot)
}

/// Restore into a new local directory only, after verifying the encrypted
/// store. The restore parent must exist and cannot be part of the store.
pub fn restore(input: &str, output: &str, root: &Path, identity: &Path,
               id: &str, destination: &Path, selection: Option<&str>) -> Result<usize> {
    supported_pair(Mode::Restore, input, output)?;
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
    // A single damaged shard may be recoverable from 8+2 parity. Attempt
    // authenticated parity repair *before* rejecting the restore, and verify
    // again. Never publish the destination when recovery is incomplete.
    if !store.verify(&identity, id)?.healthy {
        store.repair(&identity, id)?;
        if !store.verify(&identity, id)?.healthy {
            return Err("Refusing to restore a backup with unrecoverable integrity errors".into());
        }
    }
    store.restore(&identity, id, selection, &full_destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unimplemented_providers_refuse_execution() {
        assert!(supported_pair(Mode::Backup, "github", "posix").is_err());
        assert!(supported_pair(Mode::Backup, "nfs", "posix").is_ok());
        assert!(supported_pair(Mode::Backup, "smb", "posix").is_ok());
        assert!(supported_pair(Mode::Backup, "gpfs", "posix").is_ok());
        assert!(supported_pair(Mode::Backup, "sshfs", "posix").is_ok());
        assert!(supported_pair(Mode::Backup, "posix", "anti-ransomware-vault").is_err());
        assert!(supported_pair(Mode::Restore, "posix", "github").is_err());
        assert!(supported_pair(Mode::Backup, "posix", "posix").is_ok());
    }
}
