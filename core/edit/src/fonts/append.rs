//! adding glyphs to the end of a TrueType font program, keeping every
//! existing glyph, its number and its metrics exactly as they were.
//!
//! the new glyphs get the next glyph numbers. glyf and loca gain their
//! outlines (loca is written in the long form), hmtx their advances, maxp
//! and hhea their counts. post is set to version 3 (no glyph names),
//! because a version 2 table lists one name per glyph and would no
//! longer match. tables that hold one record per glyph and that readers
//! can do without (hdmx, LTSH, VDMX) are dropped for the same reason.
//! the cmap gains the new characters, so simple fonts can reach them.

use std::collections::BTreeMap;

use crate::error::EditError;
use crate::fonts::donor::Outline;
use crate::fonts::truetype::{read_loca, read_u16, table_directory, write_font};

/// tables with per-glyph records that readers rebuild or ignore.
const DROPPED_TABLES: [&[u8; 4]; 3] = [b"hdmx", b"LTSH", b"VDMX"];

/// a new entry for the font's cmap: which subtable, which code, which glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CmapEntry {
    /// windows unicode (3, 1): a character
    Unicode(u32, u16),
    /// windows symbol (3, 0): 0xF000 plus a one-byte code
    Symbol(u8, u16),
    /// macintosh roman (1, 0): a one-byte code
    Mac(u8, u16),
}

/// the font after appending, and the new glyphs' numbers.
pub(crate) struct Appended {
    pub program: Vec<u8>,
    pub first_glyph: u16,
}

fn damaged() -> EditError {
    EditError::UnsupportedFont("The embedded font is damaged.".into())
}

type Tables = BTreeMap<[u8; 4], Vec<u8>>;

fn read_tables(program: &[u8]) -> Result<Tables, EditError> {
    let directory = table_directory(program, 0).ok_or_else(damaged)?;
    Ok(directory
        .into_iter()
        .map(|(tag, range)| (tag, program[range].to_vec()))
        .collect())
}

fn table<'a>(tables: &'a Tables, tag: &[u8; 4]) -> Result<&'a Vec<u8>, EditError> {
    tables.get(tag).ok_or_else(|| {
        EditError::UnsupportedFont(
            "The embedded font has no TrueType outlines Letters can add to.".into(),
        )
    })
}

/// appends glyphs with these outlines, then the cmap entries (whose
/// glyph numbers count from the first new glyph: 0 is the first added).
pub(crate) fn append_glyphs(
    program: &[u8],
    outlines: &[Outline],
    cmap: &[CmapEntry],
) -> Result<Appended, EditError> {
    let mut tables = read_tables(program)?;
    let count = glyph_count(&tables)?;
    let new_count = count + outlines.len();
    if new_count > usize::from(u16::MAX) {
        return Err(EditError::UnsupportedFont(
            "The embedded font has no room for more glyphs.".into(),
        ));
    }
    let first_glyph = count as u16;
    append_outlines(&mut tables, count, outlines)?;
    append_metrics(&mut tables, count, outlines)?;
    set_counts(&mut tables, new_count, outlines)?;
    let shifted: Vec<CmapEntry> = cmap
        .iter()
        .map(|entry| shift(*entry, first_glyph))
        .collect();
    let rebuilt = rebuilt_cmap(program, &shifted);
    tables.insert(*b"cmap", rebuilt);
    for tag in DROPPED_TABLES {
        tables.remove(tag);
    }
    if let Some(post) = tables.get_mut(b"post") {
        // version 3: no glyph names to keep in step with the glyph count
        post.truncate(32);
        post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    }
    Ok(Appended {
        program: write_font(&tables),
        first_glyph,
    })
}

fn shift(entry: CmapEntry, first: u16) -> CmapEntry {
    match entry {
        CmapEntry::Unicode(code, glyph) => CmapEntry::Unicode(code, glyph + first),
        CmapEntry::Symbol(code, glyph) => CmapEntry::Symbol(code, glyph + first),
        CmapEntry::Mac(code, glyph) => CmapEntry::Mac(code, glyph + first),
    }
}

fn glyph_count(tables: &Tables) -> Result<usize, EditError> {
    let maxp = table(tables, b"maxp")?;
    read_u16(maxp, 4).map(usize::from).ok_or_else(damaged)
}

