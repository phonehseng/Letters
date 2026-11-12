//! the sfnt container every TrueType and OpenType font uses: a table
//! directory followed by tables. reading it is bounds checked everywhere,
//! because font files are untrusted input.

use std::collections::BTreeMap;
use std::ops::Range;

/// the tables of one font, by tag.
pub type Tables = BTreeMap<[u8; 4], Vec<u8>>;

pub fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at.checked_add(1)?)?,
    ]))
}

pub fn read_i16(bytes: &[u8], at: usize) -> Option<i16> {
    read_u16(bytes, at).map(|value| value as i16)
}

pub fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
}

/// true for the first bytes of a TrueType, OpenType or collection file.
pub fn is_sfnt(data: &[u8]) -> bool {
    matches!(
        data.get(0..4),
        Some([0, 1, 0, 0]) | Some(b"OTTO") | Some(b"true") | Some(b"ttcf")
    )
}

/// how many fonts the file holds: the count in a collection, else 1.
pub fn face_count(data: &[u8]) -> u32 {
    if data.starts_with(b"ttcf") {
        // a collection with more than 256 fonts is not a real one
        read_u32(data, 8).unwrap_or(0).min(256)
    } else {
        1
    }
}

/// where the font at `index` starts: its offset in a collection, else 0.
fn font_start(data: &[u8], index: u32) -> Option<usize> {
    if data.starts_with(b"ttcf") {
        if index >= face_count(data) {
            return None;
        }
        let at = 12usize.checked_add(4usize.checked_mul(index as usize)?)?;
        Some(read_u32(data, at)? as usize)
    } else if index == 0 {
        Some(0)
    } else {
        None
    }
}

/// where each table of the font at `index` sits in the file. tables
/// that point outside the file are left out.
pub fn table_directory(data: &[u8], index: u32) -> Option<BTreeMap<[u8; 4], Range<usize>>> {
    let start = font_start(data, index)?;
    let count = usize::from(read_u16(data, start.checked_add(4)?)?);
    let mut tables = BTreeMap::new();
    for record in 0..count {
        let at = start.checked_add(12 + record * 16)?;
        let tag: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
        let offset = read_u32(data, at + 8)? as usize;
        let length = read_u32(data, at + 12)? as usize;
        if offset.checked_add(length)? <= data.len() {
            tables.insert(tag, offset..offset + length);
        }
    }
    Some(tables)
}

/// the version number of the font at `index`: 0x00010000 or "OTTO".
pub fn flavor(data: &[u8], index: u32) -> Option<u32> {
    read_u32(data, font_start(data, index)?)
}

/// the tables of one font, copied out of the file.
pub fn read_tables(data: &[u8], index: u32) -> Option<Tables> {
    Some(
        table_directory(data, index)?
            .into_iter()
            .map(|(tag, range)| (tag, data[range].to_vec()))
            .collect(),
    )
}

/// the bytes of the one font at `index`, even when it came from a
/// collection. the web view's FontFace can't pick a font out of a
/// collection, so it is given a single font.
pub fn single_font(data: &[u8], index: u32) -> Option<Vec<u8>> {
    if index == 0 && !data.starts_with(b"ttcf") {
        return Some(data.to_vec());
    }
    Some(write_font(flavor(data, index)?, &read_tables(data, index)?))
}

/// writes tables into a font file, with the directory readers expect.
/// `flavor` is 0x00010000 for TrueType outlines or "OTTO" for CFF.
pub fn write_font(flavor: u32, tables: &Tables) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut power = 1u16;
    while power.saturating_mul(2) <= count {
        power *= 2;
    }
    let mut file = Vec::new();
    file.extend_from_slice(&flavor.to_be_bytes());
    file.extend_from_slice(&count.to_be_bytes());
    file.extend_from_slice(&(power.wrapping_mul(16)).to_be_bytes());
    file.extend_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    file.extend_from_slice(
        &(count.wrapping_mul(16).wrapping_sub(power.wrapping_mul(16))).to_be_bytes(),
    );
    let directory_end = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, bytes) in tables {
        let offset = directory_end + body.len();
        file.extend_from_slice(tag);
        file.extend_from_slice(&checksum(bytes).to_be_bytes());
        file.extend_from_slice(&(offset as u32).to_be_bytes());
        file.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(bytes);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    file.extend_from_slice(&body);
    file
}

pub fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_fonts_list_their_tables() {
        let tables = Tables::from([(*b"abcd", vec![1, 2, 3]), (*b"wxyz", vec![9; 8])]);
        let file = write_font(0x0001_0000, &tables);
        let directory = table_directory(&file, 0).unwrap();
        assert_eq!(&file[directory[b"abcd"].clone()], &[1, 2, 3]);
        assert_eq!(&file[directory[b"wxyz"].clone()], &[9; 8]);
        assert!(is_sfnt(&file));
        assert_eq!(single_font(&file, 0).unwrap(), file);
    }

    #[test]
    fn short_and_broken_input_is_refused_quietly() {
        assert!(table_directory(&[], 0).is_none());
        assert!(table_directory(&[0, 1, 0, 0, 0xff, 0xff], 0).is_none());
        assert!(table_directory(b"ttcf\0\0\0\0\0\0\0\x05", 3).is_none());
        assert_eq!(face_count(b"ttcf\0\x01\0\0\xff\xff\xff\xff"), 256);
        assert!(single_font(b"OTTO", 1).is_none());
    }
}
