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

**Counts (expanded scope):** 21 backup input roles, 11 backup output roles,
11 restore input roles, and 21 restore output roles, across 22 distinct plugin IDs.
**32 backup-role contracts**, automatically reversed as 32 restore-role contracts
(64 directional capability declarations). The potential directed pair space is
21 × 11 = 231 backup combinations and 11 × 21 = 231 inverse restore
combinations, but **none is currently qualified for generic execution**.

**Progress:** 32/32 requested backup role declarations (100% catalog coverage);
0/32 generic adapter roles certified (0%); 0/231 provider pair combinations
certified (0%). These are *different metrics*. Existing dedicated GitHub,
Dropbox and POSIX preview paths remain independent from the generic adapters.
Whole-product RC completion is not yet rebaselined after scope expansion.

**SAN and immutable-vault constraints:** iSCSI, FC, and NVMe-oF expose
remote block LUNs/namespaces, not ordinary folders. Record persistent
device identity and multipath aliases, capture and validate a crash- or
application-consistent *storage snapshot*, and prevent simultaneous writers.
Backup outputs or restores to these transports must create an **isolated,
empty, explicitly approved target**, never write to an active mapped source
or silently overwrite production media. Their current stage is Planned;
they perform **zero raw-device reads or writes**.

The anti-ransomware vault is **a ciphertext-only backup output / read-only
restore input**, not a generic backup source or a restore destination. Its
promised WORM/Object Lock retention, separate append-only ingest and recovery
credentials, immutable manifest, 2-of-3 approval, audit and an independent
recovery drill are not certified. The existing protected chunk store is a
development precursor, **not an actual immutable WORM vault**.