/// the glyph outlines in glyf, with a long loca pointing at them.
fn append_outlines(
    tables: &mut Tables,
    count: usize,
    outlines: &[Outline],
) -> Result<(), EditError> {
    let head = table(tables, b"head")?.clone();
    let long = head.get(50..52).is_some_and(|format| format == [0, 1]);
    let offsets = read_loca(table(tables, b"loca")?, count, long).ok_or_else(damaged)?;
    let mut glyf = table(tables, b"glyf")?.clone();
    glyf.truncate(*offsets.last().ok_or_else(damaged)?);
    let mut new_offsets = offsets;
    for outline in outlines {
        glyf.extend(encode_glyph(outline));
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        new_offsets.push(glyf.len());
    }
    let loca: Vec<u8> = new_offsets
        .iter()
        .flat_map(|offset| (*offset as u32).to_be_bytes())
        .collect();
    let mut head = head;
    // long loca offsets, and a checksum adjustment readers recompute
    head[50..52].copy_from_slice(&[0, 1]);
    head[8..12].copy_from_slice(&[0; 4]);
    tables.insert(*b"head", head);
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    Ok(())
}

/// every glyph's advance and left side bearing written in full, then the
/// new glyphs'. (hmtx may store the last advance once for many glyphs;
/// writing all of them keeps the new ones from inheriting it.)
fn append_metrics(
    tables: &mut Tables,
    count: usize,
    outlines: &[Outline],
) -> Result<(), EditError> {
    let hhea = table(tables, b"hhea")?;
    let long_count = usize::from(read_u16(hhea, 34).ok_or_else(damaged)?);
    let hmtx = table(tables, b"hmtx")?;
    let mut metrics = Vec::with_capacity((count + outlines.len()) * 4);
    let mut last_advance = 0u16;
    for glyph in 0..count {
        let (advance, bearing) = if glyph < long_count {
            (
                read_u16(hmtx, glyph * 4).unwrap_or(0),
                read_u16(hmtx, glyph * 4 + 2).unwrap_or(0),
            )
        } else {
            let at = long_count * 4 + (glyph - long_count) * 2;
            (last_advance, read_u16(hmtx, at).unwrap_or(0))
        };
        last_advance = advance;
        metrics.extend_from_slice(&advance.to_be_bytes());
        metrics.extend_from_slice(&bearing.to_be_bytes());
    }
    for outline in outlines {
        let advance = outline.advance.round().clamp(0.0, f32::from(u16::MAX)) as u16;
        let bearing = bounds(outline).map_or(0, |(x_min, ..)| x_min);
        metrics.extend_from_slice(&advance.to_be_bytes());
        metrics.extend_from_slice(&bearing.to_be_bytes());
    }
    tables.insert(*b"hmtx", metrics);
    Ok(())
}

/// the glyph count in maxp and hhea, and maxp's largest outline sizes.
fn set_counts(
    tables: &mut Tables,
    new_count: usize,
    outlines: &[Outline],
) -> Result<(), EditError> {
    let new_count = new_count as u16;
    let mut maxp = table(tables, b"maxp")?.clone();
    maxp[4..6].copy_from_slice(&new_count.to_be_bytes());
    if maxp.len() >= 10 {
        let points = outlines
            .iter()
            .map(|o| o.contours.iter().map(Vec::len).sum::<usize>())
            .max()
            .unwrap_or(0) as u16;
        let contours = outlines.iter().map(|o| o.contours.len()).max().unwrap_or(0) as u16;
        let old_points = read_u16(&maxp, 6).unwrap_or(0);
        let old_contours = read_u16(&maxp, 8).unwrap_or(0);
        maxp[6..8].copy_from_slice(&old_points.max(points).to_be_bytes());
        maxp[8..10].copy_from_slice(&old_contours.max(contours).to_be_bytes());
    }
    tables.insert(*b"maxp", maxp);
    let mut hhea = table(tables, b"hhea")?.clone();
    hhea[34..36].copy_from_slice(&new_count.to_be_bytes());
    let widest = outlines
        .iter()
        .map(|o| o.advance.round() as u16)
        .max()
        .unwrap_or(0);
    let old_widest = read_u16(&hhea, 10).unwrap_or(0);
    hhea[10..12].copy_from_slice(&old_widest.max(widest).to_be_bytes());
    tables.insert(*b"hhea", hhea);
    Ok(())
}

