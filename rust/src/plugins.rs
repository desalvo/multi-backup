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
    DistributedFilesystem,
    DocumentRepository,
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
/// External dependencies are policy gates, never permission to bundle vendor
/// code or auto-request elevated cloud scopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessGate {
    ExternalProtocolOrMountedClient,
    LicensedExternalClient,
    OAuthScopeReview,
}
/// Operator-mounted open-protocol CES exports. No native IBM GPFS client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GpfsAccess {
    CesNfs,
    CesSmb,
}
impl GpfsAccess {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "ces-nfs" => Ok(Self::CesNfs),
            "ces-smb" => Ok(Self::CesSmb),
            _ => Err("GPFS access must be ces-nfs or ces-smb".into()),
        }
    }
}
const GPFS_CES_ACCESS: &[GpfsAccess] = &[GpfsAccess::CesNfs,GpfsAccess::CesSmb];
/// LVM over iSCSI/FC is a policy *profile*, not a separate transport or
/// a claim that a shared VG can be activated on multiple uncoordinated hosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LvmMode {
    Local,
    ClusterLocal,
    SharedExclusive,
    HaExclusive,
    ClusterShared,
}
impl LvmMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "cluster-local" => Ok(Self::ClusterLocal),
            "shared-exclusive" => Ok(Self::SharedExclusive),
            "ha-exclusive" => Ok(Self::HaExclusive),
            "cluster-shared" => Ok(Self::ClusterShared),
            _ => Err("LVM mode must be local, cluster-local, shared-exclusive, ha-exclusive or cluster-shared".into()),
        }
    }
    fn clustered(self) -> bool {
        matches!(self, Self::ClusterLocal | Self::HaExclusive | Self::ClusterShared)
    }
}
const SAN_LVM_MODES: &[LvmMode] = &[
    LvmMode::Local, LvmMode::ClusterLocal, LvmMode::SharedExclusive,
    LvmMode::HaExclusive, LvmMode::ClusterShared,
];
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
    pub access_gate: AccessGate,
    pub gpfs_access: &'static [GpfsAccess],
    /// A nonempty list is supported only by iSCSI/FC SAN LVM profiles;
    /// raw LUNs omit the LVM mode and remain separately snapshot-gated.
    pub lvm_modes: &'static [LvmMode],
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
            access_gate: AccessGate::ExternalProtocolOrMountedClient,
            gpfs_access: &[],
            lvm_modes: &[],
            safety: match kind {
                DataKind::BlockDevice | DataKind::BlockTransport =>
                    SafetyProfile::SnapshotBlockDevice,
                DataKind::ProtectedVault => SafetyProfile::ImmutableRecoveryVault,
                _ => SafetyProfile::General,
            },
        }
    }
    const fn with_access_gate(mut self, gate: AccessGate) -> Self {
        self.access_gate = gate;
        self
    }
    const fn with_gpfs_ces(mut self) -> Self {
        self.gpfs_access = GPFS_CES_ACCESS;
        self
    }
    const fn with_san_lvm(mut self) -> Self {
        self.lvm_modes = SAN_LVM_MODES;
        self
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
    PluginDescriptor::new("google-drive", "Google Drive", DataKind::Files, PLAN, PLAN,
        "File ID+revision/export format and shared-drive inventory",
        "OAuth-scoped Drive app folder and Google-native export; upload encrypted objects to a new destination with no overwrite").with_access_gate(AccessGate::OAuthScopeReview),
    PluginDescriptor::new("onedrive", "OneDrive / SharePoint drive", DataKind::Files, PLAN, PLAN,
        "Microsoft Graph driveItem+eTag/delta token",
        "Graph OAuth/tenant consent, upload-session retry and explicit conflict handling for new encrypted objects").with_access_gate(AccessGate::OAuthScopeReview),
    PluginDescriptor::new("posix", "Local POSIX filesystem", DataKind::Files, PREVIEW, PREVIEW,
        "Filesystem walk+metadata+optional consistent snapshot", "Archive CLI supports local files; generic plugin wiring and complete ACL/xattr capture are pending"),
    PluginDescriptor::new("s3", "S3 / AWS / Ceph RGW", DataKind::ObjectStore, PLAN, PLAN,
        "Versioned bucket and multipart ETag/checksum", "Use scoped credentials, multipart resume, optional Object Lock and no overwrite by default"),
    PluginDescriptor::new("webdav", "WebDAV", DataKind::Files, PLAN, PLAN,
        "ETag/Last-Modified and explicit collection traversal", "Output capability requires write permissions; server's read-only DAV mount is unrelated"),
    PluginDescriptor::new("nfs", "NFS v3+", DataKind::Files, PREVIEW, PLAN,
        "Mounted filesystem plus server snapshot if available", "Support NFSv3, v4.x; do not assume root or atomic snapshots"),
    PluginDescriptor::new("smb", "CIFS / Samba", DataKind::Files, PREVIEW, NONE,
        "SMB durable handles or snapshots if available", "Input first; ACL/ADS/Windows path semantics need explicit mapping"),
    PluginDescriptor::new("cephfs", "CephFS", DataKind::Files, PREVIEW, PLAN,
        "CephFS snapshots or quiesced mount", "No cross-filesystem atomicity claim; separate CephFS and RBD plugins"),
    PluginDescriptor::new("glusterfs", "GlusterFS", DataKind::Files, PREVIEW, PLAN,
        "Mounted Gluster volume inventory with consistency epoch and preserved GFID/ACL/xattr metadata",
        "Quiesce or coordinate snapshots by volume topology (replica/disperse); no direct brick writes, restores to new isolated directory/volume"),
    PluginDescriptor::new("lustre", "Lustre parallel filesystem", DataKind::DistributedFilesystem, PREVIEW, PLAN,
        "Mounted Lustre client, coordinated MDT/OST consistent snapshot, stripe and ACL/xattr metadata",
        "Use separately installed authorized Lustre client; avoid bundling kernel modules; writes only to a new isolated mounted destination"),
    PluginDescriptor::new("gpfs", "IBM Storage Scale / GPFS via CES", DataKind::DistributedFilesystem, PREVIEW, PLAN,
        "NFSv3/v4 or SMB3 CES export on an authorized server; consistency proved separately",
        "Use distro open-source NFS/CIFS client; no IBM native client shipped; CES server licensing, permission and snapshot fidelity remain operator responsibilities")
        .with_gpfs_ces(),
    PluginDescriptor::new("hdfs", "Hadoop HDFS", DataKind::DistributedFilesystem, PLAN, PLAN,
        "SWebHDFS/WebHDFS, HDFS snapshot/directory metadata, ACL/xattrs, checksums",
        "Kerberos/delegation token, TLS and redirect allowlist; write only new paths, commit encrypted manifest last"),
    PluginDescriptor::new("afs", "Andrew File System / OpenAFS", DataKind::DistributedFilesystem, PLAN, PLAN,
        "AFS cell+volume IDs and VLDB inventory; read-only/backup volume snapshot, vos full/incremental dump chain and ACLs",
        "Operator-supplied OpenAFS client/tools under IBM Public License; preserve cell, mountpoints and ACLs; restore new volume only"),
    PluginDescriptor::new("alfresco", "Alfresco Content Services", DataKind::DocumentRepository, PLAN, PLAN,
        "Alfresco REST node ID, content revisions, type/aspects, ACL, metadata and relationships",
        "Use public REST/CMIS integrations only; enterprise-specific functions and editions require rights and API checks"),
    PluginDescriptor::new("owncloud", "ownCloud", DataKind::Files, PLAN, PLAN,
        "ownCloud WebDAV/OCS file ID, ETag, sharing, versions and metadata",
        "External WebDAV/OCS protocol integration; validate server edition, upload semantics, permissions and conflict control"),
    PluginDescriptor::new("nextcloud", "Nextcloud", DataKind::Files, PLAN, PLAN,
        "Nextcloud WebDAV/OCS file ID, ETag, sharing, versions and chunked upload",
        "External WebDAV/OCS protocol integration; validate server version and atomic verified commits; never overwrite"),
    PluginDescriptor::new("rbd", "Ceph RADOS Block Device (RBD)", DataKind::BlockDevice, PLAN, NONE,
        "RBD image snapshots+diff export", "Block image is not a directory; restore requires explicit image creation, never silent overwrite"),
    PluginDescriptor::new("iscsi", "iSCSI LUN", DataKind::BlockTransport, PLAN, PLAN,
        "Initiator/target IQN and LUN WWID with storage snapshot",
        "Read only a stable quiesced block snapshot; supports raw LUN or LVM (local, clustered-local, shared-exclusive, HA-exclusive, clustered-shared); restore to fresh isolated VG/LUN only").with_san_lvm(),
    PluginDescriptor::new("fc", "Fibre Channel LUN", DataKind::BlockTransport, PLAN, PLAN,
        "FC WWPN, SCSI WWID, multipath and snapshot epoch",
        "Respect multipath aliases, WWID and ownership; raw FC LUN or LVM local/clustered/shared, with fencing/locking and snapshot consistency").with_san_lvm(),
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
    PluginDescriptor::new("oracle", "Oracle Database", DataKind::Database, PLAN, NONE,
        "Online RMAN ARCHIVELOG: validated datafiles, controlfile/SPFILE, redo sequence, database SCN",
        "Requires licensed Oracle instance and locally installed RMAN; never ship Oracle binaries or enable extra-cost Oracle compression/encryption/standby features automatically"),
    PluginDescriptor::new("ssh", "SSH / SCP / SFTP", DataKind::RemoteShell, PLAN, PLAN,
        "Remote stat+hash, optional filesystem snapshot", "Strict host-key pinning; no shell interpolation, deletes or unsafe redirects"),
    PluginDescriptor::new("sshfs", "SSHFS (FUSE mounted SFTP)", DataKind::Files, PREVIEW, PLAN,
        "Existing verified SSHFS mount, strict host key, file hashes/metadata",
        "External open-source GPL-2.0 SSHFS/FUSE client only; patched version, safe symlink policy, no insecure directport and no overwrite"),
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
    /// Explicit compiled experimental path exists for this pair, but its
    /// complete provider metadata/consistency semantics are not certified.
    pub preview_executable: bool,
    pub reason: &'static str,
    pub confirmation_required_before_writes: bool,
    /// Declarative planning only: neither actual locks nor a storage array
    /// snapshot are acquired by the plugin planner.
    pub input_lvm_mode: Option<LvmMode>,
    pub output_lvm_mode: Option<LvmMode>,
    pub input_gpfs_access: Option<GpfsAccess>,
    pub output_gpfs_access: Option<GpfsAccess>,
    pub required_preflight: Vec<&'static str>,
}
/// Planning never executes a plugin; it is a safe capability check. In
/// particular, Preview != Ready and no arbitrary command or plug-in is run.
pub fn plan(mode: Mode, source_id: &str, destination_id: &str) -> Result<Plan, String> {
    plan_lvm(mode, source_id, destination_id, None, None)
}

