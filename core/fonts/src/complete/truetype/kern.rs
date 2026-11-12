//! the kern table of a completed font: the old font's pairs kept, the
//! same design's pairs for new glyphs added (format 0).

use super::*;

/// the pairs of an old kern table's format 0 subtables, as glyph numbers.
pub(crate) fn kern_pairs(kern: &[u8]) -> Vec<(u16, u16, i16)> {
    let mut pairs = Vec::new();
    if read_u16(kern, 0) != Some(0) {
        return pairs;
    }
    let tables = usize::from(read_u16(kern, 2).unwrap_or(0)).min(16);
    let mut at = 4usize;
    for _ in 0..tables {
        let (Some(length), Some(coverage)) = (read_u16(kern, at + 2), read_u16(kern, at + 4))
        else {
            break;
        };
        let horizontal_format_0 =
            coverage & 0x0001 != 0 && coverage >> 8 == 0 && coverage & 0x0004 == 0;
        if horizontal_format_0 {
            let count = usize::from(read_u16(kern, at + 6).unwrap_or(0)).min(20_000);
            for pair in 0..count {
                let p = at + 14 + pair * 6;
                if let (Some(left), Some(right), Some(value)) = (
                    read_u16(kern, p),
                    read_u16(kern, p + 2),
                    read_i16(kern, p + 4),
                ) {
                    pairs.push((left, right, value));
                }
            }
        }
        at += usize::from(length.max(6));
    }
    pairs
}

pub(crate) fn merged_kerning(
    old: Vec<(u16, u16, i16)>,
    new: &[(char, char, f64)],
    char_to_glyph: &BTreeMap<char, u16>,
    units: f64,
) -> Vec<(u16, u16, i16)> {
    let mut pairs: BTreeMap<(u16, u16), i16> =
        old.into_iter().map(|(l, r, v)| ((l, r), v)).collect();
    for (left, right, value) in new {
        if let (Some(l), Some(r)) = (char_to_glyph.get(left), char_to_glyph.get(right)) {
            let value = to_units(*value, units) as i16;
            if value != 0 {
                pairs.insert((*l, *r), value);
            }
        }
    }
    pairs.into_iter().map(|((l, r), v)| (l, r, v)).collect()
}

pub(crate) fn kern_table(pairs: &[(u16, u16, i16)]) -> Vec<u8> {
    let pairs = &pairs[..pairs.len().min(10_000)];
    let count = pairs.len() as u16;
    let mut power = 1u16;
    while power * 2 <= count {
        power *= 2;
    }
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes()); // version
    table.extend_from_slice(&1u16.to_be_bytes()); // one subtable
    table.extend_from_slice(&0u16.to_be_bytes()); // subtable version
    table.extend_from_slice(&((14 + pairs.len() * 6) as u16).to_be_bytes());
    table.extend_from_slice(&0x0001u16.to_be_bytes()); // horizontal, format 0
    table.extend_from_slice(&count.to_be_bytes());
    table.extend_from_slice(&(power * 6).to_be_bytes());
    table.extend_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    table.extend_from_slice(&(count.saturating_sub(power) * 6).to_be_bytes());
    for (left, right, value) in pairs {
        table.extend_from_slice(&left.to_be_bytes());
        table.extend_from_slice(&right.to_be_bytes());
        table.extend_from_slice(&value.to_be_bytes());
    }
    table
}