/// the box around an outline's points, rounded to font units.
fn bounds(outline: &Outline) -> Option<(u16, i16, i16, i16)> {
    let points: Vec<_> = outline.contours.iter().flatten().collect();
    let first = points.first()?;
    let (mut x_min, mut y_min, mut x_max, mut y_max) = (first.x, first.y, first.x, first.y);
    for point in &points {
        x_min = x_min.min(point.x);
        y_min = y_min.min(point.y);
        x_max = x_max.max(point.x);
        y_max = y_max.max(point.y);
    }
    let round = |value: f32| value.round() as i16;
    Some((
        round(x_min) as u16,
        round(y_min),
        round(x_max),
        round(y_max),
    ))
}

/// a simple TrueType glyph: contour ends, no instructions, a flag per
/// point, then every coordinate as a two-byte step from the one before.
fn encode_glyph(outline: &Outline) -> Vec<u8> {
    let Some((x_min, y_min, x_max, y_max)) = bounds(outline) else {
        // a space: a glyph with no outline is written as no bytes at all
        return Vec::new();
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(outline.contours.len() as i16).to_be_bytes());
    for value in [x_min as i16, y_min, x_max, y_max] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    let mut end = 0usize;
    for contour in &outline.contours {
        end += contour.len();
        bytes.extend_from_slice(&((end - 1) as u16).to_be_bytes());
    }
    bytes.extend_from_slice(&0u16.to_be_bytes());
    let points: Vec<_> = outline.contours.iter().flatten().collect();
    bytes.extend(points.iter().map(|point| u8::from(point.on_curve)));
    let rounded: Vec<(i16, i16)> = points
        .iter()
        .map(|point| (point.x.round() as i16, point.y.round() as i16))
        .collect();
    for steps in [0, 1] {
        let mut previous = 0i16;
        for (x, y) in &rounded {
            let value = if steps == 0 { *x } else { *y };
            bytes.extend_from_slice(&value.wrapping_sub(previous).to_be_bytes());
            previous = value;
        }
    }
    bytes
}

/// the font's cmap written again with its old mappings and the new
/// entries: format 4 for the windows subtables, format 6 for mac roman.
fn rebuilt_cmap(program: &[u8], additions: &[CmapEntry]) -> Vec<u8> {
    let mut unicode: BTreeMap<u32, u16> = BTreeMap::new();
    let mut symbol: BTreeMap<u32, u16> = BTreeMap::new();
    let mut mac: BTreeMap<u32, u16> = BTreeMap::new();
    if let Ok(face) = ttf_parser::Face::parse(program, 0) {
        for subtable in face
            .tables()
            .cmap
            .into_iter()
            .flat_map(|cmap| cmap.subtables)
        {
            let target = match (subtable.platform_id, subtable.encoding_id) {
                (ttf_parser::PlatformId::Windows, 1) | (ttf_parser::PlatformId::Unicode, _) => {
                    &mut unicode
                }
                (ttf_parser::PlatformId::Windows, 0) => &mut symbol,
                (ttf_parser::PlatformId::Macintosh, 0) => &mut mac,
                _ => continue,
            };
            subtable.codepoints(|code| {
                if let Some(glyph) = subtable.glyph_index(code) {
                    target.entry(code).or_insert(glyph.0);
                }
            });
        }
    }
    for entry in additions {
        match *entry {
            CmapEntry::Unicode(code, glyph) => unicode.insert(code, glyph),
            CmapEntry::Symbol(code, glyph) => symbol.insert(0xF000 + u32::from(code), glyph),
            CmapEntry::Mac(code, glyph) => mac.insert(u32::from(code), glyph),
        };
    }
    let mut subtables: Vec<(u16, u16, Vec<u8>)> = Vec::new();
    if !mac.is_empty() {
        subtables.push((1, 0, format_6(&mac)));
    }
    if !symbol.is_empty() {
        subtables.push((3, 0, format_4(&symbol)));
    }
    if !unicode.is_empty() {
        subtables.push((3, 1, format_4(&unicode)));
    }
    cmap_table(&subtables)
}

