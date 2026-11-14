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
| nfs | Planned | Planned | NFSv3+, mounted server-side snapshot if available |
| smb | Planned | — | SMB/CIFS ACL, streams and snapshot semantics |
| cephfs | Planned | Planned | CephFS snapshot, file metadata and safe writer |
| glusterfs | Planned | Planned | Gluster distributed volume, consistency barrier/snapshot, GFID, ACL/xattr; restore via mounted volume only |
| lustre | Planned | Planned | Lustre MDT/OST, stripe metadata and coordinated snapshot; external client |
| gpfs | Planned | Planned | IBM Storage Scale fileset/snapshot, operator-provided license and client |
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
| ssh | Planned | Planned | SSH/SCP/SFTP host verification, remote metadata |
| anti-ransomware-vault | — | Planned | WORM/Object Lock, independent append-only ingest and recovery plane, offline credentials |

**Counts (expanded scope r032):** 30 distinct plugin IDs.
**29 backup input** and **21 backup output** roles; the reverse direction has
21 restore input and 29 restore output roles. **50 declared backup roles**
plus 50 mirrored restore roles = 100 directional capability declarations.
The theoretical pairing space is 29 × 21 = **609** backup combinations and
609 reverse restore combinations. **Zero generic pairs are certified for
execution**; catalog registration alone does not implement remote APIs.

**Progress:** 50/50 requested backup role definitions inventoried (100% of
catalog requirements); 0/50 generic adapters certified (0%);
0/609 generic backup pairs certified (0%). Neither number represents the
overall RC completion after expanding the scope. Dedicated Rust GitHub/
Dropbox/POSIX paths remain available as separate preview commands.

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
