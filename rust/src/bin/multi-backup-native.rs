//! Standalone native Rust encrypted backup CLI. Native plugin execution is
//! the primary development path; Python data migration is optional.
use multi_backup_core::{portable, chunk_store, archive, archive_management, checkpoint_audit, dropbox_client, dropbox_hash, github_client, journal, remote_restore, updater, vault, immutable_vault, s3_vault, plugins, virtualization_inventory, hypervisor_inventory, plugin_executor, mounted_output, webdav_output, ssh_input, oracle_rman, rbd_native, iscsi_snapshot, pve_media, pve_guest_restore, pbs_native, fs_metadata_archive, backup_chain, account_restore, scheduler, afs_volume, database, execution_policy::ExecutionLimits};
use zeroize::Zeroizing;
use std::io::{self, Read};
use std::{collections::BTreeMap, env, path::{Path,Component}, process};
use sha2::{Digest,Sha256};

fn usage() -> &'static str {
    "Multi Backup Rust archive CLI (EUPL-1.2)\n\
     Usage:\n\
       multi-backup-native --version\n\
       multi-backup-native updates check|stage --channel rc|stable|early-release [--directory PRIVATE]\n\
       multi-backup-native updates activate-portable --active DIR --candidate DIR --rollback NEW_DIR --binary REL --expected-sha256 SHA (APP STOPPED; portable only)\n\
       multi-backup-native updates rollback-portable --active DIR --rollback DIR --rejected NEW_DIR --binary REL --expected-sha256 OLD_SHA (APP STOPPED)\n\
       multi-backup-native plugins list [--role input|output] [--mode backup|restore] [--json]\n\
       multi-backup-native plugins ssh-backup --config OWNER_ONLY_SSH.json --root EXISTING_AGE_STORE --identity AGE_PRIVATE --label LABEL [--top-namespace NS] [--json] (pinned host key, single version-attested remote file into age chunks)\n\
       multi-backup-native plugins webdav-http-backup --config OWNER_ONLY_WEBDAV.json --source LOCAL_DIR --filename NEW.tar.gz.age --recipient AGE_PUBLIC --identity AGE_PRIVATE [--json] (native trusted HTTPS conditional create-only PUT and independent GET; env MULTI_BACKUP_WEBDAV_PASSWORD)\n\
       multi-backup-native plugins webdav-http-restore --config OWNER_ONLY_WEBDAV.json --filename VERIFIED.tar.gz.age --identity AGE_PRIVATE --destination NEW_LOCAL_DIR [--json]\n\
       multi-backup-native plugins mounted-backup --output nfs|smb|webdav|cephfs|glusterfs|lustre|gpfs|sshfs --mount EXACT_MOUNT --source LOCAL_DIR --filename NEW.tar.gz.age --recipient AGE_PUBLIC --identity AGE_PRIVATE [--output-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins mounted-restore --input MOUNT_PROVIDER --mount EXACT_MOUNT --filename VERIFIED.tar.gz.age --identity AGE_PRIVATE --destination NEW_DIR [--input-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins s3-object-backup --config S3.json --object-key KEY --version-id VERSION --sha256 RAW_SHA256 --root EXISTING_CHUNK_STORE --identity AGE_ID --label LABEL [--top-namespace NS] [--json] (B-In versioned S3 / Ceph RGW / RustFS -> age chunk store)\n\
       multi-backup-native plugins dropbox-upload --archive VERIFIED.tar.gz.age --remote-path /New/Archive.tar.gz.age [--json] (B-Out Dropbox: create-only encrypted upload)\n\
       multi-backup-native plugins dropbox-backup --source LOCAL_DIR --remote-path /New/Archive.tar.gz.age --recipient AGE_PUBLIC [--json] (B-Out encrypted, no plaintext archive)\n\
       multi-backup-native plugins virtualization-inventory --config-json OWNER_ONLY_PVE_OR_PBS.json [--json] (READ-ONLY; no backup/restore)\n\
       multi-backup-native plugins hypervisor-inventory --config-json OWNER_ONLY_VMWARE_OR_OVIRT.json [--json] (VMs ONLY, READ-ONLY; no CT or backups/restores)\n\
       multi-backup-native plugins pve-start-backup --config-json OWNER_ONLY_PVE.json --node NODE --guest-kind qemu|lxc --guest-id VMID --mode snapshot|suspend|stop --storage PVE_STORAGE --confirm-remote-backup KIND:VMID:MODE:STORAGE [--json] (starts PVE vzdump task only; no verified archive)\n\
       multi-backup-native plugins pve-task-status --config-json OWNER_ONLY_PVE.json --node NODE --upid PVE_UPID [--json] (read-only status)\n\
       multi-backup-native plugins pve-backup-media --config-json OWNER_ONLY_PVE.json --node NODE --storage STORE --guest-id VMID [--json] (read-only provider backup volume metadata; NOT Multi Backup verified)\n\
       multi-backup-native plugins metadata-snapshot-backup --input cephfs|lustre|nfs|smb|glusterfs|sshfs --source EXACT_RO_SNAPSHOT_MOUNT --root STORE --identity AGE_KEY --label LABEL --source-snapshot-attested [--json] (ACL/xattr/PAX stream encrypted)\n\
       multi-backup-native plugins metadata-snapshot-restore --output PROVIDER --root STORE --identity AGE_KEY --id HASH --destination NEW_DIR [--json] (ACL/xattr create-only same-mount restore)\n\
       multi-backup-native plugins pve-restore-new-guest --root AGE_STORE --identity AGE_KEY --id BACKUP_ID --new-guest-id 9999 --storage local-lvm --stage-root EXISTING_TMP --confirm-new-guest qemu:9999:local-lvm [--json] (REAL qmrestore/pct restore create-only; no auto-start)\n\
       multi-backup-native plugins pve-media-backup --config-json PVE.json --node NODE --storage STORE --guest-kind qemu|lxc --guest-id ID --upid UPID --volid STORAGE:backup/FILE --sha256 HASH --bytes SIZE --root STORE --identity KEY --label NAME [--json] (PVE node-local verified vzdump to age chunks)\n\
       multi-backup-native plugins pve-media-restore-offline --root STORE --identity KEY --id HASH --destination NEW_DIR [--json]\n\
       multi-backup-native plugins pbs-disk-backup --snapshot-json PBS.json --root STORE --identity KEY --label NAME [--json] (official PBS client VM image to age chunks)\n\
       multi-backup-native plugins pbs-disk-restore-offline --root STORE --identity KEY --id HASH --destination NEW_DIR [--json]\n\
       multi-backup-native plugins iscsi-snapshot-backup --snapshot-json OWNER_ONLY.json --root STORE --identity AGE_KEY --label LABEL [--json] (read-only attested iSCSI LUN to age chunks)\n\
       multi-backup-native plugins iscsi-restore-image --root STORE --identity AGE_KEY --id HASH --destination NEW_DIR [--json] (offline raw image only; never writes to a LUN)\n\
       multi-backup-native plugins rbd-snapshot-backup --snapshot-json PRIVATE.json --root STORE --identity AGE_KEY --label LABEL [--json]\n\
       multi-backup-native plugins rbd-restore-new --root STORE --identity AGE_KEY --id HASH --pool POOL --image NEW --confirm-new-image POOL/NEW [--json]\n\
       multi-backup-native plugins oracle-rman-media-backup --pieces RMAN_PIECES_DIR --root EXISTING_AGE_STORE --identity AGE_PRIVATE --label NAME [--json] (native encrypted RMAN piece importer; RMAN job executed separately)\n\
       multi-backup-native plugins oracle-rman-media-restore --root EXISTING_AGE_STORE --identity AGE_PRIVATE --id HASH --destination NEW_DIR [--json] (create-only piece recovery; separately validate via RMAN)\n\
       multi-backup-native plugins pbs-snapshots --config-json OWNER_ONLY_PBS.json --datastore DATASTORE [--namespace NS] [--json] (read-only native VM/CT/host snapshots; PBS verification is not Multi Backup verification)\n\
       multi-backup-native plugins plan --input PLUGIN --output PLUGIN --mode backup|restore [--input-lvm MODE] [--output-lvm MODE] [--input-gpfs-access ces-nfs|ces-smb] [--output-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins database-backup --profile-json DB.json --root STORE --identity AGE_KEY --label NAME [--json]\n\
       multi-backup-native plugins database-restore-new --profile-json NEW_TARGET.json --root STORE --identity AGE_KEY --id BACKUP_ID --confirm-target ENGINE:HOST:PORT:NAME [--json]\n\
       multi-backup-native plugins afs-backup --job-json APPROVED_VOLUME_JOB.json --root STORE --identity AGE_KEY --label NAME [--max-transfer-bps RATE] [--json]\n\
       multi-backup-native plugins afs-restore-plan --target-json NEW_VOLUME_TARGET.json [--json]\n\
       multi-backup-native plugins afs-restore-new --target-json NEW_VOLUME_TARGET.json --root STORE --identity AGE_KEY --id BACKUP_ID --confirm-target CELL:SERVER:/vicepXX:NEWNAME [--max-transfer-bps RATE] [--json]\n\
       multi-backup-native plugins probe --plugin gpfs|sshfs|webdav|nfs|smb|cephfs|glusterfs|lustre --mount PREMOUNTED_DIR [--access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins backup --input posix|nfs|smb|sshfs|gpfs|webdav|cephfs|glusterfs|lustre --output posix --source DIR --root STORE --identity AGE_KEY --label NAME [--namespace NAME] [--top-namespace PATH] [--source-snapshot-attested] [--davfs-cache-not-on-remote] [--input-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native plugins restore --input posix --output posix|nfs|smb|gpfs|sshfs|cephfs|glusterfs|lustre --root STORE --identity AGE_KEY --id HASH --destination NEW_DIR [--selection PATH] [--output-gpfs-access ces-nfs|ces-smb] [--json]\n\
       multi-backup-native schedule run --config JOBS.json --state PRIVATE_STATE.json [--force] [--json]\n\
       multi-backup-native schedule daemon --config JOBS.json --state PRIVATE_STATE.json [--poll-seconds 60]\n\
       multi-backup-native storage init --root DIR --recipient age1KEY [--chunk-mib 4]\n\
       multi-backup-native storage backup --root DIR --identity IDENTITY --label NAME --sources-json SOURCES.json [--mode full|incremental] [--parent BACKUP_ID] [--top-namespace PATH]\n\
       multi-backup-native storage info --root DIR --identity IDENTITY --id HASH [--json]\n\
       multi-backup-native storage verify-chain --root DIR --identity IDENTITY --id HASH [--no-chain] [--json]\n\
       multi-backup-native storage delete-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH (automatically rebase dependents)\n\
       multi-backup-native storage gc-preview --root DIR --identity IDENTITY [--grace-hours 48] [--json]\n\
       multi-backup-native storage gc-collect --root DIR --identity IDENTITY --confirm ERASE-ORPHANS [--grace-hours 48] [--json]\n\
       multi-backup-native storage retention-preview --root DIR --identity IDENTITY [--days 30] [--keep-last 1] [--json]\n\
       multi-backup-native storage list --root DIR --identity IDENTITY [--json]\n\
       multi-backup-native storage verify --root DIR --identity IDENTITY --id HASH\n\
       multi-backup-native storage protect-backup --root DIR --identity IDENTITY --id HASH\n\
       multi-backup-native storage unprotect-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH\n\
       multi-backup-native storage delete-backup --root DIR --identity IDENTITY --id HASH --confirm-id HASH\n\
       multi-backup-native storage migrate-legacy --root DIR --identity IDENTITY --label NAME --archives-json SOURCES.json\n\
       multi-backup-native storage export-legacy --root DIR --identity IDENTITY --id HASH --destination NEW.tar.gz.age\n\
       multi-backup-native storage export-zip --root DIR --identity IDENTITY --id HASH --destination NEW_DIR [--volume-mib 2048]\n\
       multi-backup-native storage import-zip --root NEW_DIR --identity IDENTITY --bundle PORTABLE_DIR\n\
       multi-backup-native storage relocate --root SOURCE --identity IDENTITY --destination NEW_DIR [--dry-run]\n\
       multi-backup-native storage restore --root DIR --identity IDENTITY --id HASH --destination NEW_DIR [--selection PATH]\n\
       multi-backup-native archive create --source DIR --output FILE.tar.gz.age --recipient age1... [--prefix multi-backup]\n\
       multi-backup-native archive list --archive FILE.tar.gz.age --identity AGE_KEY [--json]\n\
       multi-backup-native archive verify --archive FILE.tar.gz.age --identity AGE_KEY\n\
       multi-backup-native archive verify --archive FILE.tar.gz.age --vault PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native archive extract --archive FILE.tar.gz.age --identity AGE_KEY --destination NEW_DIR\n\
       multi-backup-native archive delete --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age --confirm EXACT_FILENAME\n\
       multi-backup-native archive protect --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age\n\
       multi-backup-native archive unprotect --root PRIVATE_BACKUP_ROOT --archive FILE.tar.gz.age --confirm EXACT_FILENAME\n\
       multi-backup-native checkpoint audit --file ACTIVE.json [--json]\n\
       multi-backup-native journal start --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal resume --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native journal record --directory STATE_DIR --config CONFIG.json --step STEP --file ARCHIVE\n\
       multi-backup-native journal finish --directory STATE_DIR --config CONFIG.json\n\
       multi-backup-native dropbox hash --file LOCAL_FILE\n\
       multi-backup-native dropbox backup --output-dir PRIVATE_DIR --recipient age1KEY [--remote-path /folder]\n\
       multi-backup-native dropbox resume --output-dir PRIVATE_DIR --recipient age1KEY [--remote-path /folder]\n\
       multi-backup-native github backup --output-dir PRIVATE_DIR --recipient age1KEY [--max-runs 5]\n\
       multi-backup-native github resume --output-dir PRIVATE_DIR --recipient age1KEY [--max-runs 5]\n\
       multi-backup-native restore github --archive ARCHIVE --identity AGE_KEY --target OWNER/NEW [--visibility private] --confirm CREATE-NEW\n\
       multi-backup-native restore github --archive ARCHIVE --vault PROTECTED.agevault --target OWNER/NEW --confirm CREATE-NEW [--password-stdin]\n\
       multi-backup-native restore dropbox --archive ARCHIVE --identity AGE_KEY --path /new/file --confirm CREATE-NEW\n\
       multi-backup-native s3-vault preflight|backup|verify|restore --config CONFIG.json [--source DIR] [--id ID --identity AGE_KEY --manifest-version PINNED_VERSION --destination NEW_DIR]\n\
       multi-backup-native vault-store init --root EXISTING_DIR --recipient age1PUBLIC\n\
       multi-backup-native vault-store backup --root VAULT_DIR --source DIR [--json]\n\
       multi-backup-native vault-store list|status --root VAULT_DIR [--json]\n\
       multi-backup-native vault-store verify --root VAULT_DIR --id SNAPSHOT_ID --identity AGE_KEY [--json]\n\
       multi-backup-native vault-store restore --root VAULT_DIR --id SNAPSHOT_ID --identity AGE_KEY --destination NEW_DIR [--json]\n\
       multi-backup-native vault verify --file PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native vault create --file NEW.agevault [--password-stdin]\n\
       multi-backup-native vault export --file SOURCE.agevault --destination RECOVERY.agevault [--password-stdin]\n\
       multi-backup-native vault import --file RECOVERY.agevault --destination NEW.agevault [--password-stdin]\n\
       multi-backup-native vault change-password --file PROTECTED.agevault [--password-stdin]\n\
       multi-backup-native updates check --channel early-release|stable\n\
       multi-backup-native updates stage --channel early-release|stable --directory PRIVATE_UPDATE_DIR\n\
     Safety: explicit identity required, encrypted output only, no overwriting.\n"
}

