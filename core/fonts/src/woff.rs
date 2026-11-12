//! WOFF 1.0: an sfnt font whose tables are each compressed with zlib.
//! decoding gives back the plain font, which is what Letters keeps.

use crate::sfnt::{read_u16, read_u32, write_font, Tables};

/// the most a decoded font may grow to. a real font is far smaller; a
/// file that claims more is trying to exhaust memory.
pub const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;

/// the plain font inside a WOFF file, or None when the file is damaged.
pub fn decode(data: &[u8]) -> Option<Vec<u8>> {
    if !data.starts_with(b"wOFF") {
        return None;
    }
    let flavor = read_u32(data, 4)?;
    let table_count = usize::from(read_u16(data, 12)?);
    let mut tables = Tables::new();
    let mut total = 0usize;
    for record in 0..table_count {
        let at = 44 + record * 20;
        let tag: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
        let offset = read_u32(data, at + 4)? as usize;
        let compressed = read_u32(data, at + 8)? as usize;
        let original = read_u32(data, at + 12)? as usize;
        total = total.checked_add(original)?;
        if total > MAX_DECODED_BYTES || compressed > original {
            return None;
        }
        let stored = data.get(offset..offset.checked_add(compressed)?)?;
        let bytes = if compressed == original {
            stored.to_vec()
        } else {
            inflate(stored, original)?
        };
        if bytes.len() != original {
            return None;
        }
        tables.insert(tag, bytes);
    }
    Some(write_font(flavor, &tables))
}

/// inflates a zlib stream that should be exactly `size` bytes.
fn inflate(stored: &[u8], size: usize) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stored, size).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfnt::table_directory;

    /// builds a small woff file around the given tables, compressing them.
    fn woff_of(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(b"wOFF");
        header.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        header.extend_from_slice(&[0; 4]); // length, not checked
        header.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        header.resize(44, 0);
        let mut body = Vec::new();
        let mut directory = Vec::new();
        let data_start = 44 + 20 * tables.len();
        for (tag, bytes) in tables {
            let packed = miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6);
            let stored = if packed.len() < bytes.len() {
                packed
            } else {
                bytes.clone()
            };
            directory.extend_from_slice(tag);
            directory.extend_from_slice(&((data_start + body.len()) as u32).to_be_bytes());
            directory.extend_from_slice(&(stored.len() as u32).to_be_bytes());
            directory.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            directory.extend_from_slice(&[0; 4]);
            body.extend_from_slice(&stored);
        }
        [header, directory, body].concat()
    }

    #[test]
    fn decodes_compressed_and_plain_tables() {
        let file = woff_of(&[(*b"aaaa", vec![7; 400]), (*b"bbbb", vec![1, 2, 3])]);
        let font = decode(&file).unwrap();
        let tables = table_directory(&font, 0).unwrap();
        assert_eq!(&font[tables[b"aaaa"].clone()], &[7; 400][..]);
        assert_eq!(&font[tables[b"bbbb"].clone()], &[1, 2, 3]);
    }

    #[test]
    fn refuses_damaged_files() {
        let mut file = woff_of(&[(*b"aaaa", vec![7; 400])]);
        let last = file.len() - 1;
        file.truncate(last);
        assert!(decode(&file).is_none());
        assert!(decode(b"wOFF").is_none());
        assert!(decode(b"nope").is_none());
    }
}
