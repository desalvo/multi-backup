# Bidirectional provider matrix — Multi Backup

Every row is a **plugin role**, not an implementation claim. Restores reverse
the two plugin roles: `backup(input=A,output=B)` becomes
`restore(input=B,output=A)`; both ends must have the corresponding direction
implemented and tested before the reverse operation is enabled.

| ID | Backup input → restore output | Backup output ← restore input | Consistency requirement |
| --- | --- | --- | --- |
| github | Preview | — | Git mirror/LFS, Actions and metadata inventory |
| dropbox | Preview | Planned | File ID, revision, hash, explicit writable scope |
| google-drive | Planned | Planned | Google-native file export, shared drive metadata, scoped encrypted upload |
| onedrive | Planned | Planned | Microsoft Graph delta/eTag, tenant consent and resumable encrypted upload |
| posix | Preview | Preview | Files, ACL/xattr, optional FS snapshot |
| s3 | Planned | Planned | Versioned objects, multipart, optional Object Lock |
| webdav | Planned | Planned | ETag/collection and atomic publish workaround |
| nfs | Preview (read-only input) | Planned | NFSv3+, mounted server-side snapshot if available |
| smb | Preview (read-only input) | — | SMB/CIFS ACL, streams and snapshot semantics |
| cephfs | Planned | Planned | CephFS snapshot, file metadata and safe writer |
| glusterfs | Planned | Planned | Gluster distributed volume, consistency barrier/snapshot, GFID, ACL/xattr; restore via mounted volume only |
| lustre | Planned | Planned | Lustre MDT/OST, stripe metadata and coordinated snapshot; external client |
| gpfs | Preview (CES read-only input) | Planned | IBM Storage Scale CES NFSv3/v4 or SMB3 using open-source OS clients; consistent fileset required |
| hdfs | Planned | Planned | SWebHDFS/WebHDFS, snapshots, ACL/xattr and Kerberos |
| afs | Planned | Planned | OpenAFS cells and VLDB volume IDs, full/incremental vos dumps, ACL and new-volume restore |
| alfresco | Planned | Planned | Alfresco REST/CMIS document versions, types/aspects, ACL, repository metadata |
| owncloud | Planned | Planned | ownCloud WebDAV/OCS revisions, file IDs, permissions and ETags |
| nextcloud | Planned | Planned | Nextcloud WebDAV/OCS revisions, chunked upload, ACL/shares and ETags |
| rbd | Planned | — | RADOS Block Device snapshot/diff (image, not folder) |
| iscsi | Planned | Planned | iSCSI IQN, WWID, LUN and quiesced snapshot; create-new target only |
| fc | Planned | Planned | Fibre Channel WWPN/WWID, multipath, stable snapshot and exclusive fencing |
| nvmeof | Planned | Planned | NVMe-oF NQN/NSID/NGUID, transport, snapshots and isolated new namespace |
| proxmox | Planned | — | VE VM/CT, volumes, config and PBS integration |
| kubernetes | Planned | — | Kubernetes/OpenShift/OKD API, CSI/CRD, secrets policy |
| vmware | Planned | — | vSphere VM configuration, disks, CBT/VADP |
| ovirt | Planned | — | oVirt/RHV VM, disks and engine config |
| mysql | Planned | — | MySQL/MariaDB consistency + binlog/PITR |
| postgresql | Planned | — | pg_basebackup/WAL or logical export and PITR |
| oracle | Planned | — | Oracle RMAN online with ARCHIVELOG, redo/SCN and SPFILE/controlfile recovery |
| ssh | Planned | Planned | SSH/SCP/SFTP host verification, remote metadata |
| sshfs | Preview (read-only input) | Planned | Pre-mounted SSHFS/FUSE SFTP filesystem, pinned host key, encrypted-only output |
| anti-ransomware-vault | — | Planned | WORM/Object Lock, independent append-only ingest and recovery plane, offline credentials |

**Counts (expanded scope r036):** 32 plugin IDs. **31 backup input**
and **22 backup output** roles, mirrored by 22 restore inputs/31 restore
outputs. There are **53 declared backup roles** and 53 inverse restore
roles (106 directional capability declarations). The possible pairing
space is 31 × 22 = **682** backup combinations and 682 reverse restore
combinations. No generic pair is certified for execution.

