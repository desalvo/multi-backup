# Bidirectional provider matrix — Multi Backup

Every row is a **plugin role**, not an implementation claim. Restores reverse
the two plugin roles: \`backup(input=A,output=B)\` becomes
\`restore(input=B,output=A)\`; both ends must have the corresponding direction
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
| proxmox | Planned | — | VE VM/CT, volumes, config and PBS integration |
| kubernetes | Planned | — | Kubernetes/OpenShift/OKD API, CSI/CRD, secrets policy |
| vmware | Planned | — | vSphere VM configuration, disks, CBT/VADP |
| ovirt | Planned | — | oVirt/RHV VM, disks and engine config |
| mysql | Planned | — | MySQL/MariaDB consistency + binlog/PITR |
| postgresql | Planned | — | pg_basebackup/WAL or logical export and PITR |
| ssh | Planned | Planned | SSH/SCP/SFTP host verification, remote metadata |

**Counts:** 18 backup input roles; 7 backup output roles; 7 restore input
roles; 18 restore output roles. **25 distinct backup roles**, **50
directional capabilities** including their restore reversals. Three native
provider IDs have existing *dedicated* preliminary paths (GitHub, Dropbox,
POSIX); generic cross-provider dispatch, all planned adapters and all
provider-pair restore integrations remain unqualified.

**Definition of progress:** Catalogue/contract coverage is 25/25 (100% of
the requested role definitions). Operational plugin pair certification is
0/25 (0%) at this stage. Neither percentage is overall RC readiness. The
earlier 57.5% maturity estimate refers to the *old* scoped 20-macroarea
inventory; adding 25 significant roles means it cannot be reused as an
overall product completion percentage without an explicit rebaseline.
