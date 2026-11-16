//! Open credential files via a bounded, non-following descriptor on Unix.
//! Path metadata checks alone are not safe against replacement races.
//! Never log the file content or include it in an error message.
use std::{error::Error,fs::{self,OpenOptions},io::Read,path::Path};
type Result<T>=std::result::Result<T,Box<dyn Error+Send+Sync>>;

/// Read one small credential configuration without following the final
/// symlink. An opened descriptor is checked again before any bytes are read.
/// The caller still validates the JSON structure and intended API origin.
pub fn read_owner_only(path:&Path,max_bytes:u64)->Result<Vec<u8>>{
    if max_bytes==0||max_bytes>16*1024*1024{
        return Err("Invalid credential config maximum length".into());
    }
    let before=fs::symlink_metadata(path)?;
    if !before.file_type().is_file()||before.len()==0||before.len()>max_bytes{
        return Err("Credential configuration must be a nonempty bounded regular file".into());
    }
    let mut options=OpenOptions::new();
    options.read(true);
    #[cfg(unix)]{
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW closes the symlink-swap window between lstat and open.
        // O_NONBLOCK prevents a replaced FIFO/device from hanging the reader.
        options.custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK);
    }
    let input=options.open(path)?;
    let opened=input.metadata()?;
    if !opened.is_file()||opened.len()==0||opened.len()>max_bytes{
        return Err("Credential config descriptor is not a bounded regular file".into());
    }
    #[cfg(unix)]{
        use std::os::unix::fs::MetadataExt;
        if before.dev()!=opened.dev()||before.ino()!=opened.ino(){
            return Err("Credential configuration was replaced while opening".into());
        }
        if opened.mode()&0o077!=0{
            return Err("Credential config must be owner-only (0600 or stricter)".into());
        }
    }
    // The underlying file can grow after fstat. Never allocate unboundedly.
    let mut bytes=Vec::new();
    input.take(max_bytes+1).read_to_end(&mut bytes)?;
    if bytes.len() as u64>max_bytes{
        return Err("Credential configuration grew beyond maximum length".into());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn reads_regular_owner_only_config_and_rejects_empty_or_oversized(){
        let root=tempfile::tempdir().unwrap();
        let path=root.path().join("config");
        assert!(read_owner_only(&path,16_384).is_err());
        fs::write(&path,b"{}").unwrap();
        assert_eq!(read_owner_only(&path,16_384).unwrap(),b"{}");
        assert!(read_owner_only(&path,1).is_err());
        fs::write(&path,vec![b'a';16_385]).unwrap();
        assert!(read_owner_only(&path,16_384).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn refuses_permissions_and_final_component_symlinks(){
        use std::os::unix::fs::{symlink,PermissionsExt};
        let root=tempfile::tempdir().unwrap();
        let secret=root.path().join("actual");
        fs::write(&secret,b"{\"credential\":\"private\"}").unwrap();
        fs::set_permissions(&secret,fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_owner_only(&secret,16_384).is_err());
        fs::set_permissions(&secret,fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_owner_only(&secret,16_384).is_ok());
        let link=root.path().join("alias");
        symlink(&secret,&link).unwrap();
        assert!(read_owner_only(&link,16_384).is_err());
        assert!(read_owner_only(root.path(),16_384).is_err());
    }
}