**Progress:** 53/53 requested backup role definitions represented (100%
catalog coverage), 0/53 unified adapters qualified, and 0/682 generic
backup combinations certified. These are declaration/qualification
metrics, NOT the overall product completion percentage. Existing direct
GitHub/Dropbox/POSIX preview commands remain separate.

### LVM over iSCSI and Fibre Channel

Both `iscsi` and `fc` expose optional *LVM on SAN* layouts independent
for input and output. The plugin ID stays unchanged; these are storage
profiles, not four extra provider plugins.

| LVM layout | Cluster | Shared LUN/VG | Required safety and consistency |
| --- | --- | --- | --- |
| `local` | No | No | Single host ownership and VG/PV/LV identity |
| `cluster-local` | Yes | No | Per-node VG, namespace fencing and node identity |
| `shared-exclusive` | Optional | Yes, single writer | Exclusive VG/LV activation and verifiable ownership |
| `ha-exclusive` | Yes | Shared, one active owner | Cluster-managed active/passive, quorum and fencing |
| `cluster-shared` | Yes | Yes, distributed locks | Verified lvmlockd+DLM/sanlock as supported by OS/cluster and cluster-aware filesystem/application |

`raw LUN` remains the default when no LVM layout is selected. On restore,
volume creation must never silently reuse a production LUN, VG, PV, LV
or multipath WWID. Before reading any LV, the future driver must confirm a
coherent snapshot; file-level snapshots, Proxmox VM volume chains and raw
LVM snapshots have different semantics and cannot be interchanged.

```sh
multi-backup-native plugins plan --mode backup --input iscsi --input-lvm cluster-shared --output anti-ransomware-vault --json
multi-backup-native plugins plan --mode restore --input anti-ransomware-vault --output fc --output-lvm ha-exclusive --json
```

The commands are **read-only planning**, with explicit required-preflight
reasons and `executable=false`. They do not connect/discover LUNs, run
`pvscan`, `vgchange`, acquire locks, stop VMs, or write blocks. Lock manager
support differs between distributions and cluster resource managers; choose
a tested option instead of assuming all sanlock/DLM combinations work.

### GlusterFS input and output

`glusterfs` is a separate filesystems plugin rather than an NFS or POSIX
alias. Input requires a consistent mounted-volume view, verified replica/
disperse/shard topology and retention of GFID, ACL, xattrs and symlink
information. Output accepts only encrypted objects and manifests with
append-last atomicity, checksums and minimum-privilege credentials; it
must never write directly to a brick or overwrite an existing live tree.
For restore, write to a **new** isolated volume/path through the GlusterFS
client and only publish after integrity verification. Real snapshots,
heal/split-brain behavior and crash recovery remain untested.

### r032 — additional bidirectional plugins and vendor/legal gates

**Lustre**: operate on an authorized mounted Lustre client with MDT/OST
metadata, stripe layout, ACL/xattr and a consistent namespace/storage
snapshot. Do not bundle separate kernel modules into Multi Backup
installers without reviewing their actual license obligations.

**IBM Storage Scale / GPFS**: support documented filesystem APIs or a
customer-provisioned client; do **not** redistribute IBM code. The
operator must verify its contractual entitlement, including the right
to export/backup data with the chosen GPFS client license. Some client
licenses restrict exports using other protocols; do not assume general
rights merely because files are mounted.

**HDFS**: prefer SWebHDFS where configured, use Kerberos/SPNEGO or scoped
delegation tokens, validate NameNode/DataNode redirect hosts and TLS,
take HDFS-consistent snapshots and preserve ACL/xattr where APIs allow.

**AFS / OpenAFS**: the main AFS mode is *per volume*, not an ordinary POSIX
directory tree. Use an authorized external OpenAFS client and stable cell,
VLDB/volume IDs and read-only/backup-volume snapshots. Preserve a complete
`vos dump` full/incremental chain and volume-to-mountpoint mapping.
For recovery, `vos restore` must create a **new volume** in an
authorized target cell/partition without modifying production volumes.
Capture AFS ACLs separately when needed (filesystem conversion via
`restorevol` may lose them). Authentication uses Kerberos tokens
under least-privilege policy, never stored in encrypted manifests or logs.
OpenAFS has IBM Public License 1.0 obligations; the adapter does **not**
embed or relicense OpenAFS code.

**Alfresco**: REST/CMIS repository APIs, version history, content model,
node IDs, types/aspects and ACLs; enterprise-only extensions may have
separate license/API entitlements.