/// Validate the declarative SAN LVM mode of each endpoint independently.
/// This intentionally performs NO mount, VG activation, fencing or write.
pub fn plan_lvm(mode: Mode, source_id: &str, destination_id: &str,
                input_lvm_mode: Option<LvmMode>,
                output_lvm_mode: Option<LvmMode>) -> Result<Plan, String> {
    plan_profiles(mode,source_id,destination_id,input_lvm_mode,output_lvm_mode,None,None)
}
/// Only describe prerequisites. NEVER mount storage, launch SSH or write data.
pub fn plan_profiles(mode: Mode, source_id: &str, destination_id: &str,
                     input_lvm_mode: Option<LvmMode>, output_lvm_mode: Option<LvmMode>,
                     input_gpfs_access: Option<GpfsAccess>,
                     output_gpfs_access: Option<GpfsAccess>) -> Result<Plan, String> {
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
    let mut required_preflight = Vec::new();
    for (plugin, layout, gpfs_mode, writing) in [
        (source, input_lvm_mode, input_gpfs_access, false),
        (destination, output_lvm_mode, output_gpfs_access, true),
    ] {
        if let Some(selected) = gpfs_mode {
            if !plugin.gpfs_access.contains(&selected) {
                return Err(format!("Plugin {} does not support GPFS CES access",plugin.id));
            }
        }
        match plugin.access_gate {
            AccessGate::LicensedExternalClient =>
                required_preflight.push("verify customer-specific vendor license, data export entitlement and separate supported client installation; never bundle proprietary code"),
            AccessGate::OAuthScopeReview =>
                required_preflight.push("verify OAuth app consent, provider API restrictions and least-privilege drive/tenant scopes"),
            AccessGate::ExternalProtocolOrMountedClient => {}
        }
        if plugin.id == "afs" {
            required_preflight.push("verify AFS cell, VLDB volume ID, volume type, Kerberos tokens, ACLs and mountpoints; preserve full/incremental dump chain");
            required_preflight.push("select a stable read-only/backup-volume snapshot; do not snapshot or modify an active RW volume in place");
            if writing {
                required_preflight.push("restore only to a fresh non-existing AFS volume in an approved cell; verify VLDB mapping and ACL before publish");
            }
        }
        if plugin.id == "hdfs" {
            required_preflight.push("verify HDFS snapshot, NameNode/DataNode TLS redirection policy, Kerberos and inode/ACL consistency");
            if writing {
                required_preflight.push("publish encrypted objects into a new HDFS directory, checksum content and commit manifest last");
            }
        }
        if plugin.id == "gpfs" {
            required_preflight.push("verify authorized IBM CES server/export; NFS/SMB clients require no GPFS Client license");
            required_preflight.push("verify an existing authenticated NFSv3/v4 or CIFS/SMB3 mount provided by OS open-source tools");
            required_preflight.push("prove out-of-band GPFS fileset snapshot consistency, ACLs/xattrs and vendor metadata fidelity");
            if gpfs_mode.is_none() {
                required_preflight.push("explicitly select GPFS access ces-nfs or ces-smb; no proprietary native client fallback");
            }
            if writing {
                required_preflight.push("verify new isolated target, write ACL, encrypted-only upload and manifest-last commit without overwrite");
            }
        }
        if plugin.id == "sshfs" {
            required_preflight.push("verify existing patched SSHFS/FUSE mount, pinned SSH host key and secure authentication");
            required_preflight.push("reject insecure directport bypass, symlink escapes, disconnected mount and ambiguous paths");
            if writing {
                required_preflight.push("verify new empty target, write ACL and encrypted-only manifest-last commit without overwrite");
            }
        }
        if plugin.id == "lustre" {
            required_preflight.push("verify supported authorized mounted client, consistent namespace/fileset snapshot and recoverable filesystem metadata");
            if writing {
                required_preflight.push("write encrypted objects only to a fresh isolated mounted destination with independent authorization");
            }
        }
        if plugin.id == "alfresco" || plugin.id == "owncloud" || plugin.id == "nextcloud" {
            required_preflight.push("verify REST/DAV API rights, revisions, ACL/sharing metadata and safe new-object semantics");
        }
        if let Some(selected) = layout {
            if !plugin.lvm_modes.contains(&selected) {
                return Err(format!("Plugin {} does not support this LVM profile", plugin.id));
            }
            required_preflight.push("verify LUN WWID, PV UUID, VG/LV UUID, multipath aliases and ownership");
            required_preflight.push("prove quiesced/snapshotted application-consistent LV data and valid LVM metadata");
            if matches!(selected, LvmMode::SharedExclusive | LvmMode::HaExclusive) {
                required_preflight.push("verify exclusive VG/LV activation on one writer and cluster fencing/lease where applicable");
            }
            if selected.clustered() {
                required_preflight.push("verify cluster membership, quorum, node identity and VG activation policy");
            }
            if selected == LvmMode::ClusterShared {
                required_preflight.push("verify working distributed LVM lock manager (lvmlockd with supported DLM/sanlock), fencing and cluster-aware filesystem/application");
            }
        }
        if plugin.safety == SafetyProfile::SnapshotBlockDevice {
            required_preflight.push("verify stable, read-only snapshot source and no concurrent block writes");
            if writing {
                required_preflight.push("provision a distinct empty isolated target LUN/namespace/VG, confirm no active production path and require explicit approval");
            }
        }
        if plugin.safety == SafetyProfile::ImmutableRecoveryVault {
            required_preflight.push("independently attest WORM retention, append-only ingest and separate recovery authorizations");
        }
    }
    // A generic multi-plugin job requires streaming, verified commit and
    // restore preflight adapters, none of which are ready yet.
    let executable = source_stage == Stage::Ready && destination_stage == Stage::Ready;
    let preview_executable = match mode {
        Mode::Backup => destination.id == "posix" && matches!(source.id,
            "posix" | "nfs" | "smb" | "gpfs" | "sshfs"),
        Mode::Restore => source.id == "posix" && destination.id == "posix",
    };
    Ok(Plan {
        mode, source_plugin: source.id, destination_plugin: destination.id,
        source_stage, destination_stage, executable, preview_executable,
        reason: if executable { "both adapters are ready" } else if preview_executable {
            "compiled local or read-only mounted-source preview available; production qualification pending"
        } else {
            "catalog/planning only: one or both generic adapters are not wired or qualified"
        },
        confirmation_required_before_writes: true,
        input_lvm_mode, output_lvm_mode, input_gpfs_access, output_gpfs_access,
        required_preflight,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn all_31_user_requested_input_plugins_and_22_outputs_are_listed() {
        let inputs = ["github", "dropbox", "google-drive", "onedrive", "posix",
            "s3", "webdav", "nfs", "smb", "cephfs", "rbd", "proxmox",
            "kubernetes", "vmware", "ovirt", "mysql", "postgresql", "ssh",
            "iscsi", "fc", "nvmeof", "glusterfs", "lustre", "gpfs",
            "hdfs", "afs", "alfresco", "owncloud", "nextcloud", "sshfs",
            "oracle"];
        let outputs = ["posix", "dropbox", "s3", "webdav", "cephfs", "nfs", "ssh",
            "iscsi", "fc", "nvmeof", "anti-ransomware-vault", "glusterfs",
            "lustre", "gpfs", "hdfs", "afs", "alfresco", "google-drive", "onedrive",
            "owncloud", "nextcloud", "sshfs"];
        assert_eq!(catalog_for(Mode::Backup, true).len(), 31);
        assert_eq!(catalog_for(Mode::Backup, false).len(), 22);
        assert_eq!(CATALOG.len(), 32);
        for id in inputs { assert!(find(id).unwrap().backup_input.listed(), "{id}"); }
        for id in outputs { assert!(find(id).unwrap().backup_output.listed(), "{id}"); }
        let distinct: HashSet<_> = CATALOG.iter().map(|x| x.id).collect();
        assert_eq!(distinct.len(), CATALOG.len());
    }
    #[test]
    fn restore_reverses_backup_roles_without_claiming_write_support() {
        assert_eq!(catalog_for(Mode::Restore, true).len(), 22);
        assert_eq!(catalog_for(Mode::Restore, false).len(), 31);
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
    fn glusterfs_remains_planned_in_both_directions() {
        let d=find("glusterfs").unwrap();
        assert_eq!(d.kind, DataKind::Files);
        assert_eq!(d.backup_input, Stage::Planned);
        assert_eq!(d.backup_output, Stage::Planned);
        assert_eq!(d.restore_input, Stage::Planned);
        assert_eq!(d.restore_output, Stage::Planned);
        assert!(d.lvm_modes.is_empty());
        assert!(!plan(Mode::Backup, "glusterfs", "anti-ransomware-vault").unwrap().executable);
        assert!(!plan(Mode::Restore, "anti-ransomware-vault", "glusterfs").unwrap().executable);
    }
    #[test]
    fn iscsi_and_fc_support_distinct_local_cluster_and_shared_lvm_profiles() {
        for transport in ["iscsi", "fc"] {
            let d=find(transport).unwrap();
            assert_eq!(d.lvm_modes.len(), 5);
            for mode in SAN_LVM_MODES {
                let plan=plan_lvm(Mode::Backup, transport, "glusterfs",
                                  Some(*mode), None).unwrap();
                assert_eq!(plan.input_lvm_mode, Some(*mode));
                assert!(!plan.executable);
                assert!(plan.required_preflight.len() >= 3);
                let reverse=plan_lvm(Mode::Restore, "glusterfs", transport,
                                     None, Some(*mode)).unwrap();
                assert_eq!(reverse.output_lvm_mode, Some(*mode));
                assert!(reverse.required_preflight.iter().any(|x|x.contains("empty isolated")));
                assert!(!reverse.executable);
            }
        }
    }
    #[test]
    fn lvm_layout_cannot_be_misapplied_to_other_plugins_or_nvmeof() {
        assert!(plan_lvm(Mode::Backup,"glusterfs","posix",Some(LvmMode::Local),None).is_err());
        assert!(plan_lvm(Mode::Backup,"nvmeof","posix",Some(LvmMode::ClusterShared),None).is_err());
        assert!(plan_lvm(Mode::Backup,"iscsi","glusterfs",None,Some(LvmMode::Local)).is_err());
        assert!(LvmMode::parse("lvm-shared").is_err());
        assert_eq!(LvmMode::parse("cluster-local").unwrap(),LvmMode::ClusterLocal);
    }
    #[test]
    fn clustered_shared_lvm_requires_fencing_and_distributed_locking_preflight() {
        let plan=plan_lvm(Mode::Backup,"fc","anti-ransomware-vault",
                          Some(LvmMode::ClusterShared),None).unwrap();
        assert!(plan.required_preflight.iter().any(|x|x.contains("distributed LVM lock")));
        assert!(plan.required_preflight.iter().any(|x|x.contains("quorum")));
        assert!(!plan.executable);
        assert!(plan.confirmation_required_before_writes);
    }
    #[test]
    fn distributed_files_and_repositories_have_bidirectional_declared_roles() {
        for id in ["lustre","gpfs","hdfs","afs","alfresco",
                   "google-drive","onedrive","owncloud","nextcloud"] {
            let d = find(id).unwrap();
            // GPFS CES has a strictly read-only mounted-source preview;
            // output and all other providers remain declarative only.
            assert_eq!(d.backup_input, if id=="gpfs" {Stage::Preview} else {Stage::Planned}, "{id}");
            assert_eq!(d.backup_output, Stage::Planned, "{id}");
            assert_eq!(d.restore_input, Stage::Planned, "{id}");
            assert_eq!(d.restore_output, if id=="gpfs" {Stage::Preview} else {Stage::Planned}, "{id}");
            assert!(!plan(Mode::Backup, id, "anti-ransomware-vault").unwrap().executable);
            assert!(!plan(Mode::Restore, "anti-ransomware-vault", id).unwrap().executable);
        }
        for id in ["lustre","gpfs","hdfs","afs"] {
            assert_eq!(find(id).unwrap().kind, DataKind::DistributedFilesystem);
        }
        assert_eq!(find("alfresco").unwrap().kind, DataKind::DocumentRepository);
    }
    #[test]
    fn license_and_oauth_gates_do_not_grant_execution() {
        assert_eq!(find("gpfs").unwrap().access_gate, AccessGate::ExternalProtocolOrMountedClient);
        let p = plan(Mode::Backup, "gpfs", "posix").unwrap();
        assert!(p.required_preflight.iter().any(|text| text.contains("GPFS Client license")));
        assert!(!p.executable);
        for id in ["google-drive","onedrive"] {
            assert_eq!(find(id).unwrap().access_gate, AccessGate::OAuthScopeReview);
            let p = plan(Mode::Backup, "posix", id).unwrap();
            assert!(p.required_preflight.iter().any(|text| text.contains("OAuth")));
            assert!(!p.executable);
        }
    }
    #[test]
    fn afs_backup_and_restore_validate_cell_volume_chain_and_target() {
        let p = plan(Mode::Backup,"afs","anti-ransomware-vault").unwrap();
        assert!(p.required_preflight.iter().any(|text| text.contains("AFS cell")));
        assert!(p.required_preflight.iter().any(|text| text.contains("dump chain")));
        assert!(!p.executable);
        let restore = plan(Mode::Restore,"anti-ransomware-vault","afs").unwrap();
        assert!(restore.required_preflight.iter().any(|text| text.contains("fresh non-existing AFS volume")));
        assert!(!restore.executable);
        assert!(find("afs").unwrap().lvm_modes.is_empty());
        assert!(plan_lvm(Mode::Backup,"afs","posix",Some(LvmMode::Local),None).is_err());
    }
    #[test]
    fn hdfs_and_other_remote_repositories_never_execute_unverified() {
        for id in ["hdfs", "alfresco", "owncloud", "nextcloud"] {
            let p = plan(Mode::Backup,"posix",id).unwrap();
            assert!(p.confirmation_required_before_writes);
            assert!(!p.executable);
        }
        let p = plan(Mode::Backup,"posix","hdfs").unwrap();
        assert!(p.required_preflight.iter().any(|text| text.contains("manifest last")));
    }
    #[test]
    fn gpfs_uses_open_ces_protocol_client_without_native_driver() {
        let d=find("gpfs").unwrap();
        assert_eq!(d.access_gate,AccessGate::ExternalProtocolOrMountedClient);
        assert_eq!(d.gpfs_access,GPFS_CES_ACCESS);
        for protocol in GPFS_CES_ACCESS {
            let b=plan_profiles(Mode::Backup,"gpfs","sshfs",None,None,Some(*protocol),None).unwrap();
            assert!(!b.executable);
            let r=plan_profiles(Mode::Restore,"sshfs","gpfs",None,None,None,Some(*protocol)).unwrap();
            assert!(r.required_preflight.iter().any(|x|x.contains("Client license")));
            assert!(r.required_preflight.iter().any(|x|x.contains("manifest-last")));
        }
        assert!(GpfsAccess::parse("native").is_err());
        assert!(plan_profiles(Mode::Backup,"sshfs","posix",None,None,
                              Some(GpfsAccess::CesNfs),None).is_err());
    }
    #[test]
    fn sshfs_source_and_target_are_planned_distinct_from_sftp() {
        let d=find("sshfs").unwrap();
        assert_eq!(d.kind,DataKind::Files);
        assert_eq!(d.backup_input,Stage::Preview);
        assert_eq!(d.backup_output,Stage::Planned);
        assert_eq!(d.restore_input,Stage::Planned);
        assert_eq!(d.restore_output,Stage::Preview);
        assert_ne!(d.id,find("ssh").unwrap().id);
        let b=plan(Mode::Backup,"sshfs","anti-ransomware-vault").unwrap();
        assert!(b.required_preflight.iter().any(|x|x.contains("SSH host key")));
        assert!(!b.executable);
        let r=plan(Mode::Restore,"anti-ransomware-vault","sshfs").unwrap();
        assert!(r.required_preflight.iter().any(|x|x.contains("manifest-last")));
        assert!(!r.executable);
    }
    #[test]
    fn oracle_rman_is_database_input_not_generic_output() {
        let d=find("oracle").unwrap();
        assert_eq!(d.kind,DataKind::Database);
        assert_eq!(d.backup_input,Stage::Planned);
        assert_eq!(d.backup_output,Stage::Unsupported);
        assert_eq!(d.restore_output,Stage::Planned);
        assert_eq!(d.restore_input,Stage::Unsupported);
        assert!(!plan(Mode::Backup,"oracle","anti-ransomware-vault").unwrap().executable);
        assert!(plan(Mode::Restore,"anti-ransomware-vault","oracle").is_ok());
        assert!(plan(Mode::Backup,"posix","oracle").is_err());
    }
    #[test]
    fn plan_mode_is_not_silently_defaulted() {
        assert!(Mode::parse("Backup").is_err());
        assert_eq!(Mode::parse("backup").unwrap(), Mode::Backup);
        assert_eq!(Mode::parse("restore").unwrap(), Mode::Restore);
    }
}