fn u16s(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

fn cmap_table(subtables: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut header = u16s(&[0, subtables.len() as u16]);
    let mut body = Vec::new();
    let start = 4 + 8 * subtables.len();
    for (platform, encoding, bytes) in subtables {
        header.extend(u16s(&[*platform, *encoding]));
        header.extend(((start + body.len()) as u32).to_be_bytes());
        body.extend_from_slice(bytes);
    }
    header.extend(body);
    header
}

/// format 6: one glyph number for each code of a range (mac roman fits
/// in one range below 256).
fn format_6(map: &BTreeMap<u32, u16>) -> Vec<u8> {
    let first = *map.keys().next().unwrap_or(&0) as u16;
    let last = *map.keys().last().unwrap_or(&0) as u16;
    let glyphs: Vec<u16> = (first..=last)
        .map(|code| map.get(&u32::from(code)).copied().unwrap_or(0))
        .collect();
    let length = 10 + 2 * glyphs.len();
    let mut bytes = u16s(&[6, length as u16, 0, first, glyphs.len() as u16]);
    bytes.extend(u16s(&glyphs));
    bytes
}

/// format 4 with one segment per code: plain and always correct, and
/// small for the few hundred characters a subset font holds.
fn format_4(map: &BTreeMap<u32, u16>) -> Vec<u8> {
    let mut codes: Vec<(u16, u16)> = map
        .iter()
        .filter(|(code, _)| **code < 0xFFFF)
        .map(|(code, glyph)| (*code as u16, *glyph))
        .collect();
    codes.push((0xFFFF, 0));
    let segments = codes.len() as u16;
    let mut power = 1u16;
    while power * 2 <= segments {
        power *= 2;
    }
    let search_range = power * 2;
    let selector = power.trailing_zeros() as u16;
    let mut bytes = u16s(&[
        4,
        0,
        0,
        segments * 2,
        search_range,
        selector,
        segments * 2 - search_range,
    ]);
    bytes.extend(codes.iter().flat_map(|(code, _)| code.to_be_bytes()));
    bytes.extend(u16s(&[0]));
    bytes.extend(codes.iter().flat_map(|(code, _)| code.to_be_bytes()));
    // the delta that turns each code into its glyph (the last segment maps to 0)
    bytes.extend(codes.iter().flat_map(|(code, glyph)| {
        let delta = if *code == 0xFFFF {
            1
        } else {
            glyph.wrapping_sub(*code)
        };
        delta.to_be_bytes()
    }));
    bytes.extend(codes.iter().flat_map(|_| 0u16.to_be_bytes()));
    let length = bytes.len() as u16;
    bytes[2..4].copy_from_slice(&length.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::donor::OutlinePoint;

    fn square() -> Outline {
        let point = |x: f32, y: f32| OutlinePoint {
            x,
            y,
            on_curve: true,
        };
        Outline {
            contours: vec![vec![
                point(100.0, 0.0),
                point(400.0, 0.0),
                point(400.0, 700.0),
                point(100.0, 700.0),
            ]],
            advance: 500.0,
        }
    }

    #[test]
    fn encoded_glyphs_read_back_with_their_box() {
        let bytes = encode_glyph(&square());
        assert_eq!(i16::from_be_bytes([bytes[0], bytes[1]]), 1);
        assert_eq!(i16::from_be_bytes([bytes[6], bytes[7]]), 400);
        assert_eq!(
            u16::from_be_bytes([bytes[10], bytes[11]]),
            3,
            "the contour ends at point 3"
        );
        assert!(encode_glyph(&Outline {
            contours: Vec::new(),
            advance: 250.0
        })
        .is_empty());
    }

    #[test]
    fn format_4_maps_every_code() {
        let map = BTreeMap::from([(65u32, 3u16), (0xE9, 9)]);
        let mut tables = Tables::new();
        tables.insert(*b"cmap", cmap_table(&[(3, 1, format_4(&map))]));
        let file = write_font(&tables);
        let parsed = ttf_parser::RawFace::parse(&file, 0).unwrap();
        let cmap = ttf_parser::cmap::Table::parse(
            parsed.table(ttf_parser::Tag::from_bytes(b"cmap")).unwrap(),
        )
        .unwrap();
        let subtable = cmap.subtables.get(0).unwrap();
        assert_eq!(subtable.glyph_index(65).map(|g| g.0), Some(3));
        assert_eq!(subtable.glyph_index(0xE9).map(|g| g.0), Some(9));
        assert_eq!(subtable.glyph_index(66), None);
    }
}