**Google Drive, OneDrive**: existing planned input plugins also become
planned *output* plugins, backed by scopes validated for the chosen app
and tenant, new-object encrypted upload and conflict-safe finalization.
Provider-native documents and file permission mappings are not equivalent
to byte-stream files.

**ownCloud / Nextcloud**: two distinct IDs even if they share low-level
WebDAV/OCS protocols. Account for server-specific versions, shares, file
IDs, large-file uploads and eventual commit semantics; never assume
WebDAV success alone preserves application metadata.

All new capabilities are **Planned**, never implicitly executable. Backups
send authenticated encrypted content to output adapters; restores require
the inverse role and a new isolated destination. Missing privileges,
snapshot consistency or vendor/API entitlements must stop execution.

### r034 — open-source GPFS CES and SSHFS/FUSE

The `gpfs` plugin's **ces-nfs** and **ces-smb** profiles require an existing
NFSv3/v4 or SMB3 export from an IBM Storage Scale CES server. The Linux
client can be open-source (`nfs-utils`, `cifs-utils`) with no GPFS Client
license; the server still requires IBM's applicable Server entitlement.
No open-source native GPFS mount client is asserted or bundled. Native
GPFS fileset/snapshot/ILM metadata requires separate authorized capture.
No actual GPFS source/target plugin is yet certified for backup/restore.

The new independent `sshfs` plugin permits input and output roles, using
a pre-mounted distro-provided **SSHFS (GPL-2.0) and FUSE3** client. It differs
from streaming SSH/SFTP/SCP: file operations happen through a mounted
filesystem, so mount liveness, strong host-key verification, containment of
symlinks, stable file metadata and encrypted-only storage are required.
Avoid unencrypted directport bypass. Do not bundle the GPL client inside
the EUPL application before compatibility review.

Linux read-only discovery:
```sh
multi-backup-native plugins probe --plugin gpfs --mount /mnt/gpfs --access ces-nfs --json
multi-backup-native plugins probe --plugin sshfs --mount /mnt/sshfs --json
multi-backup-native plugins plan --mode backup --input gpfs --input-gpfs-access ces-nfs --output sshfs --json
```
The probe verifies the exact kernel mount type, not storage ownership,
server software, key pinning, write permissions or snapshot consistency.
It never mounts, reads customer files or writes any data.


## r038 — native plugin runtime preview (not certified)

Five executable **development-preview backup pairs** now have a Rust code path: POSIX→POSIX, NFS→POSIX, SMB→POSIX, SSHFS→POSIX, and GPFS CES→POSIX. The output is a verified age-encrypted POSIX chunk store. POSIX→POSIX fresh-directory restore is the one generic reverse restore path implemented. Mounted remote sources run only on Linux when the selected path is an exact matching **read-only OS mount** and the operator explicitly acknowledges a consistent snapshot. No snapshot can be proven from a user flag or mount type; ACL/xattr and live provider consistency still require end-to-end acceptance. The registered catalog remains **32 plugins, 53 declared backup roles**, with **0/682 certified generic pairs**. The remaining directions are not executors.

## r038 — Portabilità degli account e storage GC

Per ogni plugin che usa login, il restore deve poter indicare un utente di destinazione distinto da quello originale, autorizzato con credenziali proprie. Le implementazioni Rust dedicate attuali permettono un nuovo repository GitHub o un file Dropbox su un altro account, senza certificare ancora il restore parziale universale. Lo storage POSIX impiega FULL/INCREMENTAL, purge con rebase e GC con repack automatico delle parità 8+2. Questo non aumenta il numero dei plugin genericamente certificati: 0/682. Fonte [FEATURE_INVENTORY_R038.md](FEATURE_INVENTORY_R038.md).


## r039 — Cross-plugin source execution (not provider certification)

Source execution options are server_local / ssh_agentless / managed_agent_pull / managed_agent_push for *all future* input plugins. The AFS plugin has typed local/pinned SSH dump plans and a bounded parallel-volume agent preview; this does not enable arbitrary plugins over SSH or scripts without explicit adapters. Generic replica execution remains limited to already implemented filesystem inputs in server_local mode. PostgreSQL storage visibility shared/node_local controls replica placement; no cloud destination writer becomes certified merely through registration. [Orchestration IT](CLUSTER_ORCHESTRATION_R039_IT.md).
