//! Static native plugin contract for backup sources/destinations.
//!
//! No arbitrary dynamic libraries, scripts or executable plugins are loaded.
//! "Preview" means the provider has an existing dedicated Rust code path,
//! not that the common multi-plugin dispatch engine is integrated.
//! Catalog/plan are read-only: no credentials, network or data operations.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Unsupported,
    Planned,
    Preview,
    Ready,
}
impl Stage {
    pub fn listed(self) -> bool { self != Self::Unsupported }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Backup,
    Restore,
}
impl Mode {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "backup" => Ok(Self::Backup),
            "restore" => Ok(Self::Restore),
            _ => Err("Mode must be backup or restore".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Files,
    ObjectStore,
    BlockDevice,
    BlockTransport,
    ProtectedVault,
    Virtualization,
    Cluster,
    Database,
    RemoteShell,
    SourceControl,
}
/// Preflight profiles are descriptive and cannot grant disk or vault writes.
/// A future executor must independently verify snapshot origin and device
/// identity before any network-block target is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyProfile {
    General,
    SnapshotBlockDevice,
    ImmutableRecoveryVault,
}
/// The same stable plugin ID is used in both directions. Each capability is
/// declared separately: backup input -> restore output and backup output ->
/// restore input; this does not imply that either direction is implemented.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PluginDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: DataKind,
    pub backup_input: Stage,
    pub backup_output: Stage,
    pub restore_input: Stage,
    pub restore_output: Stage,
    pub consistency: &'static str,
    pub notes: &'static str,
    pub safety: SafetyProfile,
}
impl PluginDescriptor {
    const fn new(id: &'static str, title: &'static str, kind: DataKind,
                 backup_source: Stage, backup_target: Stage,
                 consistency: &'static str, notes: &'static str) -> Self {
        Self {
            id, title, kind,
            backup_input: backup_source,
            backup_output: backup_target,
            restore_input: backup_target,
            restore_output: backup_source,
            consistency, notes,
            safety: match kind {
                DataKind::BlockDevice | DataKind::BlockTransport =>
                    SafetyProfile::SnapshotBlockDevice,
                DataKind::ProtectedVault => SafetyProfile::ImmutableRecoveryVault,
                _ => SafetyProfile::General,
            },
        }
    }
    pub fn input_stage(self, mode: Mode) -> Stage {
        match mode { Mode::Backup => self.backup_input, Mode::Restore => self.restore_input }
    }
    pub fn output_stage(self, mode: Mode) -> Stage {
        match mode { Mode::Backup => self.backup_output, Mode::Restore => self.restore_output }
    }
}
// Preview is restricted to dedicated existing commands; the generic planner
// MUST NOT execute them, even for two Preview plugins.
// Role eligibility and implementation/readiness are separate.
const NONE: Stage = Stage::Unsupported;
const PLAN: Stage = Stage::Planned;
const PREVIEW: Stage = Stage::Preview;
pub const CATALOG: &[PluginDescriptor] = &[
    PluginDescriptor::new("github", "GitHub", DataKind::SourceControl, PREVIEW, NONE,
        "Git mirror+LFS and remote metadata checkpoint", "Repository/settings/Actions; remote restore creates new repositories only"),
    PluginDescriptor::new("dropbox", "Dropbox", DataKind::Files, PREVIEW, PLAN,
        "File ID+revision and content hash", "Native incremental read exists; target output plugin needs transactional upload/commit"),
    PluginDescriptor::new("google-drive", "Google Drive", DataKind::Files, PLAN, NONE,
        "File ID+revision/export format and shared-drive inventory", "Google-native documents require format-aware export and restore"),
    PluginDescriptor::new("onedrive", "OneDrive / SharePoint drive", DataKind::Files, PLAN, NONE,
        "Drive item+eTag/delta token", "Restore requires new-item permissions and a conflict policy"),
    PluginDescriptor::new("posix", "Local POSIX filesystem", DataKind::Files, PREVIEW, PREVIEW,
        "Filesystem walk+metadata+optional consistent snapshot", "Archive CLI supports local files; generic plugin wiring and complete ACL/xattr capture are pending"),
    PluginDescriptor::new("s3", "S3 / AWS / Ceph RGW", DataKind::ObjectStore, PLAN, PLAN,
        "Versioned bucket and multipart ETag/checksum", "Use scoped credentials, multipart resume, optional Object Lock and no overwrite by default"),
    PluginDescriptor::new("webdav", "WebDAV", DataKind::Files, PLAN, PLAN,
        "ETag/Last-Modified and explicit collection traversal", "Output capability requires write permissions; server's read-only DAV mount is unrelated"),
    PluginDescriptor::new("nfs", "NFS v3+", DataKind::Files, PLAN, PLAN,
        "Mounted filesystem plus server snapshot if available", "Support NFSv3, v4.x; do not assume root or atomic snapshots"),
    PluginDescriptor::new("smb", "CIFS / Samba", DataKind::Files, PLAN, NONE,
        "SMB durable handles or snapshots if available", "Input first; ACL/ADS/Windows path semantics need explicit mapping"),
    PluginDescriptor::new("cephfs", "CephFS", DataKind::Files, PLAN, PLAN,
        "CephFS snapshots or quiesced mount", "No cross-filesystem atomicity claim; separate CephFS and RBD plugins"),
    PluginDescriptor::new("rbd", "Ceph RADOS Block Device (RBD)", DataKind::BlockDevice, PLAN, NONE,
        "RBD image snapshots+diff export", "Block image is not a directory; restore requires explicit image creation, never silent overwrite"),
    PluginDescriptor::new("iscsi", "iSCSI LUN", DataKind::BlockTransport, PLAN, PLAN,
        "Initiator/target IQN and LUN WWID with storage snapshot",
        "Read only a stable quiesced block snapshot; output/restore must provision a distinct empty LUN and never overwrite an active device"),
    PluginDescriptor::new("fc", "Fibre Channel LUN", DataKind::BlockTransport, PLAN, PLAN,
        "FC WWPN, SCSI WWID, multipath and snapshot epoch",
        "Respect multipath aliases and ownership; output/restore require independently verified new-target LUN and exclusive fencing"),
    PluginDescriptor::new("nvmeof", "NVMe over Fabrics (NVMe-oF)", DataKind::BlockTransport, PLAN, PLAN,
        "NVMe NQN, NSID, EUI/NGUID and snapshot epoch",
        "Distinguish transport TCP/RDMA/FC, namespace and multipath; output/restore may write only an explicitly created isolated namespace"),
    PluginDescriptor::new("proxmox", "Proxmox VE / PBS", DataKind::Virtualization, PLAN, NONE,
        "VM/CT config plus vzdump/PBS consistency points", "Inventory VM/CT/storage, PBS integration, opt-in quiesce and stop/rollback planning"),
    PluginDescriptor::new("kubernetes", "Kubernetes / OpenShift / OKD", DataKind::Cluster, PLAN, NONE,
        "API object resourceVersion, CSI VolumeSnapshots and etcd/CRD scope", "Never capture Secrets without explicit policy; cluster restore requires name remapping"),
    PluginDescriptor::new("vmware", "VMware vSphere", DataKind::Virtualization, PLAN, NONE,
        "VADP/CBT snapshot and config inventory", "Restore into a new VM and datastore; license/API checks required"),
    PluginDescriptor::new("ovirt", "oVirt / RHV", DataKind::Virtualization, PLAN, NONE,
        "VM snapshot/engine API and volume exports", "Engine state, VM identity and disk snapshots are distinct scopes"),
    PluginDescriptor::new("mysql", "MySQL / MariaDB", DataKind::Database, PLAN, NONE,
        "Transactional snapshot plus binlog position", "Logical/physical backup requires engine-specific consistent backup and PITR policy"),
    PluginDescriptor::new("postgresql", "PostgreSQL", DataKind::Database, PLAN, NONE,
        "pg_basebackup/WAL or consistent logical snapshot", "PITR, role/grant restore and version compatibility require explicit acceptance"),
    PluginDescriptor::new("ssh", "SSH / SCP / SFTP", DataKind::RemoteShell, PLAN, PLAN,
        "Remote stat+hash, optional filesystem snapshot", "Strict host-key pinning; no shell interpolation, deletes or unsafe redirects"),
    PluginDescriptor::new("anti-ransomware-vault", "Anti-ransomware immutable vault",
        DataKind::ProtectedVault, NONE, PLAN,
        "WORM/Object Lock retention, manifest-last atomic commit and independent recovery approval",
        "Output ciphertext only via isolated append-only ingest; restore input is read-only. WORM and independent approval are NOT yet implemented or certified"),
];