fn options(args: &[String]) -> Result<(BTreeMap<&str, &str>, bool, bool), String> {
    let mut parsed = BTreeMap::new();
    let mut json = false;
    let mut password_stdin = false;
    let mut pos = 0;
    while pos < args.len() {
        if args[pos] == "--password-stdin" {
            if password_stdin { return Err("Duplicate password input flag".into()); }
            password_stdin = true;
            pos += 1;
            continue;
        }
        if ["--dry-run","--source-snapshot-attested","--davfs-cache-not-on-remote","--no-chain","--rebase-dependents","--force"].contains(&args[pos].as_str()) {
            let flag=args[pos].as_str();
            if parsed.insert(flag,"true").is_some() {
                return Err(format!("Duplicate {flag} flag"));
            }
            pos += 1;
            continue;
        }
        if args[pos] == "--json" {
            if json { return Err("Duplicate --json flag".into()); }
            json = true;
            pos += 1;
            continue;
        }
        let key = args[pos].as_str();
        if !key.starts_with("--") || pos + 1 >= args.len() {
            return Err(format!("Expected --flag value, found {key}"));
        }
        let value = args[pos + 1].as_str();
        if value.starts_with("--") || parsed.insert(key, value).is_some() {
            return Err(format!("Missing or duplicate value for {key}"));
        }
        pos += 2;
    }
    Ok((parsed, json, password_stdin))
}
fn required<'a>(args: &'a BTreeMap<&str, &str>, flag: &str) -> Result<&'a str, String> {
    args.get(flag).copied().ok_or_else(|| format!("Missing {flag}"))
}
fn accepted(args: &BTreeMap<&str, &str>, known: &[&str]) -> Result<(), String> {
    for key in args.keys() {
        if !known.contains(key) {
            return Err(format!("Unknown option {key}"));
        }
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("multi-backup-native {}-r057-native-preview (build {})",env!("CARGO_PKG_VERSION"),match option_env!("GITHUB_SHA"){Some(v)=>v,None=>"local"});
        return Ok(());
    }
    if args.len() < 2 { return Err(usage().into()); }
    let family = args[0].as_str();
    let command = args[1].as_str();
    let (flags, json, password_stdin) = options(&args[2..])?;
    if family=="plugins" && command=="metadata-snapshot-backup" {
        if password_stdin {return Err("Metadata backup requires file-based age identity".into());}
        accepted(&flags,&["--input","--source","--root","--identity","--label",
            "--source-snapshot-attested"])?;
        let result=fs_metadata_archive::backup(required(&flags,"--input")?,
            Path::new(required(&flags,"--source")?),
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?,
            flags.contains_key("--source-snapshot-attested"))?;
        if json{println!("{}",serde_json::to_string_pretty(&result)?);}
        else{println!("Verified ACL/xattr metadata archive: {}",result.backup_id);}
        return Ok(());
    }
    if family=="plugins" && command=="metadata-snapshot-restore" {
        if password_stdin {return Err("Metadata restore requires age identity".into());}
        accepted(&flags,&["--output","--root","--identity","--id","--destination"])?;
        let result=fs_metadata_archive::restore_new(required(&flags,"--output")?,
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,
            Path::new(required(&flags,"--destination")?))?;
        if json{println!("{}",serde_json::to_string_pretty(&result)?);}
        else{println!("Restored ACL/xattr metadata archive to fresh mount: {}",result.backup_id);}
        return Ok(());
    }
    if family=="plugins" && command=="pve-restore-new-guest" {
        if password_stdin{return Err("PVE restore requires age identity file".into());}
        accepted(&flags,&["--root","--identity","--id","--new-guest-id",
            "--storage","--stage-root","--confirm-new-guest"])?;
        let new_id:u32=required(&flags,"--new-guest-id")?.parse()?;
        let report=pve_guest_restore::restore_new_guest(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,new_id,
            required(&flags,"--storage")?,
            Path::new(required(&flags,"--stage-root")?),
            required(&flags,"--confirm-new-guest")?)?;
        if json{println!("{}",serde_json::to_string_pretty(&report)?);}
        else{println!("New PVE {} guest {} created (not boot-tested)",report.guest_kind,report.new_guest_id);}
        return Ok(());
    }
    if family=="plugins" && command=="pve-media-backup" {
        if password_stdin{return Err("PVE requires owner-only config and age identity".into());}
        accepted(&flags,&["--config-json","--node","--storage","--guest-kind",
          "--guest-id","--upid","--volid","--sha256","--bytes","--root","--identity","--label"])?;
        let cfg=virtualization_inventory::Config::load(
            Path::new(required(&flags,"--config-json")?))?;
        let id:u32=required(&flags,"--guest-id")?.parse()?;
        let bytes:u64=required(&flags,"--bytes")?.parse()?;
        let r=pve_media::backup(&cfg,required(&flags,"--node")?,
            required(&flags,"--storage")?,required(&flags,"--guest-kind")?,
            id,required(&flags,"--upid")?,required(&flags,"--volid")?,
            required(&flags,"--sha256")?,bytes,
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?)?;
        if json{println!("{}",serde_json::to_string_pretty(&r)?);}
        else{println!("PVE vzdump encrypted media verified: {}",r.id);}
        return Ok(());
    }
    if family=="plugins" && command=="pve-media-restore-offline" {
        if password_stdin{return Err("PVE offline restore needs age identity".into());}
        accepted(&flags,&["--root","--identity","--id","--destination"])?;
        let r=pve_media::restore_offline(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,
            Path::new(required(&flags,"--destination")?))?;
        if json{println!("{}",serde_json::to_string_pretty(&r)?);}
        else{println!("PVE verified archive recovered offline: {}",r.id);}
        return Ok(());
    }
    if family=="plugins" && command=="pbs-disk-backup" {
        if password_stdin{return Err("PBS requires external owner-only scoped secret file".into());}
        accepted(&flags,&["--snapshot-json","--root","--identity","--label"])?;
        let source=pbs_native::Source::load(
            Path::new(required(&flags,"--snapshot-json")?))?;
        let r=pbs_native::backup(&source,Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),required(&flags,"--label")?)?;
        if json{println!("{}",serde_json::to_string_pretty(&r)?);}
        else{println!("PBS native disk stream encrypted and verified: {}",r.id);}
        return Ok(());
    }
    if family=="plugins" && command=="pbs-disk-restore-offline" {
        if password_stdin{return Err("PBS offline recovery accepts an age identity".into());}
        accepted(&flags,&["--root","--identity","--id","--destination"])?;
        let r=pbs_native::restore_offline(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,
            Path::new(required(&flags,"--destination")?))?;
        if json{println!("{}",serde_json::to_string_pretty(&r)?);}
        else{println!("PBS disk image restored offline: {}",r.id);}
        return Ok(());
    }
    if family=="plugins" && command=="iscsi-snapshot-backup" {
        if password_stdin{return Err("iSCSI uses owner-only snapshot attestation".into());}
        accepted(&flags,&["--snapshot-json","--root","--identity","--label"])?;
        let snapshot=iscsi_snapshot::Snapshot::load(
            Path::new(required(&flags,"--snapshot-json")?))?;
        let item=iscsi_snapshot::backup(&snapshot,
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?)?;
        if json{println!("{}",serde_json::to_string_pretty(&item)?);}
        else{println!("Verified read-only iSCSI LUN encrypted: {}",item.backup_id);}
        return Ok(());
    }
    if family=="plugins" && command=="iscsi-restore-image" {
        if password_stdin{return Err("iSCSI offline image restore requires age key".into());}
        accepted(&flags,&["--root","--identity","--id","--destination"])?;
        let item=iscsi_snapshot::restore_new_local(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,
            Path::new(required(&flags,"--destination")?))?;
        if json{println!("{}",serde_json::to_string_pretty(&item)?);}
        else{println!("Encrypted iSCSI image restored into a NEW local directory: {}",item.backup_id);}
        return Ok(());
    }
    if family=="plugins" && command=="rbd-snapshot-backup" {
        if password_stdin{return Err("RBD requires owner-only snapshot configuration".into());}
        accepted(&flags,&["--snapshot-json","--root","--identity","--label"])?;
        let pinned=rbd_native::Snapshot::load(Path::new(required(&flags,"--snapshot-json")?))?;
        let result=rbd_native::backup(&pinned,
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?)?;
        if json {println!("{}",serde_json::to_string_pretty(&result)?);}
        else {println!("Pinned RBD snapshot encrypted: {}",result.backup_id);}
        return Ok(());
    }
    if family=="plugins" && command=="rbd-restore-new" {
        if password_stdin{return Err("RBD restore requires an exact NEW image confirmation".into());}
        accepted(&flags,&["--root","--identity","--id","--pool","--image","--confirm-new-image"])?;
        let result=rbd_native::restore_new(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,required(&flags,"--pool")?,
            required(&flags,"--image")?,required(&flags,"--confirm-new-image")?)?;
        if json {println!("{}",serde_json::to_string_pretty(&result)?);}
        else {println!("NEW RBD image independently verified: {}",result.sha256);}
        return Ok(());
    }
    if family=="plugins" && command=="oracle-rman-media-backup" {
        if password_stdin {return Err("Oracle provider accepts only an age identity file".into());}
        accepted(&flags,&["--pieces","--root","--identity","--label"])?;
        let receipt=oracle_rman::backup_pieces(
            Path::new(required(&flags,"--pieces")?),
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?)?;
        if json {println!("{}",serde_json::to_string_pretty(&receipt)?);}
        else {println!("Oracle RMAN encrypted media set verified: {}",receipt.id);}
        return Ok(());
    }
    if family=="plugins" && command=="oracle-rman-media-restore" {
        if password_stdin {return Err("Oracle provider accepts only an age identity file".into());}
        accepted(&flags,&["--root","--identity","--id","--destination"])?;
        let receipt=oracle_rman::restore_pieces(
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--id")?,
            Path::new(required(&flags,"--destination")?))?;
        if json {println!("{}",serde_json::to_string_pretty(&receipt)?);}
        else {println!("Restored {} verified Oracle RMAN media pieces to NEW path",receipt.pieces);}
        return Ok(());
    }
    if family=="plugins" && command=="ssh-backup" {
        if password_stdin {return Err("SSH requires a pinned external key file, not console password".into());}
        accepted(&flags,&["--config","--root","--identity","--label","--top-namespace"])?;
        plugins::plan(plugins::Mode::Backup,"ssh","posix")?;
        let cfg=ssh_input::Config::load(Path::new(required(&flags,"--config")?))?;
        let backup=ssh_input::backup_file(&cfg,
            Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?,
            flags.get("--top-namespace").copied().unwrap_or(""))?;
        if json {println!("{}",serde_json::to_string_pretty(&backup)?);}
        else {println!("Pinned SSH input encrypted and verified: {}",backup.id);}
        return Ok(());
    }
    if family=="plugins" && command=="webdav-http-backup" {
        if password_stdin {return Err("WebDAV credentials must use environment variable".into());}
        accepted(&flags,&["--config","--source","--filename","--recipient","--identity"])?;
        let cfg=webdav_output::Config::load(Path::new(required(&flags,"--config")?))?;
        let receipt=webdav_output::backup_directory(cfg,
            Path::new(required(&flags,"--source")?),
            required(&flags,"--filename")?,required(&flags,"--recipient")?,
            Path::new(required(&flags,"--identity")?))?;
        if json {println!("{}",serde_json::to_string_pretty(&receipt)?);}
        else {println!("WebDAV encrypted upload confirmed by independent HTTPS GET: {}",receipt.remote_file);}
        return Ok(());
    }
    if family=="plugins" && command=="webdav-http-restore" {
        if password_stdin {return Err("WebDAV credentials must use environment variable".into());}
        accepted(&flags,&["--config","--filename","--identity","--destination"])?;
        let cfg=webdav_output::Config::load(Path::new(required(&flags,"--config")?))?;
        let members=webdav_output::restore_archive(cfg,
            required(&flags,"--filename")?,Path::new(required(&flags,"--identity")?),
            Path::new(required(&flags,"--destination")?))?;
        if json {println!("{}",serde_json::json!({"restored_members":members,"provider":"webdav"}));}
        else {println!("WebDAV remote archive authenticated, {} members restored into NEW destination",members);}
        return Ok(());
    }
    if family=="plugins" && command=="mounted-backup" {
        if password_stdin {return Err("Mounted B-Out accepts only age recipient and separate key path".into());}
        accepted(&flags,&["--output","--mount","--source","--filename","--recipient",
                           "--identity","--output-gpfs-access"])?;
        let output=required(&flags,"--output")?;
        let access=flags.get("--output-gpfs-access")
            .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
        let result=mounted_output::backup_to_mount(output,
            Path::new(required(&flags,"--source")?),
            Path::new(required(&flags,"--mount")?),
            required(&flags,"--filename")?,
            required(&flags,"--recipient")?,
            Path::new(required(&flags,"--identity")?),access)?;
        if json {println!("{}",serde_json::to_string_pretty(&result)?);}
        else {println!("Encrypted mounted {} output, verified: {}",output,result.archive);}
        return Ok(());
    }
    if family=="plugins" && command=="mounted-restore" {
        if password_stdin {return Err("Mounted recovery requires a separate age identity path".into());}
        accepted(&flags,&["--input","--mount","--filename","--identity",
                           "--destination","--input-gpfs-access"])?;
        let input=required(&flags,"--input")?;
        let access=flags.get("--input-gpfs-access")
            .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
        let count=mounted_output::restore_from_mount(input,
            Path::new(required(&flags,"--mount")?),
            required(&flags,"--filename")?,
            Path::new(required(&flags,"--identity")?),
            Path::new(required(&flags,"--destination")?),access)?;
        if json {println!("{}",serde_json::json!({"restored_members":count,"input":input}));}
        else {println!("Restored {} verified encrypted members from {} to a NEW target",count,input);}
        return Ok(());
    }
    // Dedicated S3 B-In and Dropbox B-Out adapters. The generic filesystem
    // dispatcher remains intentionally locked to its tested mount protocols.
    if family=="plugins" && command=="s3-object-backup" {
        if password_stdin {return Err("S3 input accepts only AWS environment credentials".into());}
        accepted(&flags,&["--config","--object-key","--version-id","--sha256",
            "--root","--identity","--label","--top-namespace"])?;
        let _=plugins::plan(plugins::Mode::Backup,"s3","posix")?;
        let cfg=s3_vault::Config::load(Path::new(required(&flags,"--config")?))?;
        let result=s3_vault::backup_object_to_store(&cfg,
            required(&flags,"--object-key")?,required(&flags,"--version-id")?,
            required(&flags,"--sha256")?,Path::new(required(&flags,"--root")?),
            Path::new(required(&flags,"--identity")?),
            required(&flags,"--label")?,
            flags.get("--top-namespace").copied().unwrap_or(""))?;
        if json {println!("{}",serde_json::to_string_pretty(&result)?);}
        else {println!("S3 versioned object imported into verified age chunks: {}",result.id);}
        return Ok(());
    }
    if family=="plugins" && matches!(command,"dropbox-upload"|"dropbox-backup"){
        if password_stdin {return Err("Dropbox access token comes only from environment".into());}
        let _=plugins::plan(plugins::Mode::Backup,"posix","dropbox")?;
        let remote=required(&flags,"--remote-path")?;
        let receipt=if command=="dropbox-upload" {
            accepted(&flags,&["--archive","--remote-path"])?;
            dropbox_client::upload_encrypted_archive(
                Path::new(required(&flags,"--archive")?),remote)?
        }else{
            accepted(&flags,&["--source","--remote-path","--recipient"])?;
            dropbox_client::backup_directory_to_dropbox(
                Path::new(required(&flags,"--source")?),remote,
                required(&flags,"--recipient")?)?
        };
        if json {println!("{}",serde_json::to_string_pretty(&receipt)?);}
        else {println!("Dropbox encrypted create-only output verified: {}",receipt["id"]);}
        return Ok(());
    }
    if family == "s3-vault" {
        if password_stdin {return Err("S3 vault never receives account passwords".into());}
        let config=s3_vault::Config::load(Path::new(required(&flags,"--config")?))?;
        match command {
            "preflight" => {
                accepted(&flags,&["--config"])?;
                s3_vault::preflight(&config)?;
                println!("S3 bucket versioning + default COMPLIANCE Object Lock preflight: PASS");
            },
            "backup" => {
                accepted(&flags,&["--config","--source"])?;
                let item=s3_vault::backup(&config,Path::new(required(&flags,"--source")?))?;
                println!("{}",serde_json::to_string_pretty(&item)?);
            },
            "verify" => {
                accepted(&flags,&["--config","--id","--identity","--manifest-version"])?;
                let id=required(&flags,"--id")?;
                let key=Path::new(required(&flags,"--identity")?);
                let item=if let Some(version)=flags.get("--manifest-version"){
                    s3_vault::verify_at_version(&config,id,version,key)?
                }else{s3_vault::verify(&config,id,key)?};
                println!("{}",serde_json::to_string_pretty(&item)?);
            },
            "restore" => {
                accepted(&flags,&["--config","--id","--identity","--destination","--manifest-version"])?;
                let id=required(&flags,"--id")?;
                let key=Path::new(required(&flags,"--identity")?);
                let dest=Path::new(required(&flags,"--destination")?);
                let item=if let Some(version)=flags.get("--manifest-version"){
                    s3_vault::restore_at_version(&config,id,version,key,dest)?
                }else{s3_vault::restore(&config,id,key,dest)?};
                println!("{}",serde_json::to_string_pretty(&item)?);
            },
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "vault-store" {
        if password_stdin { return Err("Vault store does not accept plaintext passwords".into()); }
        match command {
            "init" => {
                accepted(&flags,&["--root","--recipient"])?;
                immutable_vault::init(Path::new(required(&flags,"--root")?),
                    required(&flags,"--recipient")?)?;
                println!("Created an APPEND-ONLY PREVIEW vault (external WORM NOT VERIFIED)");
            },
            "backup" => {
                accepted(&flags,&["--root","--source"])?;
                let item=immutable_vault::backup(
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--source")?))?;
                println!("{}",serde_json::to_string_pretty(&item)?);
            },
            "status" => {
                accepted(&flags,&["--root"])?;
                println!("{}",immutable_vault::status(Path::new(required(&flags,"--root")?))?);
            },
            "list" => {
                accepted(&flags,&["--root"])?;
                println!("{}",serde_json::to_string_pretty(
                    &immutable_vault::list(Path::new(required(&flags,"--root")?))?)?);
            },
            "verify" => {
                accepted(&flags,&["--root","--id","--identity"])?;
                let record=immutable_vault::verify(
                    Path::new(required(&flags,"--root")?),required(&flags,"--id")?,
                    Path::new(required(&flags,"--identity")?))?;
                println!("{}",serde_json::to_string_pretty(&record)?);
            },
            "restore" => {
                accepted(&flags,&["--root","--id","--identity","--destination"])?;
                let record=immutable_vault::restore(
                    Path::new(required(&flags,"--root")?),required(&flags,"--id")?,
                    Path::new(required(&flags,"--identity")?),
                    Path::new(required(&flags,"--destination")?))?;
                println!("{}",serde_json::to_string_pretty(&record)?);
            },
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "plugins" {
        if password_stdin { return Err("Plugin catalog does not accept passwords".into()); }
        match command {
            "pbs-snapshots" => {
                accepted(&flags,&["--config-json","--datastore","--namespace"])?;
                let cfg=virtualization_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let inventory=virtualization_inventory::pbs_snapshots(&cfg,
                    required(&flags,"--datastore")?,
                    flags.get("--namespace").copied().unwrap_or(""))?;
                if json {println!("{}",serde_json::to_string_pretty(&inventory)?);}
                else {println!("PBS {}: {} native snapshots (READ-ONLY; Multibackup recovery NOT verified)",
                    inventory.datastore,inventory.snapshots.len());}
            },
            "pve-backup-media" => {
                accepted(&flags,&["--config-json","--node","--storage","--guest-id"])?;
                let cfg=virtualization_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let vmid:u32=required(&flags,"--guest-id")?.parse()?;
                let media=virtualization_inventory::pve_backup_media(&cfg,
                    required(&flags,"--node")?,required(&flags,"--storage")?,vmid)?;
                if json {println!("{}",serde_json::to_string_pretty(&media)?);}
                else {println!("PVE storage {}: {} backup volumes for guest {} (provider metadata only, NOT verified archive)",
                    media.storage,media.media.len(),vmid);}
            },
            "pve-start-backup" => {
                accepted(&flags,&["--config-json","--node","--guest-kind",
                    "--guest-id","--mode","--storage","--confirm-remote-backup"])?;
                let cfg=virtualization_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let id:u32=required(&flags,"--guest-id")?.parse()?;
                let item=virtualization_inventory::start_pve_backup(&cfg,
                    required(&flags,"--node")?,required(&flags,"--guest-kind")?,
                    id,required(&flags,"--mode")?,required(&flags,"--storage")?,
                    required(&flags,"--confirm-remote-backup")?)?;
                if json {println!("{}",serde_json::to_string_pretty(&item)?);}
                else {println!("PVE vzdump task accepted: {} (not a completed or verified backup)",item.upid);}
            },
            "pve-task-status" => {
                accepted(&flags,&["--config-json","--node","--upid"])?;
                let cfg=virtualization_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let item=virtualization_inventory::pve_backup_task_status(&cfg,
                    required(&flags,"--node")?,required(&flags,"--upid")?)?;
                if json {println!("{}",serde_json::to_string_pretty(&item)?);}
                else {println!("PVE task {}: {} (remote success: {}, archive not verified)",
                    item.upid,item.state,item.remote_task_ok);}
            },
            "hypervisor-inventory" => {
                accepted(&flags,&["--config-json"])?;
                let cfg=hypervisor_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let guests=hypervisor_inventory::inventory(&cfg)?;
                if json {println!("{}",serde_json::to_string_pretty(&guests)?);}
                else {println!("{}: {} native VMs (READ-ONLY, incomplete inventory possible, backup/restore NOT certified)",
                    guests.provider,guests.guests.len());}
            },
            "virtualization-inventory" => {
                accepted(&flags,&["--config-json"])?;
                let cfg=virtualization_inventory::Config::load(
                    Path::new(required(&flags,"--config-json")?))?;
                let info=virtualization_inventory::inventory(&cfg)?;
                if json {
                    println!("{}",serde_json::to_string_pretty(&info)?);
                } else {
                    println!("{}: {} inventoried native resources (READ-ONLY, backup and restore NOT certified)",
                        info.provider,info.items.len());
                    for item in info.items {
                        println!("{}\t{}\t{}\t{}",
                            item.kind,item.id,item.name,
                            item.power_state.unwrap_or_else(||"n/a".into()));
                    }
                }
            },
            "list" => {
                accepted(&flags, &["--role", "--mode"])?;
                let mode = plugins::Mode::parse(flags.get("--mode").copied().unwrap_or("backup"))?;
                let list = match flags.get("--role").copied() {
                    None => plugins::CATALOG.iter().collect::<Vec<_>>(),
                    Some("input") => plugins::catalog_for(mode, true),
                    Some("output") => plugins::catalog_for(mode, false),
                    _ => return Err("Role must be input or output".into()),
                };
                if json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for p in list {
                        let input = p.input_stage(mode);
                        let output = p.output_stage(mode);
                        println!("{}\t{}\t{:?}\tinput={:?}\toutput={:?}",
                                 p.id, p.title, p.kind, input, output);
                    }
                }
            },
            "plan" => {
                accepted(&flags, &["--input", "--output", "--mode",
                                    "--input-lvm", "--output-lvm",
                                    "--input-gpfs-access","--output-gpfs-access"])?;
                let mode = plugins::Mode::parse(required(&flags, "--mode")?)?;
                let input_lvm = flags.get("--input-lvm").map(|v|plugins::LvmMode::parse(v))
                    .transpose()?;
                let output_lvm = flags.get("--output-lvm").map(|v|plugins::LvmMode::parse(v))
                    .transpose()?;
                let gpfs_in=flags.get("--input-gpfs-access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let gpfs_out=flags.get("--output-gpfs-access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let item = plugins::plan_profiles(mode, required(&flags,"--input")?,
                    required(&flags,"--output")?,input_lvm,output_lvm,gpfs_in,gpfs_out)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&item)?);
                } else {
                    println!("{:?}: {} -> {} (input={:?}, output={:?}); executable={}; preview_executable={}",
                             item.mode, item.source_plugin, item.destination_plugin,
                             item.source_stage, item.destination_stage, item.executable,
                             item.preview_executable);
                    println!("{}", item.reason);
                    for check in &item.required_preflight {
                        println!("preflight: {check}");
                    }
                }
            },
            "database-backup" | "database-restore-new" => {
                let backup=command=="database-backup";
                if backup {
                    accepted(&flags,&["--profile-json","--root","--identity",
                                     "--label","--max-transfer-bps"])?;
                }else{
                    accepted(&flags,&["--profile-json","--root","--identity",
                                     "--id","--confirm-target","--max-transfer-bps"])?;
                }
                let path=Path::new(required(&flags,"--profile-json")?);
                let md=std::fs::symlink_metadata(path)?;
                if !md.file_type().is_file()||md.len()>65536 {
                    return Err("Database profile must be an ordinary small file".into());
                }
                let profile:database::Profile=serde_json::from_reader(
                    std::fs::File::open(path)?)?;
                let mut limits=ExecutionLimits::default();
                if let Some(rate)=flags.get("--max-transfer-bps"){
                    limits.max_transfer_bytes_per_sec=Some(rate.parse()?);
                }
                limits.validate()?;
                let root=Path::new(required(&flags,"--root")?);
                let key=Path::new(required(&flags,"--identity")?);
                if backup{
                    let snap=database::backup(&profile,root,key,
                        required(&flags,"--label")?,&limits)?;
                    if json {println!("{}",serde_json::to_string_pretty(&snap)?);}
                    else {println!("Verified encrypted database backup: {}",snap.id);}
                }else{
                    let n=database::restore_new(&profile,root,key,
                        required(&flags,"--id")?,
                        required(&flags,"--confirm-target")?,&limits)?;
                    if json {println!("{}",serde_json::json!({
                        "new_database":profile.database,
                        "streamed_bytes":n,
                        "overwrote_existing":false
                    }));}
                    else {println!("Restored {n} bytes into NEW database {}",profile.database);}
                }
            },
            "afs-backup" => {
                accepted(&flags,&["--job-json","--root","--identity",
                                   "--label","--max-transfer-bps"])?;
                let path=Path::new(required(&flags,"--job-json")?);
                let meta=std::fs::symlink_metadata(path)?;
                if !meta.file_type().is_file() || meta.len()>65536 {
                    return Err("Unsafe AFS job configuration file".into());
                }
                let job:afs_volume::VolumeJob=serde_json::from_reader(
                    std::fs::File::open(path)?)?;
                let mut limits=ExecutionLimits::default();
                if let Some(value)=flags.get("--max-transfer-bps"){
                    limits.max_transfer_bytes_per_sec=Some(value.parse()?);
                }
                limits.validate()?;
                let backup=afs_volume::backup_volume_to_store(
                    &job,Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--label")?,&limits)?;
                if json {println!("{}",serde_json::to_string_pretty(&backup)?);}
                else {println!("AFS volume dump encrypted and verified: {}",backup.id);}
            },
            "afs-restore-plan" | "afs-restore-new" => {
                let execute=command=="afs-restore-new";
                if execute {
                    accepted(&flags,&["--target-json","--root","--identity","--id",
                        "--confirm-target","--max-transfer-bps"])?;
                }else{
                    accepted(&flags,&["--target-json"])?;
                }
                let path=Path::new(required(&flags,"--target-json")?);
                let meta=std::fs::symlink_metadata(path)?;
                if !meta.file_type().is_file()||meta.len()>65536{
                    return Err("Unsafe AFS restore profile".into());
                }
                let target:afs_volume::RestoreNewJob=serde_json::from_reader(
                    std::fs::File::open(path)?)?;
                let plan=afs_volume::plan_restore_new(&target)?;
                if execute{
                    let mut limits=ExecutionLimits::default();
                    if let Some(value)=flags.get("--max-transfer-bps"){
                        limits.max_transfer_bytes_per_sec=Some(value.parse()?);
                    }
                    limits.validate()?;
                    let (segments,bytes)=afs_volume::restore_new_chain_from_store(
                        Path::new(required(&flags,"--root")?),
                        Path::new(required(&flags,"--identity")?),
                        required(&flags,"--id")?,&target,
                        required(&flags,"--confirm-target")?,&limits)?;
                    if json {println!("{}",serde_json::json!({
                        "restored_dumps":segments,"restored_dump_bytes":bytes,
                        "new_volume":target.volume,"overwrites_existing":false
                    }));}
                    else {println!("AFS NEW volume created from {segments} verified dumps ({bytes} bytes)");}
                }else if json {println!("{}",serde_json::to_string_pretty(&plan)?);}
                else {println!("AFS new-volume restore: {} {:?}\nConfirmation: {}",
                    plan.executable,plan.arguments,plan.confirmation);}
            },
            "probe" => {
                accepted(&flags, &["--plugin","--mount","--access"])?;
                let plugin=required(&flags,"--plugin")?;
                let access=flags.get("--access")
                    .map(|v|plugins::GpfsAccess::parse(v)).transpose()?;
                let mount=Path::new(required(&flags,"--mount")?);
                let evidence=multi_backup_core::mount_probe::probe(plugin,mount,access)?;
                if json {println!("{}",serde_json::to_string_pretty(&evidence)?);}
                else {println!("{} {} fs={} rw_flag={} (read-only probe)",
                    evidence.plugin,evidence.mountpoint,evidence.fs_type,
                    evidence.rw_mount_option);}
            },
            "backup" => {
                accepted(&flags, &["--input","--output","--source","--root",
                                    "--identity","--label","--namespace","--top-namespace",
                                    "--source-snapshot-attested","--davfs-cache-not-on-remote","--input-gpfs-access",
                                    "--mode","--parent"])?;
                let kind=chunk_store::BackupKind::parse(
                    flags.get("--mode").copied().unwrap_or("full"))?;
                let snapshot = if flags.contains_key("--davfs-cache-not-on-remote") {
                    plugin_executor::backup_with_mode_davfs_cache_attested(
                    required(&flags,"--input")?,required(&flags,"--output")?,
                    Path::new(required(&flags,"--source")?),
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--label")?,
                    flags.get("--namespace").copied().unwrap_or("files"),
                    flags.get("--top-namespace").copied().unwrap_or(""),
                    flags.contains_key("--source-snapshot-attested"),
                    flags.get("--input-gpfs-access").map(|v|plugins::GpfsAccess::parse(v)).transpose()?,
                    kind,flags.get("--parent").copied())
                } else {
                    plugin_executor::backup_with_mode(
                    required(&flags,"--input")?,required(&flags,"--output")?,
                    Path::new(required(&flags,"--source")?),
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--label")?,
                    flags.get("--namespace").copied().unwrap_or("files"),
                    flags.get("--top-namespace").copied().unwrap_or(""),
                    flags.contains_key("--source-snapshot-attested"),
                    flags.get("--input-gpfs-access").map(|v|plugins::GpfsAccess::parse(v)).transpose()?,
                    kind,flags.get("--parent").copied())
                }?;
                if json { println!("{}",serde_json::to_string_pretty(&snapshot)?); }
                else {println!("Verified encrypted plugin backup {} ({} files)",
                              snapshot.id,snapshot.entries.len());}
            },
            "restore" => {
                accepted(&flags, &["--input","--output","--root","--identity",
                                    "--id","--destination","--selection","--output-gpfs-access"])?;
                let count = plugin_executor::restore_with_gpfs_access(
                    required(&flags,"--input")?,required(&flags,"--output")?,
                    Path::new(required(&flags,"--root")?),
                    Path::new(required(&flags,"--identity")?),
                    required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?),
                    flags.get("--selection").copied(),
                    flags.get("--output-gpfs-access").map(|v|plugins::GpfsAccess::parse(v)).transpose()?)?;
                if json { println!("{}",serde_json::json!({"restored_files":count})); }
                else {println!("Restored {count} verified files to a new directory");}
            },
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "schedule" {
        if password_stdin {return Err("Scheduler uses separate age identity paths".into());}
        accepted(&flags,&["--config","--state","--force","--poll-seconds"])?;
        let config=Path::new(required(&flags,"--config")?);
        let state=Path::new(required(&flags,"--state")?);
        match command {
            "run"=>{
                if flags.contains_key("--poll-seconds") {
                    return Err("Polling interval is valid only in daemon mode".into());
                }
                let now=std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let output=scheduler::run_once(config,state,flags.contains_key("--force"),now)?;
                println!("{}",serde_json::to_string_pretty(&output)?);
            },
            "daemon"=>{
                if flags.contains_key("--force"){
                    return Err("--force is only valid for a one-shot manual run".into());
                }
                let poll=flags.get("--poll-seconds").copied().unwrap_or("60").parse::<u64>()?;
                if !(10..=3600).contains(&poll){return Err("Poll seconds must be 10..3600".into());}
                loop {
                    let now=std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                    match scheduler::run_once(config,state,false,now){
                        Ok(events)=>{
                            for event in events {
                                if event.executed {
                                    println!("Scheduler {} completed; backup={:?}",
                                        event.job_id,event.backup_id);
                                }
                            }
                        },
                        Err(error)=>eprintln!("Scheduler run failed: {error}"),
                    }
                    std::thread::sleep(std::time::Duration::from_secs(poll));
                }
            },
            _=>return Err(usage().into())
        }
        return Ok(());
    }
    if family == "storage" {
        if password_stdin { return Err("Storage uses a separately supplied age identity".into()); }
        let root = Path::new(required(&flags, "--root")?);
        match command {
            "init" => {
                accepted(&flags, &["--root","--recipient","--chunk-mib"])?;
                let chunk_mib = flags.get("--chunk-mib").copied().unwrap_or("4").parse::<usize>()?;
                let bytes = chunk_mib.checked_mul(1024*1024).ok_or("Invalid chunk size")?;
                chunk_store::Store::init(root, required(&flags,"--recipient")?, bytes)?;
                println!("Initialized immutable age/Zstandard storage; legacy archives untouched");
            }
            "backup" => {
                accepted(&flags, &["--root","--identity","--label","--sources-json",
                                    "--top-namespace","--mode","--parent"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let sources_file = std::fs::File::open(required(&flags, "--sources-json")?)?;
                let values: Vec<serde_json::Value> = serde_json::from_reader(sources_file)?;
                if values.is_empty() { return Err("Sources JSON must be nonempty".into()); }
                let sources: Vec<chunk_store::Source> = values.iter().map(|value| {
                    let namespace = value["namespace"].as_str().ok_or("Missing namespace")?;
                    let path = value["directory"].as_str().ok_or("Missing directory")?;
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(chunk_store::Source {
                        namespace: namespace.to_owned(), directory: path.into(),
                    })
                }).collect::<Result<_,_>>()?;
                let kind=chunk_store::BackupKind::parse(
                    flags.get("--mode").copied().unwrap_or("full"))?;
                let snapshot = store.backup_in_namespace_mode(identity, required(&flags,"--label")?,
                    flags.get("--top-namespace").copied().unwrap_or(""), &sources,
                    kind,flags.get("--parent").copied())?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }
            "list" => {
                accepted(&flags, &["--root","--identity"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let snapshots = chunk_store::Store::open(root, identity)?.list(identity)?;
                if json {
                    let values:Vec<_>=snapshots.iter().map(|backup|{
                        let mut value=serde_json::to_value(backup)?;
                        if let Some(obj)=value.as_object_mut(){
                            obj.insert("protected".into(),serde_json::json!(backup.protected));
                        }
                        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(value)
                    }).collect::<Result<_,_>>()?;
                    println!("{}",serde_json::to_string_pretty(&values)?);
                } else {
                    for backup in &snapshots {
                        println!("{} {} {} ({} logical files)",
                            if backup.protected {"[PROTECTED]"}else{"[UNPROTECTED]"},
                            backup.id,format!("{} [{:?}]",backup.label,backup.kind),backup.entries.len());
                    }
                }
            }
            "retention-preview" => {
                accepted(&flags,&["--root","--identity","--days","--keep-last"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let policy=multi_backup_core::backup_policy::Retention {
                    days:flags.get("--days").copied().unwrap_or("30").parse()?,
                    keep_last_verified:flags.get("--keep-last").copied().unwrap_or("1").parse()?,
                    immutable_retention_enforced:true,
                };
                let now=std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let preview=chunk_store::Store::open(root,key)?
                    .preview_retention(key,&policy,now)?;
                println!("{}",serde_json::to_string_pretty(&preview)?);
            }
            "protect-backup" => {
                accepted(&flags,&["--root","--identity","--id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let changed=chunk_store::Store::open(root,key)?.protect_backup(key,id)?;
                println!("Backup {id}: {}",if changed {"protected"}else{"already protected"});
            }
            "unprotect-backup" => {
                accepted(&flags,&["--root","--identity","--id","--confirm-id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let changed=chunk_store::Store::open(root,key)?.unprotect_backup(
                    key,id,required(&flags,"--confirm-id")?)?;
                println!("Backup {id}: {}",if changed {"protection removed"}else{"not protected"});
            }
            "delete-backup" => {
                accepted(&flags,&["--root","--identity","--id","--confirm-id",
                                  "--rebase-dependents"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let id=required(&flags,"--id")?;
                let store=chunk_store::Store::open(root,key)?;
                let count=store.delete_backup_rebase(key,id,required(&flags,"--confirm-id")?)?;
                println!("Safely rebased {count} immediate descendants and purged manifest {id}. No chunks or parity deleted until separate GC.");
            }
            "gc-preview" | "gc-collect" => {
                accepted(&flags,&["--root","--identity","--grace-hours","--confirm"])?;
                let perform=command=="gc-collect";
                if perform {
                    if required(&flags,"--confirm")?!="ERASE-ORPHANS" {
                        return Err("GC execution requires explicit --confirm ERASE-ORPHANS".into());
                    }
                } else if flags.contains_key("--confirm") {
                    return Err("GC preview must never accept an execution confirmation".into());
                }
                let hours=flags.get("--grace-hours").copied().unwrap_or("48").parse()?;
                let key=Path::new(required(&flags,"--identity")?);
                let report=chunk_store::Store::open(root,key)?
                    .garbage_collect(key,hours,perform)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }
            "info" => {
                accepted(&flags,&["--root","--identity","--id"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let info=backup_chain::details(&store,key,required(&flags,"--id")?)?;
                println!("{}",serde_json::to_string_pretty(&info)?);
            }
            "verify-chain" => {
                accepted(&flags,&["--root","--identity","--id","--no-chain"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let full=!flags.contains_key("--no-chain");
                let result=backup_chain::verify_chain(&store,key,required(&flags,"--id")?,full)?;
                println!("{}",serde_json::to_string_pretty(&result)?);
                if !result.healthy{return Err("Incremental chain integrity check failed".into());}
            }
            "relocate" => {
                accepted(&flags,&["--root","--identity","--destination","--dry-run"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let dest=Path::new(required(&flags,"--destination")?);
                let dry=flags.contains_key("--dry-run");
                let report=chunk_store::Store::open(root,key)?.relocate(key,dest,dry)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
            }
            "export-legacy" => {
                accepted(&flags,&["--root","--identity","--id","--destination"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let count=chunk_store::Store::open(root,key)?.export_legacy(
                    key,required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?))?;
                println!("Stream-exported {count} verified legacy tar.gz.age members");
            }
            "migrate-legacy" => {
                accepted(&flags,&["--root","--identity","--label","--archives-json"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let source_file=std::fs::File::open(required(&flags,"--archives-json")?)?;
                let values:Vec<serde_json::Value>=serde_json::from_reader(source_file)?;
                if values.is_empty(){return Err("Legacy archive list is empty".into());}
                let sources:Vec<chunk_store::LegacySource>=values.iter().map(|v|{
                    Ok::<_,Box<dyn std::error::Error+Send+Sync>>(chunk_store::LegacySource {
                        namespace:v["namespace"].as_str().ok_or("Missing namespace")?.into(),
                        archive:v["archive"].as_str().ok_or("Missing archive path")?.into()
                    })
                }).collect::<Result<_,_>>()?;
                let store=chunk_store::Store::open(root,key)?;
                let backup=store.import_legacy(key,required(&flags,"--label")?,&sources)?;
                println!("Migrated {} files into unified logical backup {}",
                    backup.entries.len(),backup.id);
            }
            "export-zip" => {
                accepted(&flags,&["--root","--identity","--id","--destination","--volume-mib"])?;
                let key=Path::new(required(&flags,"--identity")?);
                let store=chunk_store::Store::open(root,key)?;
                let mib=flags.get("--volume-mib").copied().unwrap_or("2048").parse()?;
                let result=portable::export(&store,key,required(&flags,"--id")?,
                    Path::new(required(&flags,"--destination")?),mib)?;
                println!("Created {} portable encrypted ZIP64 volumes",result.len());
            }
            "import-zip" => {
                accepted(&flags,&["--root","--identity","--bundle"])?;
                let id=portable::import(Path::new(required(&flags,"--bundle")?),
                    Path::new(required(&flags,"--identity")?),root)?;
                println!("Imported and verified logical backup {id} into new storage");
            }
            "verify" => {
                accepted(&flags, &["--root","--identity","--id"])?;
                let identity = Path::new(required(&flags,"--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let report = store.verify(identity, required(&flags,"--id")?)?;
                println!("{}",serde_json::to_string_pretty(&report)?);
                if !report.healthy {return Err("Backup integrity or parity verification failed".into());}
            }
            "protect" | "repair" => {
                accepted(&flags, &["--root","--identity","--id"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let store = chunk_store::Store::open(root, identity)?;
                let id = required(&flags, "--id")?;
                let count = if command == "protect" {store.protect(identity,id)?}
                            else {store.repair(identity,id)?};
                println!("{}: {} stripes/chunks processed",command,count);
            }
            "restore" => {
                accepted(&flags, &["--root","--identity","--id","--selection","--destination"])?;
                let identity = Path::new(required(&flags, "--identity")?);
                let count = chunk_store::Store::open(root, identity)?.restore(
                    identity,required(&flags,"--id")?,
                    flags.get("--selection").copied(),
                    Path::new(required(&flags,"--destination")?))?;
                println!("Restored {} verified files to a fresh directory",count);
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }

    if family == "updates" {
        if password_stdin { return Err("No credentials accepted for update checks".into()); }
        if matches!(command,"activate-portable"|"rollback-portable"){
            accepted(&flags,&["--active","--candidate","--rollback","--rejected",
                "--binary","--expected-sha256"])?;
            let active=Path::new(required(&flags,"--active")?);
            let relative=Path::new(required(&flags,"--binary")?);
            let parts=relative.components();
            if relative.as_os_str().is_empty()||relative.is_absolute()||
               parts.clone().any(|c|!matches!(c,Component::Normal(_))){
                return Err("Version executable must be a safe relative path".into());
            }
            let expected=required(&flags,"--expected-sha256")?;
            if expected.len()!=64 || !expected.bytes().all(|c|c.is_ascii_hexdigit()){
                return Err("Expected SHA256 must be 64 hexadecimal characters".into());
            }
            let health=|root:&Path|->updater::UpdateResult<()> {
                let item=root.join(relative);
                let metadata=std::fs::symlink_metadata(&item)?;
                if !metadata.file_type().is_file()||metadata.file_type().is_symlink(){
                    return Err("Portable update executable must be a real file".into());
                }
                let mut input=std::fs::File::open(&item)?;
                let mut hash=Sha256::new();
                let mut buffer=[0u8;131072];
                loop {
                    let n=input.read(&mut buffer)?;
                    if n==0 {break;}
                    hash.update(&buffer[..n]);
                }
                if hex::encode(hash.finalize())!=expected.to_ascii_lowercase(){
                    return Err("Portable update executable hash differs from explicit expected digest".into());
                }
                Ok(())
            };
            match command {
                "activate-portable"=>{
                    updater::activate_portable_version(active,
                        Path::new(required(&flags,"--candidate")?),
                        Path::new(required(&flags,"--rollback")?),&health)?;
                    println!("Portable update atomically activated; previous version retained for rollback");
                }
                "rollback-portable"=>{
                    updater::rollback_portable_version(active,
                        Path::new(required(&flags,"--rollback")?),
                        Path::new(required(&flags,"--rejected")?),&health)?;
                    println!("Portable previous version restored; rejected candidate retained");
                }
                _=>unreachable!(),
            }
            return Ok(());
        }
        accepted(&flags, if command == "stage" {
            &["--channel", "--directory"] } else { &["--channel"] })?;
        let channel = required(&flags, "--channel")?;
        let info = updater::get_release(channel)?;
        match command {
            "check" => {
                if json { println!("{}", serde_json::to_string_pretty(&info)?); }
                else {
                    println!("Native Rust release: {} ({})", info.tag, info.channel);
                    println!("Matching verified asset: {}",
                        info.asset.as_ref().map_or("unavailable", |a|a.name.as_str()));
                }
            }
            "stage" => {
                let directory = Path::new(required(&flags, "--directory")?);
                let file = updater::stage(&info, directory)?;
                println!("SHA-256 verified native update staged; NOT installed: {}", file.display());
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "vault" {
        if json { return Err("--json is not supported for vault operations".into()); }
        let count = match command {
            "verify" | "create" => 1,
            "export" | "import" | "change-password" => 2,
            _ => return Err(usage().into()),
        };
        if command == "export" || command == "import" {
            accepted(&flags, &["--file", "--destination"])?;
        } else {
            accepted(&flags, &["--file"])?;
        }
        let secrets: Vec<Zeroizing<String>> = if password_stdin {
            let mut input = Zeroizing::new(Vec::new());
            io::stdin().take(4096).read_to_end(&mut *input)?;
            let text = std::str::from_utf8(&input)?;
            let lines: Vec<_> = text.lines().collect();
            if lines.len() != count {
                return Err("Expected the required number of passwords on stdin".into());
            }
            lines.iter().map(|line| Zeroizing::new(
                line.trim_end_matches('\r').to_string())).collect()
        } else {
            let mut supplied = Vec::new();
            for index in 0..count {
                let prompt = format!("Vault password {}: ", index + 1);
                supplied.push(Zeroizing::new(rpassword::prompt_password(prompt)?));
            }
            supplied
        };
        let file = Path::new(required(&flags, "--file")?);
        let recipient = match command {
            "verify" => vault::verify(file, &secrets[0])?,
            "create" => vault::create(file, &secrets[0])?,
            "export" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::export_recovery(file, to, &secrets[0], &secrets[1])?
            }
            "import" => {
                let to = Path::new(required(&flags, "--destination")?);
                vault::import_recovery(file, to, &secrets[0], &secrets[1])?
            }
            _ => vault::change_password(file, &secrets[0], &secrets[1])?,
        };
        println!("Vault {command} authenticated for recipient {recipient}; private identity not exported");
        return Ok(());
    }
    if password_stdin && family != "archive" && family != "restore" {
        return Err("--password-stdin requires vault or protected archive".into());
    }
    if family == "journal" {
        if password_stdin { return Err("--password-stdin not valid for journals".into()); }
        if !["start", "resume", "record", "finish", "restart", "show"].contains(&command) {
            return Err(usage().into());
        }
        let known = if command == "record" {
            vec!["--directory","--config","--step","--file"]
        } else {
            vec!["--directory","--config"]
        };
        accepted(&flags, &known)?;
        let dir = Path::new(required(&flags, "--directory")?);
        let configuration: serde_json::Value = serde_json::from_reader(
            std::fs::File::open(required(&flags, "--config")?)?)?;
        let start_new = command == "start" || command == "restart";
        let mut state = journal::Journal::start(dir, &configuration,
                                    start_new, !start_new)?;
        match command {
            "record" => state.record(required(&flags, "--step")?,
                                    Path::new(required(&flags, "--file")?))?,
            "finish" => state.finish()?,
            _ => {},
        }
        if json {
            println!("{}", serde_json::to_string_pretty(state.status())?);
        } else {
            println!("Journal {}: cycle={} resumed={} steps={}",
                     command, state.cycle()?, state.resumed,
                     state.status()["steps"].as_object().map_or(0, |x| x.len()));
        }
        return Ok(());
    }
    if family == "checkpoint" && command == "audit" {
        accepted(&flags, &["--file"])?;
        let state = Path::new(required(&flags, "--file")?);
        let audit = checkpoint_audit::audit(state)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&audit)?);
        } else {
            println!("Checkpoint {}: {} verified, {} invalid",
                audit.cycle, audit.valid.len(), audit.invalid.len());
        }
        if !audit.invalid.is_empty() {
            return Err("Checkpoint contains missing or modified archives".into());
        }
        return Ok(());
    }
    if family == "github" && (command == "backup" || command == "resume") {
        accepted(&flags, &["--output-dir", "--recipient", "--max-runs"])?;
        let out = Path::new(required(&flags, "--output-dir")?);
        let recipient = required(&flags, "--recipient")?;
        let limit = flags.get("--max-runs").copied().unwrap_or("5").parse::<usize>()?;
        if limit > 100_000 { return Err("CI run limit exceeds safety ceiling".into()); }
        let result = github_client::backup(out, recipient, command == "resume", limit)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if family == "restore" {
        if json { return Err("Unsupported remote restore JSON flag".into()); }
        if required(&flags, "--confirm")? != "CREATE-NEW" {
            return Err("Remote restore requires explicit --confirm CREATE-NEW".into());
        }
        let plain = flags.get("--identity").copied();
        let protected = flags.get("--vault").copied();
        if plain.is_some() == protected.is_some() {
            return Err("Specify exactly one --identity or --vault".into());
        }
        if password_stdin && protected.is_none() {
            return Err("--password-stdin requires encrypted --vault".into());
        }
        let secret = if let Some(file) = protected {
            let password = if password_stdin {
                let mut buffer = Zeroizing::new(Vec::new());
                io::stdin().take(1026).read_to_end(&mut *buffer)?;
                let line = std::str::from_utf8(&buffer)?;
                Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_owned())
            } else {
                Zeroizing::new(rpassword::prompt_password("Vault password: ")?)
            };
            Some(vault::unlock_identity(Path::new(file), &password)?)
        } else { None };
        let from = Path::new(required(&flags, "--archive")?);
        match command {
            "github" => {
                accepted(&flags, &["--archive", "--identity", "--vault", "--target", "--visibility", "--confirm",
                                    "--source-account","--target-account"])?;
                let target = required(&flags, "--target")?;
                let (target_owner,_)=remote_restore::github_slug(target)?;
                if let Some(expected)=flags.get("--target-account") {
                    if !expected.eq_ignore_ascii_case(target_owner){
                        return Err("GitHub destination account must match new repository owner".into());
                    }
                }
                if let Some(origin)=flags.get("--source-account") {
                    let _plan=account_restore::AccountRestorePlan::prepare(
                        "github",origin,target_owner,Vec::new())?;
                    // The source account is informational. gh login + owner
                    // permissions authorize writes only to the new account.
                }
                let visibility = flags.get("--visibility").copied().unwrap_or("private");
                if visibility != "private" && visibility != "public" {
                    return Err("Visibility must be private or public".into());
                }
                let url = if let Some(secret) = &secret {
                    remote_restore::github_new_repository_from_memory(
                        from, secret, target, visibility == "private")?
                } else {
                    remote_restore::github_new_repository(
                        from, Path::new(plain.unwrap()), target, visibility == "private")?
                };
                println!("New GitHub repository restored: {url}");
            },
            "dropbox" => {
                accepted(&flags, &["--archive", "--identity", "--vault", "--path", "--confirm",
                                    "--source-account","--target-account"])?;
                let path = required(&flags, "--path")?;
                let target_account=flags.get("--target-account").copied();
                if let Some(origin)=flags.get("--source-account") {
                    let destination=target_account.ok_or(
                        "Cross-user Dropbox restore requires explicit --target-account")?;
                    let _plan=account_restore::AccountRestorePlan::prepare(
                        "dropbox",origin,destination,Vec::new())?;
                }
                let result = if let Some(secret) = &secret {
                    if let Some(destination)=target_account {
                        remote_restore::dropbox_new_file_for_account_from_memory(
                            from,secret,path,destination)?
                    }else {
                        remote_restore::dropbox_new_file_from_memory(from,secret,path)?
                    }
                } else {
                    if let Some(destination)=target_account {
                        remote_restore::dropbox_new_file_for_account(
                            from,Path::new(plain.unwrap()),path,destination)?
                    }else{
                        remote_restore::dropbox_new_file(from,Path::new(plain.unwrap()),path)?
                    }
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            _ => return Err(usage().into()),
        }
        return Ok(());
    }
    if family == "dropbox" && (command == "backup" || command == "resume") {
        accepted(&flags, &["--output-dir", "--recipient", "--remote-path"])?;
        let out = Path::new(required(&flags, "--output-dir")?);
        let recipient = required(&flags, "--recipient")?;
        let remote = flags.get("--remote-path").copied().unwrap_or("");
        let result = dropbox_client::backup(out, remote, recipient, command == "resume")?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if family == "dropbox" && command == "hash" {
        if json { return Err("--json not valid for dropbox hash".into()); }
        accepted(&flags, &["--file"])?;
        let file = Path::new(required(&flags, "--file")?);
        println!("{}", dropbox_hash::hash_file(file)?);
        return Ok(());
    }
    if family == "archive" && (command == "protect" || command == "unprotect") {
        accepted(&flags,&["--root","--archive","--confirm"])?;
        if password_stdin {return Err("Archive protection uses no plaintext password input".into());}
        let root=Path::new(required(&flags,"--root")?);
        let file=Path::new(required(&flags,"--archive")?);
        let result=if command=="protect" {
            if flags.contains_key("--confirm"){return Err("--confirm not used for protection".into());}
            archive_management::protect(root,file)?
        }else{
            archive_management::unprotect(root,file,required(&flags,"--confirm")?)?
        };
        println!("Legacy archive {}: {}",
            if command=="protect"{"protection"}else{"unprotection"},
            if result {"changed"}else{"already in requested state"});
        return Ok(());
    }
    if family == "archive" && command == "delete" {
        accepted(&flags, &["--root", "--archive", "--confirm"])?;
        if password_stdin { return Err("Password input not used for local deletion".into()); }
        let root = Path::new(required(&flags, "--root")?);
        let file = Path::new(required(&flags, "--archive")?);
        let confirmation = required(&flags, "--confirm")?;
        let removed = archive_management::delete(root, file, confirmation, false)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&removed)?);
        } else {
            println!("Deleted {} local files with durable audit log", removed.len());
        }
        return Ok(());
    }
    if family != "archive" { return Err(usage().into()); }
    match command {
        "create" => {
            accepted(&flags, &["--source", "--output", "--recipient", "--prefix"])?;
            if password_stdin { return Err("No password accepted for archive creation".into()); }
            if json { return Err("--json not valid for create".into()); }
            let source = Path::new(required(&flags, "--source")?);
            let output = Path::new(required(&flags, "--output")?);
            let recipient = required(&flags, "--recipient")?;
            let prefix = flags.get("--prefix").copied().unwrap_or("multi-backup");
            let count = archive::create(source, output, recipient, prefix)?;
            println!("Created {count} verified input entries in encrypted archive {}", output.display());
        },
        "list" | "verify" | "extract" => {
            let known = if command == "extract" {
                vec!["--archive", "--identity", "--vault", "--destination"]
            } else {
                vec!["--archive", "--identity", "--vault"]
            };
            accepted(&flags, &known)?;
            if json && command != "list" {
                return Err("--json valid only for list".into());
            }
            let path = Path::new(required(&flags, "--archive")?);
            let plain = flags.get("--identity").copied();
            let protected = flags.get("--vault").copied();
            if plain.is_some() == protected.is_some() {
                return Err("Specify exactly one --identity or --vault".into());
            }
            if password_stdin && protected.is_none() {
                return Err("--password-stdin requires an encrypted --vault".into());
            }
            let unlocked = if let Some(vault_file) = protected {
                let password = if password_stdin {
                    let mut bytes = Zeroizing::new(Vec::new());
                    io::stdin().take(1026).read_to_end(&mut *bytes)?;
                    let line = std::str::from_utf8(&bytes)?;
                    Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_string())
                } else {
                    Zeroizing::new(rpassword::prompt_password("Vault password: ")?)
                };
                Some(vault::unlock_identity(Path::new(vault_file), &password)?)
            } else { None };
            let plain_path = plain.map(Path::new);
            match command {
                "list" => {
                    let members = if let Some(secret) = &unlocked {
                        archive::list_from_memory(path, secret)?
                    } else {
                        archive::list(path, plain_path.unwrap())?
                    };
                    if json {
                        println!("{}", serde_json::to_string_pretty(&members)?);
                    } else {
                        for item in members {
                            println!("{}\t{}\t{}",
                                if item.directory { "dir" } else { "file" },
                                item.bytes, item.path);
                        }
                    }
                }
                "verify" => {
                    let count = if let Some(secret) = &unlocked {
                        archive::verify_from_memory(path, secret)?
                    } else {
                        archive::verify(path, plain_path.unwrap())?
                    };
                    println!("Verified {count} members, encrypted stream integrity checked");
                }
                _ => {
                    let destination = Path::new(required(&flags, "--destination")?);
                    let members = if let Some(secret) = &unlocked {
                        archive::extract_from_memory(path, secret, destination)?
                    } else {
                        archive::extract(path, plain_path.unwrap(), destination)?
                    };
                    println!("Restored {} members safely to {}", members.len(), destination.display());
                }
            }
        },
        _ => return Err(usage().into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Multi Backup native archive error: {error}");
        process::exit(2);
    }
}

#[cfg(test)]
mod flag_tests{
    use super::*;
    #[test]
    fn dry_run_is_value_free_and_rejects_duplicate(){
        let words=vec!["--root".into(),"/tmp/data".into(),"--dry-run".into()];
        let (flags,json,password)=options(&words).unwrap();
        assert_eq!(flags.get("--dry-run"),Some(&"true"));
        assert!(!json&&!password);
        let repeated=vec!["--dry-run".into(),"--dry-run".into()];
        assert!(options(&repeated).is_err());
    }
}
