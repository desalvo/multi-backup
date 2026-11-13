# Plugin architecture / API

The public plugin registry is `backends.available()`. Each `BackendDescriptor` exposes a stable identifier, display title, description, capabilities, `default_enabled`, and a `handler` callback. A plugin is selected using `multi-backup backup --backend <id>` and its availability is controlled by `multi-backup plugin enable/disable <id>`; the state is also adjustable in the GUI. Plugins are not automatically discovered from untrusted Python source directories, to prevent code injection.

## GitHub backend

Runs the inherited `ResumableBackup` engine from `multi_backup.py`: hashed checkpoints, per-repository metadata and CI archives, immutable encrypted Git snapshots, account-level recovery metadata, and optional dry-run or real CI artifact cleanup. Deletion is guarded by a dynamic dependency scan and is only performed when expressly enabled. Run status and aggregate cleanup reports are available through CLI progress events.

## Dropbox backend (RC)

Enabled only on demand. Credentials from `MULTI_BACKUP_DROPBOX_TOKEN` or OS keyring. Dropbox v2 APIs used: `files/list_folder`, `files/list_folder/continue` and `files/download`. All are **read-only**. An object key is `SHA256(file-id + NUL + revision)`; each encrypted archive is persisted under `dropbox/objects/<prefix>/`. A checksum verified on next run lets the plugin reuse the exact encrypted object, reducing storage and network usage. An encrypted manifest records the current contents of the selected folder. No local retention or remote purge operations exist in this plugin. External Dropbox team sharing policy, Apps and account settings are not included in this first RC.

## Plugin security contract

Plugins must explicitly state destructive capabilities, never store raw credentials in logs/checkpoints/manifests, use safe path validation, require explicit opt-in for destructive actions, publish only atomically after successful encryption, and remain testable with mocked APIs. Proposed future plugins (S3, WebDAV, OneDrive) must follow these same rules. In production, all external plugins should be distributed as signed packages; dynamic execution of arbitrary files is not enabled.
