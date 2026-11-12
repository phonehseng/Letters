//! the character map of a completed font: the old font's symbol and mac
//! subtables kept, new unicode subtables (formats 4 and 12) with every
//! character, old and new.

use super::*;

/// the cmap: the old font's symbol and mac tables kept as they were (so
/// symbolic pdf fonts still find their old glyphs), and new unicode
/// tables with every character, old and new.
pub(crate) fn cmap_table(char_to_glyph: &BTreeMap<char, u16>, old: &[u8]) -> Vec<u8> {
    let mut subtables: Vec<((u16, u16), Vec<u8>)> = old_symbol_subtables(old);
    subtables.push(((3, 1), format_4(char_to_glyph)));
    if char_to_glyph.keys().any(|c| u32::from(*c) > 0xFFFF) {
        subtables.push(((3, 10), format_12(char_to_glyph)));
    }
    subtables.sort_by_key(|(id, _)| *id);
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&(subtables.len() as u16).to_be_bytes());
    let mut offset = 4 + 8 * subtables.len();
    for ((platform, encoding), bytes) in &subtables {
        table.extend_from_slice(&platform.to_be_bytes());
        table.extend_from_slice(&encoding.to_be_bytes());
        table.extend_from_slice(&(offset as u32).to_be_bytes());
        offset += bytes.len();
    }
    for (_, bytes) in subtables {
        table.extend_from_slice(&bytes);
    }
    table
}

/// the (3, 0) symbol and (1, 0) mac subtables of an old cmap, copied.
pub(crate) fn old_symbol_subtables(old: &[u8]) -> Vec<((u16, u16), Vec<u8>)> {
    let mut found = Vec::new();
    let count = usize::from(read_u16(old, 2).unwrap_or(0)).min(64);
    for record in 0..count {
        let at = 4 + record * 8;
        let (Some(platform), Some(encoding), Some(offset)) = (
            read_u16(old, at),
            read_u16(old, at + 2),
            read_u32(old, at + 4),
        ) else {
            break;
        };
        if (platform, encoding) != (3, 0) && (platform, encoding) != (1, 0) {
            continue;
        }
        let offset = offset as usize;
        let length = match read_u16(old, offset) {
            Some(0 | 2 | 4 | 6) => read_u16(old, offset + 2).map(usize::from),
            Some(8 | 10 | 12 | 13) => read_u32(old, offset + 4).map(|l| l as usize),
            _ => None,
        };
        if let Some(bytes) = length.and_then(|length| old.get(offset..offset.checked_add(length)?))
        {
            if !found.iter().any(|(id, _)| *id == (platform, encoding)) {
                found.push(((platform, encoding), bytes.to_vec()));
            }
        }
    }
    found
}

/// a format 4 subtable: runs of characters whose glyph numbers also run
/// on become one segment each.
pub(crate) fn format_4(char_to_glyph: &BTreeMap<char, u16>) -> Vec<u8> {
    let mut segments: Vec<(u16, u16, u16)> = Vec::new(); // start, end, first glyph
    for (c, glyph) in char_to_glyph {
        let code = u32::from(*c);
        if code >= 0xFFFF {
            continue;
        }
        let code = code as u16;
        match segments.last_mut() {
            Some((start, end, first))
                if *end + 1 == code && first.wrapping_add(code - *start) == *glyph =>
            {
                *end = code;
            }
            _ => segments.push((code, code, *glyph)),
        }
    }
    segments.push((0xFFFF, 0xFFFF, 0));
    let count = segments.len() as u16;
    let mut power = 1u16;
    while power * 2 <= count {
        power *= 2;
    }
    let mut table = Vec::new();
    table.extend_from_slice(&4u16.to_be_bytes());
    table.extend_from_slice(&0u16.to_be_bytes()); // length, set below
    table.extend_from_slice(&0u16.to_be_bytes()); // language
    table.extend_from_slice(&(count * 2).to_be_bytes());
    table.extend_from_slice(&(power * 2).to_be_bytes());
    table.extend_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    table.extend_from_slice(&(count * 2 - power * 2).to_be_bytes());
    for (_, end, _) in &segments {
        table.extend_from_slice(&end.to_be_bytes());
    }
    table.extend_from_slice(&0u16.to_be_bytes()); // reserved pad
    for (start, _, _) in &segments {
        table.extend_from_slice(&start.to_be_bytes());
    }
    for (start, _, first) in &segments {
        // the final segment maps 0xFFFF to glyph 0: delta 1 wraps it round
        let delta = if *start == 0xFFFF {
            1
        } else {
            first.wrapping_sub(*start)
        };
        table.extend_from_slice(&delta.to_be_bytes());
    }
    for _ in &segments {
        table.extend_from_slice(&0u16.to_be_bytes()); // no glyph id arrays
    }
    let length = table.len() as u16;
    table[2..4].copy_from_slice(&length.to_be_bytes());
    table
}

/// a format 12 subtable, for characters beyond the basic plane.
pub(crate) fn format_12(char_to_glyph: &BTreeMap<char, u16>) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for (c, glyph) in char_to_glyph {
        let code = u32::from(*c);
        let glyph = u32::from(*glyph);
        match groups.last_mut() {
            Some((start, end, first)) if *end + 1 == code && *first + (code - *start) == glyph => {
                *end = code;
            }
            _ => groups.push((code, code, glyph)),
        }
    }
    let mut table = Vec::new();
    table.extend_from_slice(&12u16.to_be_bytes());
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&((16 + groups.len() * 12) as u32).to_be_bytes());
    table.extend_from_slice(&0u32.to_be_bytes()); // language
    table.extend_from_slice(&(groups.len() as u32).to_be_bytes());
    for (start, end, first) in groups {
        table.extend_from_slice(&start.to_be_bytes());
        table.extend_from_slice(&end.to_be_bytes());
        table.extend_from_slice(&first.to_be_bytes());
    }
    table
}
