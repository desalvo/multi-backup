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
    Experimental,
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
    /// Narrow Ready promise: empty for any unqualified provider. No general provider certification.
    pub ready_scope: Option<&'static str>,
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
            ready_scope: None,
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
    const fn with_ready_scope(mut self, scope:&'static str) -> Self {
        self.ready_scope=Some(scope);
        self
    }
    const fn with_access_gate(mut self, gate: AccessGate) -> Self {
        self.access_gate = gate;
        self
    }
    /// A source adapter does not automatically imply a destination restore.
    const fn with_restore_output_stage(mut self, stage:Stage)->Self{
        self.restore_output=stage;
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
const READY: Stage = Stage::Ready;
const EXPERIMENTAL: Stage = Stage::Experimental;
pub const CATALOG: &[PluginDescriptor] = &[
    PluginDescriptor::new("github", "GitHub", DataKind::SourceControl, PREVIEW, NONE,
        "Git mirror+LFS and remote metadata checkpoint", "Repository/settings/Actions; remote restore creates new repositories only"),
    PluginDescriptor::new("dropbox", "Dropbox", DataKind::Files, PREVIEW, PREVIEW,
        "File ID+revision and content hash",
        "B-In read by file-id/revision and hash; dedicated Rust B-Out accepts only verified age archives, create-only upload sessions, no auto-rename or overwrite, then confirms independent remote metadata id/rev/hash; remote restore still needs an end-to-end qualification"),
    PluginDescriptor::new("icloud", "Apple iCloud Drive (macOS coordinated local files)", DataKind::Files, PREVIEW, PREVIEW,
        "User-authorized macOS iCloud Drive local File Provider path; NSFileCoordinator one regular file and ciphertext-only create-only destination",
        "B-In native macOS helper stages one coordinated readable file then age encrypts locally; B-Out copies only previously verified .age ciphertext into a NEW iCloud Drive file, coordinating with File Provider and checking local byte digest. Requires signed/macOS user filesystem permissions and iCloud sync online for remote durability. NO CloudKit access to entire iCloud Drive; no Apple ID login, remote API, cross-device sync receipt, folder hierarchy parity, ACL/metadata or server-side unattended non-macOS access; not production READY"),
    PluginDescriptor::new("google-drive", "Google Drive", DataKind::Files, PLAN, PLAN,
        "File ID+revision/export format and shared-drive inventory",
        "OAuth-scoped Drive app folder and Google-native export; upload encrypted objects to a new destination with no overwrite").with_access_gate(AccessGate::OAuthScopeReview),
    PluginDescriptor::new("onedrive", "OneDrive / SharePoint drive", DataKind::Files, PLAN, PLAN,
        "Microsoft Graph driveItem+eTag/delta token",
        "Graph OAuth/tenant consent, upload-session retry and explicit conflict handling for new encrypted objects").with_access_gate(AccessGate::OAuthScopeReview),
    PluginDescriptor::new("posix", "Local POSIX regular-file data (content-only)", DataKind::Files, READY, READY,
        "Quiesced source tree; file-content-only, no metadata/ACL/xattr/symlink fidelity",
        "READY ONLY for a quiesced local tree of ordinary readable files, including nested files, age ciphertext, parity repair and new-directory content restore. Deliberately refuses symlink/special source files; empty directories, file modes, timestamps, ownership, ACLs, xattrs, hardlink identities and live application consistency are NOT captured. Not a general-purpose complete POSIX filesystem backup")
        .with_ready_scope("content_only_quiesced_regular_files; no ACL/xattr/owner/mode/timestamps/hardlinks/symlinks/empty dirs"),
    PluginDescriptor::new("s3", "S3 single-object import (MinIO-verified)", DataKind::ObjectStore, READY, PREVIEW,
        "Single existing version-pinned object + independently attested byte length/SHA256, encrypted immutable import",
        "READY only for ONE MinIO-tested immutable version-pinned S3 object read into age chunk store, digest+size verified before commit. B-Out remains Preview; no generic restore-to-S3, bulk object listing, object ACL/metadata, multi-version backups or claims for untested S3 vendors")
        .with_restore_output_stage(PLAN)
        .with_ready_scope("minio_version_pinned_single_object_sha256_age_import; no batch/ACL/metadata/restore-to-s3"),
    PluginDescriptor::new("webdav", "WebDAV authenticated HTTPS archive + frozen davfs2 source", DataKind::Files, READY, READY,
        "Authenticated HTTPS WebDAV mounted RO quiesced source; create-only ciphertext PUT, independent remote GET and decrypted NEW local recovery",
        "READY ONLY: attested quiescent davfs2 RO mount regular-file bytes and native HTTPS Basic Auth conditional PUT encrypted archive + independent GET SHA256 + NEW local restore. NOT general plaintext remote restore, ETag/ACL/xattr fidelity, resumable large files, FUSE durability or production snapshot consistency")
        .with_restore_output_stage(PLAN)
        .with_ready_scope("webdav_authenticated_ro_davfs2_bytes_create_only_https_ciphertext_put_get; no plaintext-dav-restore/ACL/ETag/resumable"),
    PluginDescriptor::new("nfs", "NFSv3 genuine kernel mount byte-content", DataKind::Files, READY, READY,
        "Authorized exact read-only NFSv3 client with external frozen source, complete ordinary-file bytes and incremental Rust age chunks",
        "READY ONLY NFSv3 localhost real nfsd exported share with independently attested RO mount and quiescent regular-file FULL/INC, integrity check, create-only NEW RW mounted restore, ciphertext-only B-Out and independent age readback. NOT NFSv4/krb5, server snapshot, ACL/xattr/owner/mode fidelity, multi-host HA/fencing, production network resilience")
        .with_ready_scope("nfsv3_kernel_export_attested_frozen_regular_file_full_inc_new_restore_ciphertext_bout; no nfs4/kerberos/ACL/xattr/ha"),
    PluginDescriptor::new("smb", "Samba SMB3 authenticated mounted byte-content", DataKind::Files, READY, READY,
        "Actual Samba SMB3 local daemon and authenticated kernel CIFS RO/RW mounts, stable ordinary-file bytes, age and new-path recovery",
        "READY ONLY authenticated SMB3.0 real CIFS client source, frozen ordinary-file bytes FULL/INC, age-integrity, NEW RW restore and encrypted ciphertext archive B-Out readback. NOT Windows ADS/ACL, durable handle/snapshot, NTFS ACL/owner, remote multipath, production HA or multi-SMB dialect interoperability")
        .with_ready_scope("samba_smb3_authenticated_cifs_attested_regular_file_full_inc_new_restore_ciphertext_bout; no ADS/ACL/snapshots/ha"),
    PluginDescriptor::new("cephfs", "CephFS regular-file content (genuine Rook client)", DataKind::Files, READY, PREVIEW,
        "Attested read-only CephFS snapshot/mount, only ordinary file content to age chunks and create-only directory recovery",
        "READY limited to bytes of regular files on an exact verified CephFS mount with independent frozen snapshot attestation; encrypted restore into a NEW directory. Mount-based encrypted B-Out remains Preview. No automatic MDS snapshot barrier, ACL/xattr/uid/gid/mode/empty directory fidelity through generic executor, physical fault/fencing or coordinated multi-FS consistency")
        .with_restore_output_stage(READY)
        .with_ready_scope("cephfs_snapshot_attested_mounted_regular_file_content_create_only; no atomic-multi-fs/metadata/physical-fault"),
    PluginDescriptor::new("glusterfs", "GlusterFS live daemon and FUSE regular bytes", DataKind::Files, READY, READY,
        "Actual GlusterFS server with single disposable volume, verified FUSE RO/RW client, frozen FULL/INC and age new-path recovery",
        "READY ONLY real single-node GlusterFS volume with authenticated/authorized FUSE mounts, quiesced regular-file FULL/INC, create-only isolated restore and encrypted ciphertext B-Out with age readback. NOT GFID topology recreation, rebalance, distributed multi-node replicas, ACL/xattr/owner/mode fidelity, quorum, brick failure or HA")
        .with_ready_scope("glusterfs_single_node_real_fuse_mount_attested_regular_file_full_inc_restore_ciphertext_bout; no GFID/multi-node/ACL/xattr/HA"),
    PluginDescriptor::new("lustre", "Lustre real Rocky9 MDT/OST regular-file bytes", DataKind::DistributedFilesystem, READY, READY,
        "Real genuine KVM Lustre MDT and OST quiesced file-content full to age; create-only restore and ciphertext archive mounted B-Out",
        "READY only: stable regular-file bytes on authorized real QEMU Lustre MDT/OST client, age encrypted FULL, byte-identical NEW path restore and create-only mounted ciphertext B-Out + age readback. NO coordinated online MDT/OST atomic snapshot, full UID/GID/ACL/xattr/mtime, stripe recreation, physical host failure, HA/fencing")
        .with_ready_scope("lustre_qemu_mdt_ost_quiesced_regular_file_bytes_new_restore_ciphertext_archive; no ACL/xattr/stripes/online-atomic-HA"),
    PluginDescriptor::new("gpfs", "IBM Storage Scale / GPFS via NFS-SMB (PROTOCOL PREVIEW)", DataKind::DistributedFilesystem, PREVIEW, PREVIEW,
        "Client protocol compatibility with NFSv3/v4 or SMB3 mount; source consistency proved separately",
        "Optional RC NFS/SMB protocol client preview only. Generic NFS and Samba tests do NOT certify IBM GPFS native ACLs, snapshot, CES failover or IBM filesystem metadata. Mounted CES-protocol NFS/SMB encrypted create-only B-Out preview; real IBM CES field qualification separately deferred.")
        .with_gpfs_ces(),
    PluginDescriptor::new("hdfs", "Hadoop HDFS", DataKind::DistributedFilesystem, PLAN, PLAN,
        "SWebHDFS/WebHDFS, HDFS snapshot/directory metadata, ACL/xattrs, checksums",
        "Kerberos/delegation token, TLS and redirect allowlist; write only new paths, commit encrypted manifest last"),
    PluginDescriptor::new("afs", "Andrew File System / OpenAFS", DataKind::DistributedFilesystem, PREVIEW, PREVIEW,
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
    PluginDescriptor::new("rbd", "Ceph RBD pinned full snapshot (genuine 3 OSD)", DataKind::BlockDevice, READY, NONE,
        "Exact numeric RBD snapshot ID + independent size/SHA256, encrypted full export and NEW RBD image readback",
        "READY for one complete immutable RBD snapshot export using actual rbd, authenticated age store and CREATE-ONLY different RBD image verified by independent block readback. Backup into RBD is unsupported; no online VM/CT quiesce, CBT/diff, incremental chains, mirroring, namespaces or production fencing")
        .with_restore_output_stage(READY)
        .with_ready_scope("ceph_rbd_pinned_snapshot_full_image_new_image_readback; no guest-consistency/diff/fencing"),
    PluginDescriptor::new("iscsi", "iSCSI LUN (loopback TGT isolated)", DataKind::BlockTransport, READY, PLAN,
        "Pinned read-only kernel IQN/LUN serial and SHA256 -> age archive -> NEW isolated local TGT LUN",
        "READY only on dedicated Linux lab TGT + initiator: pinned source RO LUN, kernel IQN/LUN WWID/bytes/SHA256, age backup, create-new isolated local TGT target+LUN with byte-for-byte independent readback. Backup direct to remote LUN remains Planned. No arbitrary SAN B-Out, CHAP/multipath, virtual machine quiesce, distributed LVM or physical fencing")
        .with_san_lvm().with_restore_output_stage(READY)
        .with_ready_scope("linux_tgt_loopback_readonly_pinned_snapshot_new_lun_readback; no remote-san/multipath/fencing"),
    PluginDescriptor::new("fc", "Fibre Channel LUN", DataKind::BlockTransport, PLAN, PLAN,
        "FC WWPN, SCSI WWID, multipath and snapshot epoch",
        "Respect multipath aliases, WWID and ownership; raw FC LUN or LVM local/clustered/shared, with fencing/locking and snapshot consistency").with_san_lvm(),
    PluginDescriptor::new("nvmeof", "NVMe over Fabrics (NVMe-oF)", DataKind::BlockTransport, PLAN, PLAN,
        "NVMe NQN, NSID, EUI/NGUID and snapshot epoch",
        "Distinguish transport TCP/RDMA/FC, namespace and multipath; output/restore may write only an explicitly created isolated namespace"),
    // Keep the historical "proxmox" ID for backward compatibility with
    // saved jobs, but never conflate a PVE hypervisor with a PBS datastore.
    PluginDescriptor::new("proxmox", "Proxmox Virtual Environment (PVE)", DataKind::Virtualization, PREVIEW, PLAN,
        "QEMU VM and native LXC CT inventory with explicit snapshot/suspend/stop modes",
        "PVE B-In preview uses real completed UPID status, TLS media list, node-local pvesm path and independently pinned archive bytes/SHA256 before age backup; offline create-only recovery. Full VM/CT restore, B-Out guest import and field certification remain planned").with_restore_output_stage(PLAN),
    PluginDescriptor::new("pbs", "Proxmox Backup Server (PBS)", DataKind::Virtualization, PREVIEW, PLAN,
        "Distinct PBS datastore and snapshot groups for VM/CT/host backup archives",
        "PBS B-In preview streams one signed/verified VM image through official proxmox-backup-client after validated snapshot file inventory, fingerprint/CA TLS, independent SHA256 and age encryption. Only offline image recovery; PBS write-back, CT pxar, live provider restore and field E2E remain planned").with_restore_output_stage(PLAN),
    PluginDescriptor::new("kubernetes", "Kubernetes / OpenShift / OKD", DataKind::Cluster, PLAN, NONE,
        "API object resourceVersion, CSI VolumeSnapshots and etcd/CRD scope", "Never capture Secrets without explicit policy; cluster restore requires name remapping"),
    PluginDescriptor::new("vmware", "VMware vSphere", DataKind::Virtualization, PREVIEW, NONE,
        "Offline vSphere OVF+VMDK full export through verified govmomi/govc into age-encrypted chunks",
        "Compiled VMware OVF offline B-In requires powered-off exact MoRef and owner-only export principal; separate import principal creates a NEW powered-off VM on named datastore, with TLS verified, no guest autostart and strict no-overwrite. Real vCenter/ESXi field lab, guest boot/data verification, online backup VADP/CBT, hot snapshot and production certification NOT DONE; CT disabled; NO guest-agent/container-runtime CT fallback.").with_restore_output_stage(PREVIEW),
    PluginDescriptor::new("ovirt", "oVirt / RHV", DataKind::Virtualization, PREVIEW, NONE,
        "VM-only Engine API READY backup and versioned imageio disk download", "Native read-only IMAGEIO VM disk B-In needs independently preissued READY Engine backup and matching transfer URL, VM/disk UUIDs, SHA256+byte count and trusted HTTPS; records one QCOW2 disk in age store, with offline create-only recovery. NO native VM restore, no multi-disk/OVF completeness, no VM create/start, CT disabled (oVirt has no native supported CT). NO guest-agent/container-runtime CT fallback"),
    PluginDescriptor::new("mysql", "MariaDB 11.4 / MySQL 8.4 single-DB FULL logical", DataKind::Database, READY, NONE,
        "One InnoDB database FULL single-transaction dump streamed into age store and restored into NEW target",
        "READY for MariaDB 11.4 or Oracle MySQL 8.4 single-database transactional logical FULL dump/create-only new database recovery with binary+row verification and wrong credentials/overwrite denial. No MySQL GTID/binlog PITR, global users/grants, coordinated cluster Galera restore, concurrent DDL consistency or cross-version portability")
        .with_ready_scope("mariadb11_mysql84_single_database_full_innodb_create_only; no GTID/PITR/globals/online-DDL"),
    PluginDescriptor::new("postgresql", "PostgreSQL 16 (single DB FULL logical Ready scope)", DataKind::Database, READY, NONE,
        "One PostgreSQL 16 database FULL pg_dump custom format and create-only pg_restore; never all-cluster readiness",
        "READY ONLY for an isolated PostgreSQL 16 single-database, logical FULL dump and encrypted age roundtrip to a NEW database, with nonzero exit/credentials/overwrite denied. No --owner/--acl; no cluster roles, globals, tablespaces permissions, PITR under Patroni, online DDL, all-cluster recovery, cross-version portability, or three-host physical HA. Separate PostgreSQL WAL/PITR integration is not certified by this scoped Ready status")
        .with_ready_scope("postgresql16_single_database_full_logical_create_only; no roles/owners/ACLs/globals/cluster-PITR/online-DDL"),
    PluginDescriptor::new("oracle", "Oracle Free RMAN media preservation (not live DB restore)", DataKind::Database, READY, NONE,
        "Pre-generated nonempty RMAN .bkp piece files -> age-encrypted archive -> new local RMAN piece directory",
        "READY only for complete, validated, ordinary .bkp media piece files: age encrypt, verify and create-new local media restore with exact file bytes. Real Oracle Free RMAN RESTORE VALIDATE and separate PDB/CDB/SCN-PITR lab exercised, but native provider does NOT orchestrate Oracle DB restore/recovery. No RAC, ASM, TDE, online DB PITR orchestration, proprietary binaries or production claim")
        .with_restore_output_stage(READY)
        .with_ready_scope("oracle_rman_media_pieces_new_directory_age_verified; not database-restore/RAC/ASM/TDE"),
    PluginDescriptor::new("ssh", "OpenSSH pinned single remote file import", DataKind::RemoteShell, READY, PLAN,
        "Trusted known_hosts, expected one remote file SHA256/length, authenticated encrypted Rust B-In",
        "READY solely for pinned SSH known_hosts single remote file backed by independent byte length/SHA256 and age-encrypted stream, verified NEW local archive restore. SCP/SFTP transport smoke real, but NO recursive source, SFTP directory B-In, arbitrary remote shell commands, remote B-Out/restore or application-consistent source")
        .with_restore_output_stage(PLAN)
        .with_ready_scope("openssh_known_hosts_one_pinned_remote_file_sha256_age_import; no recursive-sftp/remote-write/application-snapshot"),
    PluginDescriptor::new("sshfs", "SSHFS exact mounted RO/RW bytes and encrypted archive", DataKind::Files, READY, READY,
        "Pinned SSH host key, exact FUSE SSHFS RO/RW mount, age chunks, create-only NEW remote directory and ciphertext B-Out",
        "READY only on authenticated SSHFS real FUSE source with exact verified mount, RO quiescence attest and regular-file bytes; age backup, NEW RW SSHFS restore, create-only encrypted archive and independent remote readback. No ACL/xattr/owner/mode fidelity, server-side snapshots, symlink traversal, remote host compromise protection or SFTP recursive native adapter")
        .with_ready_scope("sshfs_fuse_attested_regular_file_bytes_new_rw_restore_ciphertext_bout; no ACL/xattr/remote-snapshot/root-immunity"),
    PluginDescriptor::new("anti-ransomware-vault", "S3 WORM vault (MinIO Object Lock scope)",
        DataKind::ProtectedVault, NONE, READY,
        "Independent provider-enforced S3 COMPLIANCE retention, immutable version IDs, writer/recovery credentials, age restore",
        "READY ONLY for genuine MinIO versioned COMPLIANCE Object Lock ciphertext ingest and protected manifest, pinned version+age recovery with two separate nonadmin IAM principals in real lab. Local append-only vault is NOT equivalent to WORM; root/host compromise, provider operator, retention policy bypass, physical/geo independence and other S3 vendors are NOT certified. Never claim ransomware immunity")
        .with_ready_scope("minio_s3_object_lock_compliance_ciphertext_versioned_recovery; no root-immune/physical-DR"),
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
        if plugin.id == "postgresql" || plugin.id == "mysql" {
            required_preflight.push("require transaction-consistent FULL dump with verified role privileges; native client version must match server compatibility");
            required_preflight.push("require private credential file, verified TLS for remote databases, and destination NEW database that does not yet exist");
            required_preflight.push("logical full backup only; WAL/binlog PITR, application quiescing and physical incremental backups are not certified");
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
            required_preflight.push("PROTOCOL PREVIEW: test GPFS via NFS/SMB clients; IBM CES/GPFS native behavior still unqualified");
            required_preflight.push("verify operator-authorized NFS/SMB network export (IBM CES when actually using GPFS); NFS/SMB clients require no GPFS Client license");
            required_preflight.push("verify an existing authenticated NFSv3/v4 or CIFS/SMB3 mount provided by OS open-source tools");
            required_preflight.push("prove source snapshot or quiescent dataset consistency out of band; preserve only tested NFS/SMB metadata; IBM ACL/ILM/fileset semantics require IBM CES tests");
            if gpfs_mode.is_none() {
                required_preflight.push("explicitly select GPFS access ces-nfs or ces-smb; no proprietary native client fallback");
            }
            if writing {
                required_preflight.push("verify exact writable NFS/SMB mount, create-only new target, verified backup decryption, and no overwrite; IBM CES write semantics not separately certified");
            }
        }
        if plugin.id=="proxmox" {
            required_preflight.push("require genuine PVE snapshot/suspend/stop task completion PLUS verified new backup media; task UPID alone never proves backup success");
            if writing {required_preflight.push("B-Out PVE target is planning-only: require approved storage configured for backup, create-only native backup reference, verified age ciphertext and offline restore proof; no overwrite of VM/CT");}
        }
        if plugin.id=="rbd" {
            required_preflight.push("require Ceph RBD immutable snapshot ID, pool, image name, feature compatibility and client ceph auth attestation; no write to live image");
            if writing {required_preflight.push("RBD output planning-only: create an isolated NEW unmapped image from verified encrypted stream, require human approval and copy-on-write fencing");}
        }
        if plugin.id=="ssh" {
            required_preflight.push("SSH B-In requires 0600 pinned known_hosts and private key, safe absolute file path, independently supplied SHA256 and bytes, and successful authenticated SSH exit before manifest commit");
            if writing {required_preflight.push("Direct SSH/SCP/SFTP B-Out not implemented; no unsafe shell or overwrite fallback");}
        }
        if plugin.id == "s3" {
            required_preflight.push("S3 source B-In requires exact immutable version ID and independent raw SHA-256 reference; no mutable latest-object fallback");
            required_preflight.push("S3 source must pass authenticated HTTPS, exact returned version and size; after streaming chunks verify SHA before publishing encrypted manifest");
            if writing {required_preflight.push("S3 WORM output requires independent versioning and COMPLIANCE retention; generic restore-to-S3 is not implemented");}
        }
        if plugin.id == "dropbox" {
            required_preflight.push("Dropbox B-Out requires scoped files.content.write and files.metadata.read; upload only age ciphertext to a NEW destination without overwrite or autorename");
            required_preflight.push("Verify returned object ID, revision, bytes and Dropbox block content_hash against independent GET metadata; retry ambiguous commits only after operator reconciliation");
        }
        if plugin.id == "webdav" {
            required_preflight.push("require exact mounted FUSE davfs2 read-only client, trusted HTTPS server certificate, authentication and out-of-band quiescent snapshot attestation");
            required_preflight.push("Mounted WebDAV encrypted B-Out is preview-only; native remote DAV metadata ETag/ACL and authenticated RW field acceptance are still pending");
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
    // A generic pair is executable only when BOTH role stages are Ready AND
    // its actual compiled dispatcher is independently allowlisted below.
    // The sole Ready scope in r214 is quiesced regular-file content POSIX->POSIX;
    // native/provider-specific Preview commands never become generic jobs.
    // Reflect the ACTUAL compiled generic dispatcher, not catalog roles.
    // AFS uses separate typed volume CLI commands and cannot be generalized
    // as a mounted-file backup or a POSIX directory restore.
    let preview_executable = match mode {
        Mode::Backup => destination.id == "posix" && matches!(source.id,
            "posix" | "nfs" | "smb" | "gpfs" | "sshfs" | "webdav" |
            "cephfs" | "glusterfs" | "lustre"),
        Mode::Restore => source.id == "posix" && matches!(destination.id,
            "posix" | "nfs" | "smb" | "gpfs" | "sshfs" | "cephfs" | "glusterfs" | "lustre"),
    };
    let executable = source_stage == Stage::Ready && destination_stage == Stage::Ready
        && preview_executable && source.id == "posix" && destination.id == "posix";
    Ok(Plan {
        mode, source_plugin: source.id, destination_plugin: destination.id,
        source_stage, destination_stage, executable, preview_executable:preview_executable && !executable,
        reason: if executable { "READY in explicitly narrow quiesced regular-file content-only scope; POSIX metadata/ACL/xattr and live snapshots excluded" } else if source_stage == Stage::Experimental || destination_stage == Stage::Experimental {
            "EXPERIMENTAL: optional provider, external IBM field qualification pending; never a default RC blocker"
        } else if preview_executable {
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
    fn ready_scope_must_be_explicit_and_only_posix_content_roundtrip_can_execute(){
        for plugin in CATALOG{
            let any_ready=[plugin.backup_input,plugin.backup_output,
                plugin.restore_input,plugin.restore_output]
                .iter().any(|s|*s==Stage::Ready);
            assert_eq!(any_ready,plugin.ready_scope.is_some(),
                "READY must have an explicit narrow scope: {}",plugin.id);
            if any_ready {assert!(matches!(plugin.id,
                "posix"|"s3"|"cephfs"|"rbd"|"iscsi"|"mysql"|
                "postgresql"|"oracle"|"anti-ransomware-vault"|
                "webdav"|"ssh"|"sshfs"|"lustre"|"nfs"|"smb"|"glusterfs"));}
        }
        let p=plan(Mode::Backup,"posix","posix").unwrap();
        assert_eq!(p.source_stage,Stage::Ready);
        assert_eq!(p.destination_stage,Stage::Ready);
        assert!(p.executable && !p.preview_executable);
        let restore=plan(Mode::Restore,"posix","posix").unwrap();
        assert!(restore.executable && !restore.preview_executable);
        let postgres=find("postgresql").unwrap();
        assert_eq!(postgres.backup_input,Stage::Ready);
        assert_eq!(postgres.restore_output,Stage::Ready);
        assert_eq!(postgres.backup_output,Stage::Unsupported);
        assert!(postgres.ready_scope.unwrap().contains("postgresql16_single_database"));
        assert!(!plan(Mode::Backup,"postgresql","posix").unwrap().executable,
            "Dedicated SQL CLI must not become a generic filesystem copier");
        assert!(p.reason.contains("metadata/ACL/xattr"));
        for provider in ["nfs","smb","cephfs","glusterfs","lustre","gpfs",
            "sshfs","webdav","s3"] {
            let plan=plan(Mode::Backup,provider,"posix").unwrap();
            assert!(!plan.executable,"provider {provider} cannot inherit POSIX READY");
        }
    }

    #[test]
    fn r217_scoped_ready_role_matrix_excludes_unsupported_vendor_write_operations(){
        let roles=[
            ("posix", [Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("s3", [Stage::Ready,Stage::Preview,Stage::Preview,Stage::Planned]),
            ("cephfs", [Stage::Ready,Stage::Preview,Stage::Preview,Stage::Ready]),
            ("rbd", [Stage::Ready,Stage::Unsupported,Stage::Unsupported,Stage::Ready]),
            ("iscsi", [Stage::Ready,Stage::Planned,Stage::Planned,Stage::Ready]),
            ("mysql", [Stage::Ready,Stage::Unsupported,Stage::Unsupported,Stage::Ready]),
            ("postgresql", [Stage::Ready,Stage::Unsupported,Stage::Unsupported,Stage::Ready]),
            ("oracle", [Stage::Ready,Stage::Unsupported,Stage::Unsupported,Stage::Ready]),
            ("anti-ransomware-vault", [Stage::Unsupported,Stage::Ready,Stage::Ready,Stage::Unsupported]),
            ("webdav",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Planned]),
            ("ssh",[Stage::Ready,Stage::Planned,Stage::Planned,Stage::Planned]),
            ("sshfs",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("lustre",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("nfs",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("smb",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("glusterfs",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
        ];
        let mut actual=0;
        for entry in CATALOG {
            let observed=[entry.backup_input,entry.backup_output,entry.restore_input,entry.restore_output];
            let should=roles.iter().find(|(id,_)|entry.id==*id);
            if let Some((_,expect))=should {
                actual+=1;
                assert_eq!(observed,*expect,"Unexpected READY escalation for {}",entry.id);
                assert!(entry.ready_scope.is_some());
            }else{
                assert!(entry.ready_scope.is_none(),"False READY for {}",entry.id);
                assert!(!observed.contains(&Stage::Ready),"Unqualified READY for {}",entry.id);
            }
        }
        assert_eq!(actual,roles.len());
        for provider in ["mysql","oracle","iscsi","rbd","s3","cephfs","anti-ransomware-vault","webdav","ssh","sshfs","lustre"] {
            let p=find(provider).unwrap();
            assert!(p.ready_scope.unwrap().contains(';'),"Scope needs exclusions: {provider}");
            // Dedicated provider commands are never mistaken for POSIX dispatcher.
            let supported=if p.backup_input.listed(){
                plan(Mode::Backup,provider,"posix").ok()
            }else{plan(Mode::Backup,"posix",provider).ok()};
            assert!(supported.is_some(),"Missing capability plan for {provider}");
            assert!(!supported.unwrap().executable,"No uncompiled generic adapter for {provider}");
        }
    }
    #[test]
    fn r216_transport_fields_match_only_the_real_lab_paths(){
        for (id,expected) in [
            ("webdav",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Planned]),
            ("ssh",[Stage::Ready,Stage::Planned,Stage::Planned,Stage::Planned]),
            ("sshfs",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready]),
            ("lustre",[Stage::Ready,Stage::Ready,Stage::Ready,Stage::Ready])
        ] {
            let d=find(id).unwrap();
            assert_eq!([d.backup_input,d.backup_output,d.restore_input,d.restore_output],expected);
            assert!(d.ready_scope.unwrap().contains("; no "));
            assert!(!plan(Mode::Backup,id,"posix").unwrap().executable);
        }
        assert_eq!(CATALOG.iter().filter(|p|p.ready_scope.is_some()).count(),16);
        assert!(plan(Mode::Restore,"posix","webdav").unwrap().executable==false);
    }
    #[test]
    fn r217_nfs_smb_gluster_qualified_single_server_bytes_not_false_gpfs_native() {
        for id in ["nfs","smb","glusterfs"]{
            let d=find(id).unwrap();
            let roles=[d.backup_input,d.backup_output,d.restore_input,d.restore_output];
            assert_eq!(roles,[Stage::Ready;4]);
            assert!(d.ready_scope.unwrap().contains("no "));
            assert!(!plan(Mode::Backup,id,"posix").unwrap().executable);
            assert!(!plan(Mode::Restore,"posix",id).unwrap().executable);
            assert!(plan(Mode::Backup,id,"posix").unwrap().preview_executable);
        }
        assert_eq!(find("gpfs").unwrap().backup_input,Stage::Preview);
        assert_eq!(find("gpfs").unwrap().backup_output,Stage::Preview);
        assert!(find("gpfs").unwrap().ready_scope.is_none());
    }
    #[test]
    fn all_33_requested_input_plugins_and_26_outputs_are_listed() {
        let inputs = ["github", "dropbox", "icloud", "google-drive", "onedrive", "posix",
            "s3", "webdav", "nfs", "smb", "cephfs", "rbd", "proxmox", "pbs",
            "kubernetes", "vmware", "ovirt", "mysql", "postgresql", "ssh",
            "iscsi", "fc", "nvmeof", "glusterfs", "lustre", "gpfs",
            "hdfs", "afs", "alfresco", "owncloud", "nextcloud", "sshfs",
            "oracle"];
        let outputs = ["posix", "dropbox", "icloud", "s3", "webdav", "cephfs", "nfs", "ssh", "pbs",
            "iscsi", "fc", "nvmeof", "anti-ransomware-vault", "glusterfs",
            "lustre", "gpfs", "hdfs", "afs", "alfresco", "google-drive", "onedrive",
            "owncloud", "nextcloud", "sshfs", "smb", "proxmox"];
        assert_eq!(catalog_for(Mode::Backup, true).len(), 33);
        assert_eq!(catalog_for(Mode::Backup, false).len(), 26);
        assert_eq!(CATALOG.len(), 34);
        for id in inputs { assert!(find(id).unwrap().backup_input.listed(), "{id}"); }
        for id in outputs { assert!(find(id).unwrap().backup_output.listed(), "{id}"); }
        let distinct: HashSet<_> = CATALOG.iter().map(|x| x.id).collect();
        assert_eq!(distinct.len(), CATALOG.len());
    }
    #[test]
    fn r218_icloud_is_bidirectional_preview_not_generic_or_cloudkit() {
        let d=find("icloud").expect("iCloud must be listed");
        assert_eq!([d.backup_input,d.backup_output,d.restore_input,d.restore_output],
            [Stage::Preview;4]);
        assert!(d.ready_scope.is_none());
        assert!(d.notes.contains("NSFileCoordinator") || d.consistency.contains("NSFileCoordinator"));
        assert!(d.notes.contains("NO CloudKit"));
        let incoming=plan(Mode::Backup,"icloud","posix").unwrap();
        let outgoing=plan(Mode::Backup,"posix","icloud").unwrap();
        assert!(!incoming.executable && !outgoing.executable);
        assert!(!incoming.preview_executable && !outgoing.preview_executable);
    }
    #[test]
    fn restore_reverses_backup_roles_without_claiming_write_support() {
        assert_eq!(catalog_for(Mode::Restore, true).len(), 26);
        assert_eq!(catalog_for(Mode::Restore, false).len(), 33);
        for entry in CATALOG {
            if matches!(entry.id,"s3"|"ssh"|"webdav"|"proxmox"|"pbs") {
                // No S3/SSH/PVE/PBS full native provider-side remote restore.
                assert_eq!(entry.backup_input,
                    if matches!(entry.id,"s3"|"ssh"|"webdav"){Stage::Ready}else{Stage::Preview});
                assert_eq!(entry.restore_output,Stage::Planned);
            } else {
                assert_eq!(entry.backup_input, entry.restore_output);
            }
            assert_eq!(entry.backup_output, entry.restore_input);
        }
    }
    #[test]
    fn mounted_ciphertext_output_is_distinct_from_plaintext_remote_restore(){
        for p in ["nfs","smb","webdav","cephfs","glusterfs","lustre","gpfs","sshfs"]{
            let d=find(p).unwrap();
            assert_eq!(d.backup_output,if matches!(p,"webdav"|"sshfs"|"lustre"|"nfs"|"smb"|"glusterfs"){Stage::Ready}else{Stage::Preview});
            assert_eq!(d.restore_input,if matches!(p,"webdav"|"sshfs"|"lustre"|"nfs"|"smb"|"glusterfs"){Stage::Ready}else{Stage::Preview});
            let b=plan(Mode::Backup,"posix",p).unwrap();
            let r=plan(Mode::Restore,p,"posix").unwrap();
            assert!(!b.executable && !b.preview_executable);
            assert!(!r.executable && !r.preview_executable);
        }
    }
    #[test]
    fn rbd_create_only_restore_is_typed_not_a_generic_bout(){
        assert_eq!(find("rbd").unwrap().backup_output,Stage::Unsupported);
        assert_eq!(find("rbd").unwrap().restore_input,Stage::Unsupported);
        assert_eq!(find("rbd").unwrap().restore_output,Stage::Ready);
        assert!(plan(Mode::Backup,"posix","rbd").is_err());
        let r=plan(Mode::Restore,"posix","rbd").unwrap();
        assert_eq!(r.destination_stage,Stage::Ready);
        assert!(!r.executable && !r.preview_executable);
        let p=plan(Mode::Backup,"posix","proxmox").unwrap();
        assert_eq!(p.destination_stage,Stage::Planned);
        assert!(!p.executable && !p.preview_executable);
    }
    #[test]
    fn pve_and_pbs_have_distinct_non_executable_capabilities() {
        let pve=find("proxmox").expect("PVE compatibility ID");
        let pbs=find("pbs").expect("PBS must be separately selectable");
        assert_eq!(pve.backup_input,Stage::Preview);
        assert_eq!(pve.backup_output,Stage::Planned);
        assert_eq!(pbs.backup_input,Stage::Preview);
        assert_eq!(pbs.backup_output,Stage::Planned);
        assert_ne!(pve.id,pbs.id);
        assert!(!plan(Mode::Backup,"proxmox","posix").unwrap().executable);
        assert!(!plan(Mode::Backup,"pbs","posix").unwrap().preview_executable);
        assert!(!plan(Mode::Backup,"posix","pbs").unwrap().preview_executable);
        assert!(!plan(Mode::Restore,"pbs","posix").unwrap().preview_executable);
    }
    #[test]
    fn vmware_and_ovirt_ct_are_disabled_until_native_api_certification() {
        for id in ["vmware","ovirt"] {
            let adapter=find(id).expect("Hypervisor catalog entry required");
            assert_eq!(adapter.backup_input,
                if matches!(id,"ovirt"|"vmware"){Stage::Preview}else{Stage::Planned});
            assert_eq!(adapter.backup_output,Stage::Unsupported);
            assert!(adapter.notes.contains("CT disabled"));
            assert!(adapter.notes.contains("NO guest-agent/container-runtime CT fallback"));
            assert!(!plan(Mode::Backup,id,"posix").unwrap().preview_executable);
            assert!(!plan(Mode::Restore,"posix",id).unwrap().preview_executable);
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
                assert_eq!(p.executable,source.id=="posix"&&target.id=="posix");
                assert!(p.confirmation_required_before_writes);
            }
        }
        for source in catalog_for(Mode::Restore, true) {
            for target in catalog_for(Mode::Restore, false) {
                assert_eq!(plan(Mode::Restore, source.id, target.id).unwrap().executable,
                    source.id=="posix"&&target.id=="posix");
            }
        }
    }
    #[test]
    fn actual_preview_pairs_match_the_native_dispatcher(){
        assert!(plan(Mode::Backup,"posix","posix").unwrap().executable);
        for input in ["nfs","smb","gpfs","sshfs","webdav",
                      "cephfs","glusterfs","lustre"]{
            assert!(plan(Mode::Backup,input,"posix").unwrap().preview_executable);
        }
        assert!(plan(Mode::Restore,"posix","posix").unwrap().executable);
        for output in ["nfs","smb","sshfs","cephfs","glusterfs","lustre"]{
            assert!(plan(Mode::Restore,"posix",output).unwrap().preview_executable);
        }
        assert!(!plan(Mode::Backup,"afs","posix").unwrap().preview_executable,
            "AFS volume commands are separate, not filesystem dispatcher");
        assert!(!plan(Mode::Restore,"posix","afs").unwrap().preview_executable);
        assert!(!plan(Mode::Backup,"github","posix").unwrap().preview_executable,
            "Dedicated provider code is not generic executable capability");
    }
    #[test]
    fn preview_does_not_masquerade_as_qualified_plugin() {
        let p=plan(Mode::Backup, "github", "posix").unwrap();
        assert_eq!(p.source_stage, Stage::Preview);
        assert_eq!(p.destination_stage, Stage::Ready);
        assert!(!p.executable);
        assert_eq!(find("postgresql").unwrap().kind, DataKind::Database);
        for database in ["postgresql","mysql"] {
            let d=find(database).unwrap();
            assert_eq!(d.backup_input,Stage::Ready);
            assert_eq!(d.restore_output,Stage::Ready);
            assert_eq!(d.restore_input,Stage::Unsupported);
            assert!(!plan(Mode::Backup,database,"posix").unwrap().preview_executable,
                "Dedicated DB commands must not be misrepresented as generic executor");
        }
        assert_eq!(find("rbd").unwrap().kind, DataKind::BlockDevice);
    }
    #[test]
    fn san_block_sources_and_targets_require_consistent_snapshot_preflight() {
        for id in ["iscsi", "fc", "nvmeof"] {
            let descriptor=find(id).unwrap();
            assert_eq!(descriptor.kind, DataKind::BlockTransport);
            assert_eq!(descriptor.safety, SafetyProfile::SnapshotBlockDevice);
            assert_eq!(descriptor.backup_input, if id=="iscsi" {Stage::Ready} else {Stage::Planned});
            assert_eq!(descriptor.backup_output, Stage::Planned);
            assert_eq!(descriptor.restore_input, Stage::Planned);
            assert_eq!(descriptor.restore_output, if id=="iscsi"{Stage::Ready}else{Stage::Planned});
            assert!(!plan(Mode::Backup, id, "anti-ransomware-vault").unwrap().executable);
            assert!(!plan(Mode::Restore, "anti-ransomware-vault", id).unwrap().executable);
        }
        assert_eq!(find("rbd").unwrap().safety, SafetyProfile::SnapshotBlockDevice);
    }
    #[test]
    fn native_s3_worm_vault_is_dedicated_preview_not_generic_backup_pipeline() {
        let s3=find("s3").expect("S3 vault registry is required");
        assert_eq!(s3.backup_input,Stage::Ready,
            "S3 version-pinned object import has dedicated native Rust CLI");
        assert_eq!(s3.backup_output,Stage::Preview);
        assert_eq!(s3.restore_input,Stage::Preview);
        assert_eq!(s3.restore_output,Stage::Planned);
        let backup=plan(Mode::Backup,"posix","s3").unwrap();
        let restore=plan(Mode::Restore,"s3","posix").unwrap();
        assert!(!backup.executable && !backup.preview_executable,
            "Dedicated s3-vault command MUST NOT bypass generic dispatcher");
        assert!(!restore.executable && !restore.preview_executable);
    }
    #[test]
    fn s3_input_and_dropbox_output_are_dedicated_but_not_general_executors(){
        let s3=find("s3").unwrap();
        let dropbox=find("dropbox").unwrap();
        assert_eq!(s3.backup_input,Stage::Ready);
        assert_eq!(s3.restore_output,Stage::Planned);
        assert_eq!(dropbox.backup_output,Stage::Preview);
        assert_eq!(dropbox.restore_input,Stage::Preview);
        let s3_pair=plan(Mode::Backup,"s3","posix").unwrap();
        let output_pair=plan(Mode::Backup,"posix","dropbox").unwrap();
        assert!(!s3_pair.executable && !s3_pair.preview_executable);
        assert!(!output_pair.executable && !output_pair.preview_executable);
    }
    #[test]
    fn anti_ransomware_vault_is_ciphertext_destination_and_restore_source_only() {
        let vault=find("anti-ransomware-vault").unwrap();
        assert_eq!(vault.safety, SafetyProfile::ImmutableRecoveryVault);
        assert_eq!(vault.backup_input, Stage::Unsupported);
        assert_eq!(vault.backup_output, Stage::Ready);
        assert_eq!(vault.restore_input, Stage::Ready);
        assert_eq!(vault.restore_output, Stage::Unsupported);
        assert!(plan(Mode::Backup, "posix", vault.id).is_ok());
        assert!(plan(Mode::Restore, vault.id, "posix").is_ok());
        assert!(plan(Mode::Backup, vault.id, "posix").is_err());
        assert!(plan(Mode::Restore, "posix", vault.id).is_err());
        assert!(!plan(Mode::Backup, "posix", vault.id).unwrap().executable);
    }
    #[test]
    fn glusterfs_real_mounted_single_volume_ready_does_not_certify_replicated_cluster() {
        let d=find("glusterfs").unwrap();
        assert_eq!(d.kind, DataKind::Files);
        assert_eq!(d.backup_input, Stage::Ready);
        assert_eq!(d.backup_output, Stage::Ready);
        assert_eq!(d.restore_input, Stage::Ready);
        assert_eq!(d.restore_output, Stage::Ready);
        assert!(d.ready_scope.unwrap().contains("single_node_real_fuse_mount"));
        assert!(d.ready_scope.unwrap().contains("no GFID"));
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
            // GPFS via standard NFS/SMB now has protocol-only input and
            // create-only mounted output preview, not native GPFS support.
            assert_eq!(d.backup_input, if id=="lustre"{Stage::Ready}else if matches!(id,"gpfs"|"afs"){Stage::Preview}else{Stage::Planned}, "{id}");
            assert_eq!(d.backup_output, if id=="lustre"{Stage::Ready}else if matches!(id,"afs"|"gpfs"){Stage::Preview}else{Stage::Planned}, "{id}");
            assert_eq!(d.restore_input, if id=="lustre"{Stage::Ready}else if matches!(id,"afs"|"gpfs"){Stage::Preview}else{Stage::Planned}, "{id}");
            assert_eq!(d.restore_output, if id=="lustre"{Stage::Ready}else if matches!(id,"gpfs"|"afs"){Stage::Preview}else{Stage::Planned}, "{id}");
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
        assert_eq!(d.backup_input,Stage::Preview);
        assert!(!plan(Mode::Backup,"gpfs","posix").unwrap().executable);
        assert!(plan_profiles(Mode::Restore,"posix","gpfs",None,None,
            None,Some(GpfsAccess::CesNfs)).unwrap().preview_executable);
        for protocol in GPFS_CES_ACCESS {
            let b=plan_profiles(Mode::Backup,"gpfs","sshfs",None,None,Some(*protocol),None).unwrap();
            assert!(!b.executable);
            let r=plan_profiles(Mode::Restore,"sshfs","gpfs",None,None,None,Some(*protocol)).unwrap();
            assert!(r.required_preflight.iter().any(|x|x.contains("Client license")));
            assert!(r.required_preflight.iter().any(|x|x.contains("create-only")));
        }
        assert!(GpfsAccess::parse("native").is_err());
        assert!(plan_profiles(Mode::Backup,"sshfs","posix",None,None,
                              Some(GpfsAccess::CesNfs),None).is_err());
    }
    #[test]
    fn webdav_ro_source_requires_exact_native_mount_and_attestation() {
        let d=find("webdav").unwrap();
        assert_eq!(d.backup_input, Stage::Ready);
        assert_eq!(d.backup_output, Stage::Ready);
        assert_eq!(d.restore_output,Stage::Planned);
        let webdav_plan=plan(Mode::Backup,"webdav","posix").unwrap();
        assert!(webdav_plan.preview_executable);
        assert!(!webdav_plan.executable);
        assert!(webdav_plan.required_preflight.iter().any(|s|s.contains("davfs2")));
        assert!(!plan(Mode::Restore,"posix","webdav").unwrap().preview_executable);
    }
    #[test]
    fn sshfs_source_and_target_are_planned_distinct_from_sftp() {
        let d=find("sshfs").unwrap();
        assert_eq!(d.kind,DataKind::Files);
        assert_eq!(d.backup_input,Stage::Ready);
        assert_eq!(d.backup_output,Stage::Ready);
        assert_eq!(d.restore_input,Stage::Ready);
        assert_eq!(d.restore_output,Stage::Ready);
        assert_ne!(d.id,find("ssh").unwrap().id);
        assert_eq!(find("ssh").unwrap().backup_input,Stage::Ready);
        assert_eq!(find("ssh").unwrap().restore_output,Stage::Planned);
        let b=plan(Mode::Backup,"sshfs","anti-ransomware-vault").unwrap();
        assert!(b.required_preflight.iter().any(|x|x.contains("SSH host key")));
        assert!(!b.executable);
        let r=plan(Mode::Restore,"anti-ransomware-vault","sshfs").unwrap();
        assert!(r.required_preflight.iter().any(|x|x.contains("manifest-last")));
        assert!(!r.executable);
    }
    #[test]
    fn oracle_rman_media_ready_is_not_generic_oracle_database_restore() {
        let d=find("oracle").unwrap();
        assert_eq!(d.kind,DataKind::Database);
        assert_eq!(d.backup_input,Stage::Ready);
        assert_eq!(d.backup_output,Stage::Unsupported);
        assert_eq!(d.restore_output,Stage::Ready);
        assert_eq!(d.restore_input,Stage::Unsupported);
        assert!(d.ready_scope.unwrap().contains("oracle_rman_media_pieces_new_directory"));
        let b=plan(Mode::Backup,"oracle","anti-ransomware-vault").unwrap();
        assert!(!b.executable && !b.preview_executable);
        let r=plan(Mode::Restore,"anti-ransomware-vault","oracle").unwrap();
        assert!(!r.executable && !r.preview_executable);
        assert!(plan(Mode::Backup,"posix","oracle").is_err());
        assert!(d.notes.contains("does NOT orchestrate Oracle DB restore"),"Never pretend to automate RMAN database restoration");
    }
    #[test]
    fn plan_mode_is_not_silently_defaulted() {
        assert!(Mode::parse("Backup").is_err());
        assert_eq!(Mode::parse("backup").unwrap(), Mode::Backup);
        assert_eq!(Mode::parse("restore").unwrap(), Mode::Restore);
    }
}
