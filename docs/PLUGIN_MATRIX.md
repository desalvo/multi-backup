# Bidirectional provider matrix — Multi Backup

Every row is a **plugin role**, not an implementation claim. Restores reverse
the two plugin roles: `backup(input=A,output=B)` becomes
`restore(input=B,output=A)`; both ends must have the corresponding direction
implemented and tested before the reverse operation is enabled.

| ID | Backup input → restore output | Backup output ← restore input | Consistency requirement |
| --- | --- | --- | --- |
| github | Preview | — | Git mirror/LFS, Actions and metadata inventory |
| dropbox | Preview | Planned | File ID, revision, hash, explicit writable scope |
| google-drive | Planned | — | Google-native file export, shared drive metadata |
| onedrive | Planned | — | Microsoft Graph drive delta/eTag |
| posix | Preview | Preview | Files, ACL/xattr, optional FS snapshot |
| s3 | Planned | Planned | Versioned objects, multipart, optional Object Lock |
| webdav | Planned | Planned | ETag/collection and atomic publish workaround |
| nfs | Planned | Planned | NFSv3+, mounted server-side snapshot if available |
| smb | Planned | — | SMB/CIFS ACL, streams and snapshot semantics |
| cephfs | Planned | Planned | CephFS snapshot, file metadata and safe writer |
| glusterfs | Planned | Planned | Gluster distributed volume, consistency barrier/snapshot, GFID, ACL/xattr; restore via mounted volume only |
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

**Counts (expanded scope r031):** 22 backup input roles, 12 backup output
roles, 12 restore inputs and 22 restore outputs across 23 distinct plugin IDs.
There are **34 backup-role definitions** and 34 inverse restore-role definitions
(68 directional capability records). Cross-product: 22 × 12 = **264**
possible directed backup pairs, with 264 reversed restore pairs; none
is certified for unified generic dispatch yet.

**Progress:** catalogue 34/34 (100% declared). Qualified unified plugin roles
0/34 (0%) and certified end-to-end backup pairs 0/264 (0%). These measures
do not imply 0% existing dedicated GitHub/Dropbox/POSIX preview capability,
nor a defensible overall RC percentage.

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
