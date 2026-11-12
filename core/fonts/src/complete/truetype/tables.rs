//! the font-wide tables of a completed font (head, hhea, maxp, name, post,
//! OS/2), updated from the document font's or written new, and the file's
//! checksum adjustment.

use super::*;

pub(crate) fn put_u16(table: &mut [u8], at: usize, value: u16) {
    if let Some(slot) = table.get_mut(at..at + 2) {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

pub(crate) fn put_i16(table: &mut [u8], at: usize, value: i16) {
    put_u16(table, at, value as u16);
}

pub(crate) fn updated_head(head: &[u8], added: &[EncodedGlyph]) -> Vec<u8> {
    let mut table = head.to_vec();
    // long loca, and a checksum adjustment worked out at the end
    put_i16(&mut table, 50, 1);
    table[8..12].copy_from_slice(&[0; 4]);
    let mut bounds = (
        read_i16(head, 36).unwrap_or(0),
        read_i16(head, 38).unwrap_or(0),
        read_i16(head, 40).unwrap_or(0),
        read_i16(head, 42).unwrap_or(0),
    );
    for glyph in added.iter().filter(|g| g.contours > 0) {
        bounds = (
            bounds.0.min(glyph.bounds.0),
            bounds.1.min(glyph.bounds.1),
            bounds.2.max(glyph.bounds.2),
            bounds.3.max(glyph.bounds.3),
        );
    }
    put_i16(&mut table, 36, bounds.0);
    put_i16(&mut table, 38, bounds.1);
    put_i16(&mut table, 40, bounds.2);
    put_i16(&mut table, 42, bounds.3);
    table
}

pub(crate) fn updated_hhea(hhea: &[u8], total: usize, added: &[EncodedGlyph]) -> Vec<u8> {
    let mut table = hhea.to_vec();
    let mut advance_max = read_u16(hhea, 10).unwrap_or(0);
    let mut min_left = read_i16(hhea, 12).unwrap_or(0);
    let mut min_right = read_i16(hhea, 14).unwrap_or(0);
    let mut extent = read_i16(hhea, 16).unwrap_or(0);
    for glyph in added {
        advance_max = advance_max.max(glyph.advance);
        if glyph.contours > 0 {
            min_left = min_left.min(glyph.lsb);
            let right = i32::from(glyph.advance) - i32::from(glyph.bounds.2);
            min_right = min_right.min(right.clamp(-32_768, 32_767) as i16);
            extent = extent.max(glyph.bounds.2);
        }
    }
    put_u16(&mut table, 10, advance_max);
    put_i16(&mut table, 12, min_left);
    put_i16(&mut table, 14, min_right);
    put_i16(&mut table, 16, extent);
    put_u16(&mut table, 34, total as u16);
    table
}

pub(crate) fn updated_maxp(maxp: &[u8], total: usize, added: &[EncodedGlyph]) -> Vec<u8> {
    let mut table = maxp.to_vec();
    put_u16(&mut table, 4, total as u16);
    // version 1.0 also counts the most points and contours in a glyph
    if read_u32(maxp, 0) == Some(0x0001_0000) && maxp.len() >= 32 {
        let points = added.iter().map(|g| g.points).max().unwrap_or(0);
        let contours = added.iter().map(|g| g.contours).max().unwrap_or(0);
        put_u16(&mut table, 6, read_u16(maxp, 6).unwrap_or(0).max(points));
        put_u16(&mut table, 8, read_u16(maxp, 8).unwrap_or(0).max(contours));
    }
    table
}

pub(crate) fn new_head(units: u16, glyphs: &[EncodedGlyph], metrics: &FontMetrics) -> Vec<u8> {
    let drawn = glyphs.iter().filter(|g| g.contours > 0);
    let x_min = drawn.clone().map(|g| g.bounds.0).min().unwrap_or(0);
    let y_min = drawn.clone().map(|g| g.bounds.1).min().unwrap_or(0);
    let x_max = drawn.clone().map(|g| g.bounds.2).max().unwrap_or(0);
    let y_max = drawn.map(|g| g.bounds.3).max().unwrap_or(0);
    let mut table = Vec::with_capacity(54);
    table.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    table.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // font revision
    table.extend_from_slice(&0u32.to_be_bytes()); // checksum adjustment, set at the end
    table.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic number
    table.extend_from_slice(&0x000Bu16.to_be_bytes()); // baseline and side bearing at 0, integer sizes
    table.extend_from_slice(&units.to_be_bytes());
    table.extend_from_slice(&[0; 16]); // created and modified: unknown
    for value in [x_min, y_min, x_max, y_max] {
        table.extend_from_slice(&value.to_be_bytes());
    }
    let mac_style: u16 =
        if metrics.italic { 2 } else { 0 } | if metrics.weight_class >= 700 { 1 } else { 0 };
    table.extend_from_slice(&mac_style.to_be_bytes());
    table.extend_from_slice(&8u16.to_be_bytes()); // smallest readable size
    table.extend_from_slice(&2i16.to_be_bytes()); // font direction hint
    table.extend_from_slice(&1i16.to_be_bytes()); // long loca
    table.extend_from_slice(&0i16.to_be_bytes()); // glyph data format
    table
}

pub(crate) fn new_hhea(metrics: &FontMetrics, units: f64, glyphs: &[EncodedGlyph]) -> Vec<u8> {
    let advance_max = glyphs.iter().map(|g| g.advance).max().unwrap_or(0);
    let drawn = glyphs.iter().filter(|g| g.contours > 0);
    let min_left = drawn.clone().map(|g| g.lsb).min().unwrap_or(0);
    let min_right = drawn
        .clone()
        .map(|g| (i32::from(g.advance) - i32::from(g.bounds.2)).clamp(-32_768, 32_767) as i16)
        .min()
        .unwrap_or(0);
    let extent = drawn.map(|g| g.bounds.2).max().unwrap_or(0);
    let mut table = Vec::with_capacity(36);
    table.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    table.extend_from_slice(&(to_units(metrics.ascender, units) as i16).to_be_bytes());
    table.extend_from_slice(&(to_units(metrics.descender, units) as i16).to_be_bytes());
    table.extend_from_slice(&(to_units(metrics.line_gap, units) as i16).to_be_bytes());
    table.extend_from_slice(&advance_max.to_be_bytes());
    table.extend_from_slice(&min_left.to_be_bytes());
    table.extend_from_slice(&min_right.to_be_bytes());
    table.extend_from_slice(&extent.to_be_bytes());
    table.extend_from_slice(&1i16.to_be_bytes()); // caret slope rise
    table.extend_from_slice(&0i16.to_be_bytes()); // caret slope run
    table.extend_from_slice(&[0; 10]); // caret offset and four reserved
    table.extend_from_slice(&0i16.to_be_bytes()); // metric data format
    table.extend_from_slice(&(glyphs.len() as u16).to_be_bytes());
    table
}

pub(crate) fn new_maxp(glyphs: &[EncodedGlyph]) -> Vec<u8> {
    let mut table = Vec::with_capacity(32);
    table.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    table.extend_from_slice(&(glyphs.len() as u16).to_be_bytes());
    table.extend_from_slice(
        &glyphs
            .iter()
            .map(|g| g.points)
            .max()
            .unwrap_or(0)
            .to_be_bytes(),
    );
    table.extend_from_slice(
        &glyphs
            .iter()
            .map(|g| g.contours)
            .max()
            .unwrap_or(0)
            .to_be_bytes(),
    );
    // composite points and contours, then zones (2: no twilight use)
    table.extend_from_slice(&[0, 0, 0, 0, 0, 2]);
    // twilight points, storage, function and instruction definitions,
    // stack, instruction size, component elements and depth: all unused
    table.extend_from_slice(&[0; 18]);
    table
}

/// a name table with only the neutral names, in windows english.
pub fn name_table(family: &str, style: &str) -> Vec<u8> {
    let postscript: String = format!("{family}-{style}")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(63)
        .collect();
    let names: [(u16, String); 6] = [
        (1, family.to_string()),
        (2, style.to_string()),
        (3, format!("{family} {style}; completed by Letters")),
        (4, format!("{family} {style}")),
        (5, "Version 1.000".to_string()),
        (6, postscript),
    ];
    let mut strings = Vec::new();
    let mut records = Vec::new();
    for (id, text) in &names {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        records.push((*id, strings.len() as u16, encoded.len() as u16));
        strings.extend_from_slice(&encoded);
    }
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&(records.len() as u16).to_be_bytes());
    table.extend_from_slice(&((6 + records.len() * 12) as u16).to_be_bytes());
    for (id, offset, length) in records {
        for value in [3u16, 1, 0x0409, id, length, offset] {
            table.extend_from_slice(&value.to_be_bytes());
        }
    }
    table.extend_from_slice(&strings);
    table
}

/// a version 3 post table (no glyph names), keeping the old one's
/// slant and underline when there was one.
pub(crate) fn post_table(old: Option<&[u8]>, metrics: &FontMetrics, units: f64) -> Vec<u8> {
    let mut table = vec![0u8; 32];
    table[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    match old.filter(|old| old.len() >= 16) {
        Some(old) => table[4..16].copy_from_slice(&old[4..16]),
        None => {
            let angle = (metrics.italic_angle * 65_536.0).round() as i32;
            table[4..8].copy_from_slice(&angle.to_be_bytes());
            table[8..10].copy_from_slice(&(to_units(-0.1, units) as i16).to_be_bytes());
            table[10..12].copy_from_slice(&(to_units(0.05, units) as i16).to_be_bytes());
        }
    }
    table
}

pub(crate) fn first_and_last(char_to_glyph: &BTreeMap<char, u16>) -> (u16, u16) {
    let first = char_to_glyph
        .keys()
        .next()
        .map_or(0x20, |c| u32::from(*c).min(0xFFFF) as u16);
    let last = char_to_glyph
        .keys()
        .last()
        .map_or(0x20, |c| u32::from(*c).min(0xFFFF) as u16);
    (first, last)
}

pub(crate) fn updated_os2(os2: &[u8], char_to_glyph: &BTreeMap<char, u16>) -> Vec<u8> {
    let mut table = os2.to_vec();
    let (first, last) = first_and_last(char_to_glyph);
    put_u16(&mut table, 64, first);
    put_u16(&mut table, 66, last);
    table
}

/// a version 4 OS/2 table for fonts that came without one (pdf subsets
/// often drop it, and word processors want it).
pub(crate) fn os2_table(
    input: &FontToWrite,
    units: f64,
    char_to_glyph: &BTreeMap<char, u16>,
    advances: &[u16],
) -> Vec<u8> {
    let metrics = input.metrics;
    let unit = |value: f64| to_units(value, units) as i16;
    let drawn: Vec<u32> = advances
        .iter()
        .filter(|a| **a > 0)
        .map(|a| u32::from(*a))
        .collect();
    let average = if drawn.is_empty() {
        0
    } else {
        drawn.iter().sum::<u32>() / drawn.len() as u32
    };
    let (first, last) = first_and_last(char_to_glyph);
    let mut t = Vec::with_capacity(96);
    let mut push = |value: u16| t.extend_from_slice(&value.to_be_bytes());
    push(4); // version
    push(average.min(65_535) as u16);
    push(metrics.weight_class.clamp(1, 1000));
    push(5); // normal width
    push(input.fs_type);
    for value in [0.65, 0.6, 0.0, 0.075, 0.65, 0.6, 0.0, 0.35] {
        push(unit(value) as u16); // sub and superscript size and offset
    }
    push(unit(0.05) as u16); // strikeout size
    push(unit(0.26) as u16); // strikeout position
    push(0); // family class
    t.extend_from_slice(&[0; 10]); // panose: unknown
    t.extend_from_slice(&0x0000_0003u32.to_be_bytes()); // basic latin and latin-1
    t.extend_from_slice(&[0; 12]);
    t.extend_from_slice(b"NONE");
    let mut push = |value: u16| t.extend_from_slice(&value.to_be_bytes());
    let selection = if metrics.italic { 0x0001 } else { 0x0040 }
        | if metrics.weight_class >= 700 {
            0x0020
        } else {
            0
        };
    push(selection);
    push(first);
    push(last);
    push(unit(metrics.ascender) as u16);
    push(unit(metrics.descender) as u16);
    push(unit(metrics.line_gap) as u16);
    push(unit(metrics.ascender).max(0) as u16);
    push(unit(-metrics.descender).max(0) as u16);
    t.extend_from_slice(&0x0000_0001u32.to_be_bytes()); // latin 1 code page
    t.extend_from_slice(&0u32.to_be_bytes());
    let mut push = |value: u16| t.extend_from_slice(&value.to_be_bytes());
    push(unit(metrics.x_height.unwrap_or(0.5)) as u16);
    push(unit(metrics.cap_height.unwrap_or(0.7)) as u16);
    push(0); // default char
    push(0x20); // break char
    push(1); // max context
    t
}

/// writes the tables and sets head's checksum adjustment, as the spec
/// asks: 0xB1B0AFBA minus the checksum of the whole file.
pub(crate) fn finish(tables: &Tables) -> Vec<u8> {
    let mut file = write_font(0x0001_0000, tables);
    if let Some(range) = table_directory(&file, 0).and_then(|d| d.get(b"head").cloned()) {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&file));
        if let Some(slot) = file.get_mut(range.start + 8..range.start + 12) {
            slot.copy_from_slice(&adjustment.to_be_bytes());
        }
    }
    file
}
