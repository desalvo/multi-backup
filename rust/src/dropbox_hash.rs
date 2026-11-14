//! Dropbox SHA-256 hash of contiguous 4 MiB block digests.
//! The Rust implementation streams and never opens network connections.
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr},
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

const BLOCK: usize = 4 * 1024 * 1024;

pub fn hash_file(path: &Path) -> io::Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "Dropbox hash path must be an ordinary file"));
    }
    let mut stream = File::open(path)?;
    let mut overall = Sha256::new();
    let mut block = vec![0u8; BLOCK];
    loop {
        let mut filled = 0usize;
        while filled < BLOCK {
            let read = stream.read(&mut block[filled..])?;
            if read == 0 { break; }
            filled += read;
        }
        if filled == 0 { break; }
        overall.update(Sha256::digest(&block[..filled]));
        if filled < BLOCK { break; }
    }
    Ok(format!("{:x}", overall.finalize()))
}

/// Return 0 only on successful hash calculation. The caller owns its output
/// buffer and must provide >=65 bytes for ASCII hex plus a trailing NUL.
#[no_mangle]
pub unsafe extern "C" fn multi_backup_dropbox_hash(
    source: *const c_char, out: *mut u8, out_len: usize
) -> i32 {
    if source.is_null() || out.is_null() || out_len < 65 { return -1; }
    let name = match CStr::from_ptr(source).to_str() {
        Ok(value) if !value.is_empty() => value,
        _ => return -2,
    };
    match hash_file(Path::new(name)) {
        Ok(hex) if hex.len() == 64 => {
            std::ptr::copy_nonoverlapping(hex.as_ptr(), out, 64);
            *out.add(64) = 0;
            0
        }
        _ => -3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_file_matches_dropbox_spec() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("empty");
        fs::write(&file, b"").unwrap();
        assert_eq!(hash_file(&file).unwrap(),
                   format!("{:x}", Sha256::digest(b"")));
    }
    #[test]
    fn uses_digest_of_digest_for_each_block() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("data");
        fs::write(&file, b"abcd").unwrap();
        let first = Sha256::digest(b"abcd");
        assert_eq!(hash_file(&file).unwrap(),
                   format!("{:x}", Sha256::digest(first)));
    }
}
