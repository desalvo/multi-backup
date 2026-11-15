# Native Rust plugin executors — r039+ EN

Updated 15 November 2026. **Draft PR #11 preview; NOT a production certified RC.** The plugin catalog, typed command plan, actual executable runtime and externally proven recovery are separate stages.

## Compiled execution matrix

| Task | Source | Destination/required checks |
| --- | --- | --- |
| Generic filesystem backup | POSIX, NFS, SMB, SSHFS, GPFS CES, **CephFS, GlusterFS, Lustre** | POSIX age/zstd encrypted chunk store and 8+2 parity; Linux exact read-only mount evidence plus operator snapshot attestation for nonlocal input |
| Generic filesystem restore | Verified POSIX chunk store | New POSIX directory or exact RW mount NFS, SSHFS, CephFS, GlusterFS, Lustre; no existing destination may be overwritten |
| Dedicated AFS backup | Exact full/incremental vos dump on approved local or pinned SSH agentless host | Encrypted chunk stream without plaintext spool; manifest only after successful child exit |
| Dedicated AFS restore | Verified AFS volume-specific full/incremental chain | NEW volume at declared cell/server/partition, full overwrite abort followed by ordered increments on the newly created volume only |

These are **8 generic backup source paths and 6 generic restore destinations**, plus separate AFS volume code. They are preview executors, NOT proof that 682 generic combinations are certified. Dedicated GitHub and Dropbox Rust actions remain separate from this dispatcher.

### Mounted examples

~~~bash
multi-backup-native plugins probe --plugin glusterfs --mount /mnt/gluster --json
multi-backup-native plugins backup --input glusterfs --output posix --source /mnt/gluster --root /backups/store --identity /secrets/age-key --label gluster --source-snapshot-attested --json
multi-backup-native plugins restore --input posix --output cephfs --root /backups/store --identity /secrets/age-key --id VERIFIED_BACKUP --destination /mnt/cephfs/NEW_RESTORE
~~~

Supported kernel mount types: CephFS ceph/fuse.ceph-fuse; GlusterFS fuse.glusterfs; Lustre lustre. Kernel RO/RW flag and matching exact mountpoint checks do not prove application consistency, snapshots, xattrs/ACLs or correct HA ownership. Multi Backup does not mount remote resources. The restore target is a nonexistent direct child of the checked RW mount. GPFS CES remains a read-only mounted input preview; output requires a mode-aware adapter.

## Native encrypted OpenAFS volume stream

Input job JSON:

~~~json
{"id":"afs-01","cell":"example.org","volume":"user.original",
 "auth":{"type":"existing_token"},"execution":{"type":"local"},
 "mode":{"kind":"full"}}
~~~

~~~bash
multi-backup-native plugins afs-backup --job-json input.json --root /backups/store --identity /secrets/age-key --label afs-full --max-transfer-bps 20971520 --json
~~~

An incremental mode uses kind=incremental with an actual OpenAFS formatted since timestamp and a previous encrypted backup ID. The source cell/volume must match the ancestor. The operator must validate continuity across real AFS snapshot epochs: date syntax alone cannot prove no missing changes. The stdout dump is encrypted into the shared chunk store with no unencrypted file staging. Producer failure prevents manifest publication; unused encrypted chunks can later be garbage collected safely.

SSH agentless mode uses an explicit host/user/identity/known_hosts profile with pinned host key, public-key-only auth, no user SSH config, forwarding, proxy or prompts. SSH host-side localauth requires approved fileserver assertion and exact AFS site. Automated keytab/kinit/aklog and local privileged localauth remain unimplemented pending safety and live privileges. Managed agents have independently approved profile hashes.

### Restore to a fresh volume only

Target job JSON:

~~~json
{"cell":"example.org","server":"afs1.example.org","partition":"/vicepa",
 "volume":"new.recovered","auth":{"type":"existing_token"}}
~~~

~~~bash
multi-backup-native plugins afs-restore-plan --target-json target.json --json
multi-backup-native plugins afs-restore-new --target-json target.json --root /backups/store --identity /secrets/age-key --id AFS_ENCRYPTED_BACKUP_ID --confirm-target example.org:afs1.example.org:/vicepa:new.recovered --json
~~~

This command validates the entire full+incremental encrypted chain and only accepts a typed AFS per-volume stream. The first vos restore is explicitly configured with overwrite abort, no fixed volume ID, and a name limited to 22 bytes. Subsequent incremental vos restores are issued solely after a successful fresh full restore in the SAME invocation. Existing targets are never deliberately overwritten. Partial new volumes after errors are left for manual inspection, never automatically removed or retried. Restore currently requires a valid local AFS token.

CI runs rust/tests/afs_stream_roundtrip.rs with real age encryption, the compiled Rust CLI and a FAKE vos child executable to verify ordered full/incremental data, bad confirmations and failed sources. Live AFS cell/ACL/VLDB/replication/keytab tests are outstanding and are mandatory before release certification.

## Planned/not yet executable providers

S3/RGW, Google Drive, OneDrive, WebDAV, ownCloud, Nextcloud, Alfresco, HDFS, RBD and SAN block endpoints, hypervisors, Kubernetes/OpenShift, PostgreSQL/MySQL/Oracle, plus full provider-to-provider transfers. Every actual adapter must validate distinct source and target credentials, no-overwrite recovery, provider metadata, cryptographic integrity, idempotency, and real failure-mode acceptance. No catalog-only plugin silently executes as generic POSIX.

See PLUGIN_MATRIX.md, PLUGIN_API.md, AGENTS_AFS_R039_EN.md, RC_CERTIFICATION_MATRIX.md and HANDOFF.md.

## Update: AFS recovery through pinned SSH agentless (15 November 2026)

The target JSON may include an optional execution object with type=ssh_agentless, host, user, absolute identity key path, pinned known_hosts and afs_server=false. Full+incremental NEW volume restoration may run locally with an existing AFS token, or across a dedicated SSH connection where the remote account independently already has an authorized AFS token. SSH uses strict host-key enforcement, public-key-only auth, -F none, disabled ProxyCommand/ProxyJump, no forwarding, no prompts or password fallback. Verified decrypted vos dump bytes stream over encrypted SSH stdin and are not staged on local plaintext disk. Full uses overwrite abort; subsequent increments occur only after full success in the same invocation. CI uses fake ssh+vos; live SSH, remote keytab and agent-managed restore are still NOT certified.
