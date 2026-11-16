//! Read-only Linux mount evidence for pre-mounted NFS/SMB/GPFS CES/SSHFS.
//! No mount/umount, network, customer-file, SSH credential or write operations.
use crate::plugins::GpfsAccess;
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MountEvidence {
    pub plugin: String,
    pub mountpoint: String,
    pub fs_type: String,
    /// Kernel mount rw flag only, never an ACL or remote permission check.
    pub rw_mount_option: bool,
    pub source_redacted: bool,
    #[serde(skip_serializing)]
    source_is_https_webdav: bool,
}
#[cfg(target_os="linux")]
fn decode_mount_field(value:&str)->Option<String>{
    let src=value.as_bytes();
    let mut out=Vec::with_capacity(src.len());
    let mut i=0usize;
    while i<src.len(){
        if src[i]==b'\\' {
            if i+3>=src.len(){return None;}
            let mut byte=0u8;
            for digit in &src[i+1..i+4]{
                if !(b'0'..=b'7').contains(digit){return None;}
                byte=byte*8+(digit-b'0');
            }
            out.push(byte);
            i+=4;
        }else{
            out.push(src[i]);i+=1;
        }
    }
    String::from_utf8(out).ok()
}
#[cfg(target_os="linux")]
fn parse_mountinfo(raw:&str)->Vec<MountEvidence>{
    let mut entries=Vec::new();
    for line in raw.lines(){
        let Some((lhs,rhs))=line.split_once(" - ") else {continue;};
        let left:Vec<_>=lhs.split_whitespace().collect();
        let right:Vec<_>=rhs.split_whitespace().collect();
        if left.len()<6||right.len()<2{continue;}
        let Some(mountpoint)=decode_mount_field(left[4]) else {continue;};
        entries.push(MountEvidence{
            plugin:String::new(),mountpoint,
            fs_type:right[0].to_string(),
            rw_mount_option:left[5].split(',').any(|v|v=="rw"),
            source_redacted:true,
            source_is_https_webdav:right[1].starts_with("https://"),
        });
    }
    entries
}
/// Inspect the exact Linux mountpoint. Successful evidence does NOT
/// authenticate SSH host keys, verify the GPFS backend or prove consistency.
pub fn probe(plugin:&str,mountpoint:&Path,access:Option<GpfsAccess>)
    ->Result<MountEvidence,String>{
    if !matches!(plugin,"gpfs"|"sshfs"|"nfs"|"smb"|"webdav"|"cephfs"|"glusterfs"|"lustre"){
        return Err("Unsupported filesystem source; use nfs/smb/gpfs/sshfs/cephfs/glusterfs/lustre".into());
    }
    if plugin=="gpfs"&&access.is_none(){
        return Err("GPFS requires explicit ces-nfs or ces-smb mode".into());
    }
    if plugin!="gpfs"&&access.is_some(){
        return Err("Only GPFS may receive a CES access setting".into());
    }
    #[cfg(not(target_os="linux"))]{
        let _=(mountpoint,access);
        return Err("Mount probing is currently Linux-only and never mounts devices".into());
    }
    #[cfg(target_os="linux")]{
        if !mountpoint.is_absolute(){return Err("Mountpoint must be absolute".into());}
        let meta=std::fs::symlink_metadata(mountpoint)
            .map_err(|_|"Mountpoint not found".to_string())?;
        if meta.file_type().is_symlink()||!meta.is_dir(){
            return Err("Mountpoint must be a real directory, not a symlink".into());
        }
        let canonical=std::fs::canonicalize(mountpoint)
            .map_err(|_|"Cannot resolve mountpoint".to_string())?;
        let location=canonical.to_str().ok_or("Mountpoint must be UTF-8")?;
        let mountinfo=std::fs::read_to_string("/proc/self/mountinfo")
            .map_err(|_|"Cannot inspect Linux mountinfo".to_string())?;
        let allowed:&[&str]=match (plugin,access){
            ("gpfs",Some(GpfsAccess::CesNfs))=>&["nfs","nfs4"],
            ("gpfs",Some(GpfsAccess::CesSmb))=>&["cifs","smb3"],
            ("sshfs",None)=>&["fuse.sshfs"],
            ("webdav",None)=>&["fuse.davfs","fuse"],
            ("nfs",None)=>&["nfs","nfs4"],
            ("smb",None)=>&["cifs","smb3"],
            // Native CephFS and the operator-installed FUSE client. Neither
            // proves snapshot consistency or validates the backing cluster.
            ("cephfs",None)=>&["ceph","fuse.ceph-fuse"],
            // Only the explicitly mounted client, never direct brick access.
            ("glusterfs",None)=>&["fuse.glusterfs"],
            ("lustre",None)=>&["lustre"],
            _=>return Err("Unsupported mount protocol".into()),
        };
        let mut entry=parse_mountinfo(&mountinfo).into_iter().find(|item|
            item.mountpoint==location && allowed.contains(&item.fs_type.as_str())
              && (plugin!="webdav" || item.fs_type=="fuse.davfs"
                  || item.source_is_https_webdav)
        ).ok_or("Exact mountpoint is not a matching network/FUSE filesystem")?;
        entry.plugin=plugin.to_owned();
        Ok(entry)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[cfg(target_os="linux")]
    #[test]
    fn mountinfo_reports_escaped_mountpoints_and_fs_types(){
        let text=r#"30 20 0:40 / /mnt/GPFS\040files rw,nosuid - nfs4 server:/exp rw
31 20 0:41 / /mnt/backup ro,nodev - fuse.sshfs user@server:/path ro
32 20 0:42 / /mnt/local rw - ext4 /dev/loop rw"#;
        let v=parse_mountinfo(text);
        assert_eq!(v.len(),3);
        assert_eq!(v[0].mountpoint,"/mnt/GPFS files");
        assert_eq!(v[0].fs_type,"nfs4");
        assert!(v[0].rw_mount_option);
        assert_eq!(v[1].fs_type,"fuse.sshfs");
        assert!(!v[1].rw_mount_option);
        assert!(v[0].source_redacted);
        // Ubuntu davfs2 can expose the generic "fuse" kernel type, not
        // "fuse.davfs"; require an HTTPS WebDAV source on that fallback.
        let more=parse_mountinfo("33 20 0:43 / /mnt/dav ro,nodev - fuse https://localhost:18919/ ro\n34 20 0:44 / /mnt/unsafe ro - fuse /tmp/fake ro");
        assert_eq!(more.len(),2);
        assert!(more[0].source_is_https_webdav);
        assert!(!more[1].source_is_https_webdav);

    }
    #[test]
    fn reject_bad_plugin_and_protocol_selection(){
        let tmp=tempfile::tempdir().unwrap();
        assert!(probe("gpfs",tmp.path(),None).is_err());
        assert!(probe("sshfs",tmp.path(),Some(GpfsAccess::CesNfs)).is_err());
        assert!(probe("webdav",tmp.path(),Some(GpfsAccess::CesNfs)).is_err());
        assert!(probe("posix",tmp.path(),None).is_err());
        assert!(probe("nfs",tmp.path(),Some(GpfsAccess::CesNfs)).is_err());
        assert!(probe("smb",tmp.path(),Some(GpfsAccess::CesSmb)).is_err());
        for plugin in ["cephfs","glusterfs","lustre"] {
            assert!(probe(plugin,tmp.path(),Some(GpfsAccess::CesNfs)).is_err());
        }
    }
    #[cfg(target_os="linux")]
    #[test]
    fn local_directories_are_never_network_mounts(){
        let tmp=tempfile::tempdir().unwrap();
        assert!(probe("gpfs",tmp.path(),Some(GpfsAccess::CesNfs)).is_err());
        assert!(probe("gpfs",tmp.path(),Some(GpfsAccess::CesSmb)).is_err());
        assert!(probe("sshfs",tmp.path(),None).is_err());
        assert!(probe("webdav",tmp.path(),None).is_err());
        assert!(probe("nfs",tmp.path(),None).is_err());
        assert!(probe("smb",tmp.path(),None).is_err());
        for plugin in ["cephfs","glusterfs","lustre"] {
            assert!(probe(plugin,tmp.path(),None).is_err());
        }
    }
}