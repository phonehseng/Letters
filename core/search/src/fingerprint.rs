//! telling whether a file changed since it was indexed, cheaply.
//!
//! size and modification time catch nearly every change. a checksum of
//! the first and last 64 kb catches the rest (a file copied over another
//! with its old time kept) without reading a 500 page pdf from end to end.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::model::Fingerprint;

const SAMPLE_BYTES: u64 = 64 * 1024;

/// the fingerprint of a file, or none when it cannot be read.
pub fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as u64);
    let size = metadata.len();
    let hash = sample_hash(path, size).ok()?;
    Some(Fingerprint {
        size,
        modified,
        hash,
    })
}

fn sample_hash(path: &Path, size: u64) -> std::io::Result<u32> {
    let mut file = File::open(path)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0u8; SAMPLE_BYTES as usize];
    let read = read_up_to(&mut file, &mut buffer)?;
    hasher.update(&buffer[..read]);
    if size > SAMPLE_BYTES * 2 {
        file.seek(SeekFrom::Start(size - SAMPLE_BYTES))?;
        let read = read_up_to(&mut file, &mut buffer)?;
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

fn read_up_to(file: &mut File, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_in_content_change_the_fingerprint() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.txt");
        std::fs::write(&path, "first version").unwrap();
        let first = fingerprint(&path).unwrap();
        assert_eq!(first, fingerprint(&path).unwrap());
        // same size, different bytes
        std::fs::write(&path, "first VERSION").unwrap();
        let second = fingerprint(&path).unwrap();
        assert_eq!(first.size, second.size);
        assert_ne!(first.hash, second.hash);
        assert!(fingerprint(&folder.path().join("missing.txt")).is_none());
        assert!(fingerprint(folder.path()).is_none());
    }

    #[test]
    fn large_files_are_sampled_at_both_ends() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("big.bin");
        let mut bytes = vec![7u8; 300 * 1024];
        std::fs::write(&path, &bytes).unwrap();
        let before = fingerprint(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] = 8;
        std::fs::write(&path, &bytes).unwrap();
        assert_ne!(before.hash, fingerprint(&path).unwrap().hash);
    }
}