pub fn find(id: &str) -> Option<&'static PluginDescriptor> {
    CATALOG.iter().find(|x| x.id == id)
}
pub fn catalog_for(mode: Mode, input: bool) -> Vec<&'static PluginDescriptor> {
    CATALOG.iter().filter(|x| {
        if input { x.input_stage(mode).listed() } else { x.output_stage(mode).listed() }
    }).collect()
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub mode: Mode,
    pub source_plugin: &'static str,
    pub destination_plugin: &'static str,
    pub source_stage: Stage,
    pub destination_stage: Stage,
    pub executable: bool,
    pub reason: &'static str,
    pub confirmation_required_before_writes: bool,
}
/// Planning never executes a plugin; it is a safe capability check. In
/// particular, Preview != Ready and no arbitrary command or plug-in is run.
pub fn plan(mode: Mode, source_id: &str, destination_id: &str) -> Result<Plan, String> {
    let source = find(source_id).ok_or_else(|| format!("Unknown plugin: {source_id}"))?;
    let destination = find(destination_id).ok_or_else(|| format!("Unknown plugin: {destination_id}"))?;
    let source_stage = source.input_stage(mode);
    let destination_stage = destination.output_stage(mode);
    if !source_stage.listed() {
        return Err(format!("{source_id} cannot be used as a {} input", match mode {
            Mode::Backup => "backup", Mode::Restore => "restore"
        }));
    }
    if !destination_stage.listed() {
        return Err(format!("{destination_id} cannot be used as a {} output", match mode {
            Mode::Backup => "backup", Mode::Restore => "restore"
        }));
    }
    // A generic multi-plugin job requires streaming, verified commit and
    // restore preflight adapters, none of which are ready yet.
    let executable = source_stage == Stage::Ready && destination_stage == Stage::Ready;
    Ok(Plan {
        mode, source_plugin: source.id, destination_plugin: destination.id,
        source_stage, destination_stage, executable,
        reason: if executable { "both adapters are ready" } else {
            "catalog/planning only: one or both generic adapters are not wired or qualified"
        },
        confirmation_required_before_writes: true,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn all_21_user_requested_input_plugins_and_11_outputs_are_listed() {
        let inputs = ["github", "dropbox", "google-drive", "onedrive", "posix",
            "s3", "webdav", "nfs", "smb", "cephfs", "rbd", "proxmox",
            "kubernetes", "vmware", "ovirt", "mysql", "postgresql", "ssh",
            "iscsi", "fc", "nvmeof"];
        let outputs = ["posix", "dropbox", "s3", "webdav", "cephfs", "nfs", "ssh",
            "iscsi", "fc", "nvmeof", "anti-ransomware-vault"];
        assert_eq!(catalog_for(Mode::Backup, true).len(), 21);
        assert_eq!(catalog_for(Mode::Backup, false).len(), 11);
        assert_eq!(CATALOG.len(), 22);
        for id in inputs { assert!(find(id).unwrap().backup_input.listed(), "{id}"); }
        for id in outputs { assert!(find(id).unwrap().backup_output.listed(), "{id}"); }
        let distinct: HashSet<_> = CATALOG.iter().map(|x| x.id).collect();
        assert_eq!(distinct.len(), CATALOG.len());
    }
    #[test]
    fn restore_reverses_backup_roles_without_claiming_write_support() {
        assert_eq!(catalog_for(Mode::Restore, true).len(), 11);
        assert_eq!(catalog_for(Mode::Restore, false).len(), 21);
        for entry in CATALOG {
            assert_eq!(entry.backup_input, entry.restore_output);
            assert_eq!(entry.backup_output, entry.restore_input);
        }
    }
    #[test]
    fn unsupported_roles_fail_closed_and_unknown_names_are_rejected() {
        assert!(plan(Mode::Backup, "github", "mysql").is_err());
        assert!(plan(Mode::Restore, "github", "dropbox").is_err());
        assert!(plan(Mode::Backup, "not-an-installed-plugin", "posix").is_err());
    }
    #[test]
    fn plans_are_non_destructive_until_real_adapters_pass_gates() {
        for source in catalog_for(Mode::Backup, true) {
            for target in catalog_for(Mode::Backup, false) {
                let p=plan(Mode::Backup, source.id, target.id).unwrap();
                assert!(!p.executable);
                assert!(p.confirmation_required_before_writes);
            }
        }
        for source in catalog_for(Mode::Restore, true) {
            for target in catalog_for(Mode::Restore, false) {
                assert!(!plan(Mode::Restore, source.id, target.id).unwrap().executable);
            }
        }
    }
    #[test]
    fn preview_does_not_masquerade_as_qualified_plugin() {
        let p=plan(Mode::Backup, "github", "posix").unwrap();
        assert_eq!(p.source_stage, Stage::Preview);
        assert_eq!(p.destination_stage, Stage::Preview);
        assert!(!p.executable);
        assert_eq!(find("postgresql").unwrap().kind, DataKind::Database);
        assert_eq!(find("rbd").unwrap().kind, DataKind::BlockDevice);
    }
    #[test]
    fn san_block_sources_and_targets_require_consistent_snapshot_preflight() {
        for id in ["iscsi", "fc", "nvmeof"] {
            let descriptor=find(id).unwrap();
            assert_eq!(descriptor.kind, DataKind::BlockTransport);
            assert_eq!(descriptor.safety, SafetyProfile::SnapshotBlockDevice);
            assert_eq!(descriptor.backup_input, Stage::Planned);
            assert_eq!(descriptor.backup_output, Stage::Planned);
            assert_eq!(descriptor.restore_input, Stage::Planned);
            assert_eq!(descriptor.restore_output, Stage::Planned);
            assert!(!plan(Mode::Backup, id, "anti-ransomware-vault").unwrap().executable);
            assert!(!plan(Mode::Restore, "anti-ransomware-vault", id).unwrap().executable);
        }
        assert_eq!(find("rbd").unwrap().safety, SafetyProfile::SnapshotBlockDevice);
    }
    #[test]
    fn anti_ransomware_vault_is_ciphertext_destination_and_restore_source_only() {
        let vault=find("anti-ransomware-vault").unwrap();
        assert_eq!(vault.safety, SafetyProfile::ImmutableRecoveryVault);
        assert_eq!(vault.backup_input, Stage::Unsupported);
        assert_eq!(vault.backup_output, Stage::Planned);
        assert_eq!(vault.restore_input, Stage::Planned);
        assert_eq!(vault.restore_output, Stage::Unsupported);
        assert!(plan(Mode::Backup, "posix", vault.id).is_ok());
        assert!(plan(Mode::Restore, vault.id, "posix").is_ok());
        assert!(plan(Mode::Backup, vault.id, "posix").is_err());
        assert!(plan(Mode::Restore, "posix", vault.id).is_err());
        assert!(!plan(Mode::Backup, "posix", vault.id).unwrap().executable);
    }
    #[test]
    fn plan_mode_is_not_silently_defaulted() {
        assert!(Mode::parse("Backup").is_err());
        assert_eq!(Mode::parse("backup").unwrap(), Mode::Backup);
        assert_eq!(Mode::parse("restore").unwrap(), Mode::Restore);
    }
}
