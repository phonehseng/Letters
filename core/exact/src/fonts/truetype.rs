//! writes a clean truetype font from scratch (recipe 3.6).
//!
//! only the tables word needs: glyf and loca with simple glyphs, hmtx,
//! hhea, head, maxp, a cmap holding just the chars in use, a name table
//! with the unique family name, os/2 and post. no hinting, no layout
//! tables, so nothing in the font can move a glyph.

use std::collections::BTreeMap;

use super::outline::{ContourPoint, Contours};

/// one glyph of the font being written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontGlyph {
    pub contours: Contours,
    pub advance: u16,
}

/// everything that goes into one font file.
pub struct FontSpec<'a> {
    pub family: &'a str,
    pub units_per_em: u16,
    /// glyph 0 is `.notdef`
    pub glyphs: &'a [FontGlyph],
    /// code point to glyph index
    pub cmap: &'a BTreeMap<u32, u16>,
    pub ascent: i16,
    pub descent: i16,
    /// the embedding permission, copied from the source program
    pub fs_type: u16,
}

/// os/2 ulCodePageRange1 bit 0, latin 1: word silently replaced a font that
/// claims no code page with times new roman (measured, 16.0.20527, round 3).
const CODE_PAGE_LATIN_1: u32 = 1;

pub(super) struct Writer(pub(super) Vec<u8>);

impl Writer {
    pub(super) fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub(super) fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub(super) fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub(super) fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub(super) fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Bounds {
    pub(super) x_min: i32,
    pub(super) y_min: i32,
    pub(super) x_max: i32,
    pub(super) y_max: i32,
}

pub(super) fn bounds_of(contours: &Contours) -> Option<Bounds> {
    let mut points = contours.iter().flatten();
    let first = points.next()?;
    let mut b = Bounds {
        x_min: first.x,
        y_min: first.y,
        x_max: first.x,
        y_max: first.y,
    };
    for p in points {
        b.x_min = b.x_min.min(p.x);
        b.y_min = b.y_min.min(p.y);
        b.x_max = b.x_max.max(p.x);
        b.y_max = b.y_max.max(p.y);
    }
    Some(b)
}

pub(super) fn clamp16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// one simple glyph in the glyf format, flags and coordinates compacted.
pub(super) fn glyph_bytes(contours: &Contours) -> Vec<u8> {
    let Some(bounds) = bounds_of(contours) else {
        return Vec::new();
    };
    let mut w = Writer(Vec::new());
    w.i16(contours.len() as i16);
    for v in [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max] {
        w.i16(clamp16(v));
    }
    let mut end = 0usize;
    for contour in contours {
        end += contour.len();
        w.u16((end - 1) as u16);
    }
    w.u16(0); // no instructions
    let points: Vec<&ContourPoint> = contours.iter().flatten().collect();
    let (mut flags, mut xs, mut ys) = (Vec::new(), Writer(Vec::new()), Writer(Vec::new()));
    let (mut last_x, mut last_y) = (0i32, 0i32);
    for point in points {
        let mut flag = u8::from(point.on_curve);
        let (dx, dy) = (point.x - last_x, point.y - last_y);
        (last_x, last_y) = (point.x, point.y);
        flag |= coordinate(dx, &mut xs, 0x02, 0x10);
        flag |= coordinate(dy, &mut ys, 0x04, 0x20);
        flags.push(flag);
    }
    for flag in flags {
        w.u8(flag);
    }
    w.0.extend(xs.0);
    w.0.extend(ys.0);
    w.0
}

/// writes one coordinate delta and returns its flag bits: a short vector
/// with a sign bit, "same as before" for zero, or a full 16-bit value.
fn coordinate(delta: i32, out: &mut Writer, short_bit: u8, same_or_positive_bit: u8) -> u8 {
    if delta == 0 {
        return same_or_positive_bit;
    }
    if delta.abs() <= 255 {
        out.u8(delta.unsigned_abs() as u8);
        return short_bit | if delta > 0 { same_or_positive_bit } else { 0 };
    }
    out.i16(clamp16(delta));
    0
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// writes the font file.
pub fn write_truetype(spec: &FontSpec) -> Vec<u8> {
    let glyph_data: Vec<Vec<u8>> = spec
        .glyphs
        .iter()
        .map(|g| glyph_bytes(&g.contours))
        .collect();
    write_tables(spec, glyph_data, Vec::new())
}

/// the glyf, loca and every other table, assembled.
pub(super) fn write_tables(
    spec: &FontSpec,
    glyph_data: Vec<Vec<u8>>,
    extra_tables: Vec<([u8; 4], Vec<u8>)>,
) -> Vec<u8> {
    let mut glyf = Vec::new();
    let mut loca = Writer(Vec::new());
    for data in &glyph_data {
        loca.u32(glyf.len() as u32);
        glyf.extend_from_slice(data);
        while !glyf.len().is_multiple_of(4) {
            glyf.push(0);
        }
    }
    loca.u32(glyf.len() as u32);

    let all_bounds: Vec<Option<Bounds>> =
        spec.glyphs.iter().map(|g| bounds_of(&g.contours)).collect();
    let font_bounds = all_bounds.iter().flatten().fold(None::<Bounds>, |acc, b| {
        Some(match acc {
            None => *b,
            Some(a) => Bounds {
                x_min: a.x_min.min(b.x_min),
                y_min: a.y_min.min(b.y_min),
                x_max: a.x_max.max(b.x_max),
                y_max: a.y_max.max(b.y_max),
            },
        })
    });
    let font_bounds = font_bounds.unwrap_or_default();

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"OS/2", os2_table(spec, &all_bounds)),
        (*b"cmap", cmap_table(spec.cmap)),
        (*b"glyf", glyf),
        (*b"head", head_table(spec, font_bounds)),
        (*b"hhea", hhea_table(spec, &all_bounds)),
        (*b"hmtx", hmtx_table(spec, &all_bounds)),
        (*b"loca", loca.0),
        (*b"name", name_table(spec.family)),
        (*b"post", post_table()),
    ];
    if !extra_tables.iter().any(|(tag, _)| tag == b"maxp") {
        tables.push((*b"maxp", maxp_table(spec)));
    }
    tables.extend(extra_tables);
    tables.sort_by_key(|table| table.0);
    assemble(tables)
}

/// the table directory, the tables padded to four bytes, and the head
/// checksum adjustment.
fn assemble(tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut power = 1u16;
    let mut selector = 0u16;
    while power * 2 <= count {
        power *= 2;
        selector += 1;
    }
    let mut out = Writer(Vec::new());
    out.u32(0x0001_0000);
    out.u16(count);
    out.u16(power * 16);
    out.u16(selector);
    out.u16(count * 16 - power * 16);
    let mut offset = 12 + 16 * tables.len();
    let mut head_offset = 0;
    for (tag, data) in &tables {
        out.0.extend_from_slice(tag);
        out.u32(checksum(data));
        out.u32(offset as u32);
        out.u32(data.len() as u32);
        if tag == b"head" {
            head_offset = offset;
        }
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.0.extend_from_slice(data);
        while !out.0.len().is_multiple_of(4) {
            out.0.push(0);
        }
    }
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out.0));
    out.0[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    out.0
}

fn head_table(spec: &FontSpec, bounds: Bounds) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000);
    w.u32(0x0001_0000); // font revision 1.0
    w.u32(0); // checksum adjustment, filled in at the end
    w.u32(0x5F0F_3CF5);
    w.u16(0x0003); // baseline at y = 0, left side bearing at x = 0
    w.u16(spec.units_per_em);
    w.i64(0); // created
    w.i64(0); // modified
    for v in [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max] {
        w.i16(clamp16(v));
    }
    w.u16(0); // mac style: regular
    w.u16(3); // lowest recommended ppem
    w.i16(2); // font direction hint
    w.i16(1); // long loca offsets
    w.i16(0);
    w.0
}

fn hhea_table(spec: &FontSpec, bounds: &[Option<Bounds>]) -> Vec<u8> {
    let advance_max = spec.glyphs.iter().map(|g| g.advance).max().unwrap_or(0);
    let mut min_lsb = i32::MAX;
    let mut min_rsb = i32::MAX;
    let mut max_extent = i32::MIN;
    for (glyph, b) in spec.glyphs.iter().zip(bounds) {
        if let Some(b) = b {
            min_lsb = min_lsb.min(b.x_min);
            min_rsb = min_rsb.min(i32::from(glyph.advance) - b.x_max);
            max_extent = max_extent.max(b.x_max);
        }
    }
    let or_zero = |v: i32, unset: i32| if v == unset { 0 } else { v };
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000);
    w.i16(spec.ascent);
    w.i16(spec.descent);
    w.i16(0); // line gap
    w.u16(advance_max);
    w.i16(clamp16(or_zero(min_lsb, i32::MAX)));
    w.i16(clamp16(or_zero(min_rsb, i32::MAX)));
    w.i16(clamp16(or_zero(max_extent, i32::MIN)));
    w.i16(1); // caret slope rise
    w.i16(0); // caret slope run
    w.i16(0); // caret offset
    for _ in 0..4 {
        w.i16(0);
    }
    w.i16(0); // metric data format
    w.u16(spec.glyphs.len() as u16);
    w.0
}

fn hmtx_table(spec: &FontSpec, bounds: &[Option<Bounds>]) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    for (glyph, b) in spec.glyphs.iter().zip(bounds) {
        w.u16(glyph.advance);
        w.i16(clamp16(b.map(|b| b.x_min).unwrap_or(0)));
    }
    w.0
}

pub(super) fn maxp_table(spec: &FontSpec) -> Vec<u8> {
    let max_points = spec
        .glyphs
        .iter()
        .map(|g| g.contours.iter().map(Vec::len).sum::<usize>())
        .max()
        .unwrap_or(0);
    let max_contours = spec
        .glyphs
        .iter()
        .map(|g| g.contours.len())
        .max()
        .unwrap_or(0);
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000);
    w.u16(spec.glyphs.len() as u16);
    w.u16(max_points as u16);
    w.u16(max_contours as u16);
    w.u16(0); // composite points
    w.u16(0); // composite contours
    w.u16(2); // zones
    for _ in 0..8 {
        w.u16(0); // twilight points, storage, function and instruction defs, stack, instructions, components, depth
    }
    w.0
}

fn post_table() -> Vec<u8> {
    let mut w = Writer(Vec::new());
    w.u32(0x0003_0000); // format 3: no glyph names
    w.u32(0); // italic angle
    w.i16(-100); // underline position
    w.i16(50); // underline thickness
    w.u32(0); // not fixed pitch
    for _ in 0..4 {
        w.u32(0);
    }
    w.0
}

fn name_table(family: &str) -> Vec<u8> {
    let postscript: String = family.chars().filter(|c| !c.is_whitespace()).collect();
    let names = [
        (1u16, family.to_string()),
        (2, "Regular".to_string()),
        (3, format!("{postscript};letters")),
        (4, format!("{family} Regular")),
        (6, format!("{postscript}-Regular")),
    ];
    // macintosh roman records first, then windows unicode ones: the table is sorted by platform
    let mut records: Vec<(u16, u16, u16, u16, Vec<u8>)> = Vec::new();
    for (id, text) in &names {
        let ascii: Vec<u8> = text
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect();
        records.push((1, 0, 0, *id, ascii));
    }
    for (id, text) in &names {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        records.push((3, 1, 0x409, *id, utf16));
    }
    let mut w = Writer(Vec::new());
    w.u16(0);
    w.u16(records.len() as u16);
    w.u16((6 + 12 * records.len()) as u16);
    let mut storage = Vec::new();
    for (platform, encoding, language, id, bytes) in &records {
        w.u16(*platform);
        w.u16(*encoding);
        w.u16(*language);
        w.u16(*id);
        w.u16(bytes.len() as u16);
        w.u16(storage.len() as u16);
        storage.extend_from_slice(bytes);
    }
    w.0.extend(storage);
    w.0
}

/// unicode range bits of os/2 for the chars in the font (only the blocks
/// that occur in latin documents, plus the supplementary private use area).
fn unicode_ranges(codes: &[u32]) -> [u32; 4] {
    const BLOCKS: &[(u32, u32, usize)] = &[
        (0x0000, 0x007F, 0),
        (0x0080, 0x00FF, 1),
        (0x0100, 0x017F, 2),
        (0x0180, 0x024F, 3),
        (0x0250, 0x02AF, 4),
        (0x02B0, 0x02FF, 5),
        (0x0300, 0x036F, 6),
        (0x0370, 0x03FF, 7),
        (0x0400, 0x04FF, 9),
        (0x2000, 0x206F, 31),
        (0x2070, 0x209F, 32),
        (0x20A0, 0x20CF, 33),
        (0x2100, 0x214F, 37),
        (0x2190, 0x21FF, 37),
        (0x2200, 0x22FF, 38),
        (0x25A0, 0x25FF, 45),
        (0xE000, 0xF8FF, 60),
        (0xFB00, 0xFB4F, 62),
        (0xF0000, 0xFFFFD, 90),
    ];
    let mut bits = [0u32; 4];
    for &code in codes {
        if code > 0xFFFF {
            bits[57 / 32] |= 1 << (57 % 32); // non-plane 0
        }
        for &(low, high, bit) in BLOCKS {
            if (low..=high).contains(&code) {
                bits[bit / 32] |= 1 << (bit % 32);
            }
        }
    }
    bits
}

fn os2_table(spec: &FontSpec, bounds: &[Option<Bounds>]) -> Vec<u8> {
    let upm = i32::from(spec.units_per_em);
    let drawn: Vec<u16> = spec
        .glyphs
        .iter()
        .zip(bounds)
        .filter(|(_, b)| b.is_some())
        .map(|(g, _)| g.advance)
        .collect();
    let average = if drawn.is_empty() {
        0
    } else {
        drawn.iter().map(|&a| i64::from(a)).sum::<i64>() / drawn.len() as i64
    };
    let codes: Vec<u32> = spec.cmap.keys().copied().collect();
    let first = codes.first().copied().unwrap_or(0x20).min(0xFFFF) as u16;
    let last = codes.last().copied().unwrap_or(0x20).min(0xFFFF) as u16;
    let mut w = Writer(Vec::new());
    w.u16(4);
    w.i16(clamp16(average as i32));
    w.u16(400); // weight
    w.u16(5); // width
    w.u16(spec.fs_type);
    let sub = clamp16(upm * 65 / 100);
    for v in [
        sub,
        sub,
        0,
        clamp16(upm * 14 / 100),
        sub,
        sub,
        0,
        clamp16(upm * 48 / 100),
    ] {
        w.i16(v); // subscript and superscript size and offsets
    }
    w.i16(clamp16(upm * 5 / 100)); // strikeout size
    w.i16(clamp16(upm * 26 / 100)); // strikeout position
    w.i16(0); // family class
    w.0.extend_from_slice(&[0; 10]); // panose
    for bits in unicode_ranges(&codes) {
        w.u32(bits);
    }
    w.0.extend_from_slice(b"LTRS");
    w.u16(0x40); // regular
    w.u16(first);
    w.u16(last);
    w.i16(spec.ascent);
    w.i16(spec.descent);
    w.i16(0); // typo line gap
    w.u16(spec.ascent.max(0) as u16);
    w.u16(spec.descent.unsigned_abs());
    w.u32(CODE_PAGE_LATIN_1);
    w.u32(0);
    w.i16(clamp16(upm / 2)); // x height
    w.i16(clamp16(upm * 7 / 10)); // cap height
    w.u16(0); // default char
    w.u16(0x20); // break char
    w.u16(1); // max context
    w.0
}

/// a cmap with format 4 for the basic plane and format 12 for everything,
/// the way font tools write it when there are chars above U+FFFF.
fn cmap_table(cmap: &BTreeMap<u32, u16>) -> Vec<u8> {
    let format4 = cmap_format4(cmap);
    let format12 = cmap
        .keys()
        .any(|&c| c > 0xFFFF)
        .then(|| cmap_format12(cmap));
    let mut subtables: Vec<(u16, u16, usize)> = vec![(0, 3, 4), (3, 1, 4)];
    if format12.is_some() {
        subtables.push((0, 4, 12));
        subtables.push((3, 10, 12));
    }
    subtables.sort();
    let header_len = 4 + 8 * subtables.len();
    let offset_of = |format: usize| -> u32 {
        if format == 4 {
            header_len as u32
        } else {
            (header_len + format4.len()) as u32
        }
    };
    let mut w = Writer(Vec::new());
    w.u16(0);
    w.u16(subtables.len() as u16);
    for (platform, encoding, format) in &subtables {
        w.u16(*platform);
        w.u16(*encoding);
        w.u32(offset_of(*format));
    }
    w.0.extend(&format4);
    if let Some(table) = format12 {
        w.0.extend(table);
    }
    w.0
}

/// runs of consecutive codes whose glyph indexes are also consecutive.
fn consecutive_groups(cmap: &BTreeMap<u32, u16>, limit: u32) -> Vec<(u32, u32, u16)> {
    let mut groups: Vec<(u32, u32, u16)> = Vec::new();
    for (&code, &glyph) in cmap.range(..=limit) {
        if let Some(last) = groups.last_mut() {
            if code == last.1 + 1 && u32::from(glyph) == u32::from(last.2) + (code - last.0) {
                last.1 = code;
                continue;
            }
        }
        groups.push((code, code, glyph));
    }
    groups
}

fn cmap_format4(cmap: &BTreeMap<u32, u16>) -> Vec<u8> {
    let mut segments: Vec<(u16, u16, i16)> = consecutive_groups(cmap, 0xFFFE)
        .into_iter()
        .map(|(start, end, glyph)| {
            (
                start as u16,
                end as u16,
                (i32::from(glyph) - start as i32) as i16,
            )
        })
        .collect();
    segments.push((0xFFFF, 0xFFFF, 1));
    let count = segments.len() as u16;
    let mut power = 1u16;
    let mut selector = 0u16;
    while power * 2 <= count {
        power *= 2;
        selector += 1;
    }
    let mut w = Writer(Vec::new());
    w.u16(4);
    w.u16(16 + 8 * count);
    w.u16(0);
    w.u16(count * 2);
    w.u16(power * 2);
    w.u16(selector);
    w.u16(count * 2 - power * 2);
    for (_, end, _) in &segments {
        w.u16(*end);
    }
    w.u16(0);
    for (start, _, _) in &segments {
        w.u16(*start);
    }
    for (_, _, delta) in &segments {
        w.i16(*delta);
    }
    for _ in &segments {
        w.u16(0);
    }
    w.0
}

fn cmap_format12(cmap: &BTreeMap<u32, u16>) -> Vec<u8> {
    let groups = consecutive_groups(cmap, u32::MAX);
    let mut w = Writer(Vec::new());
    w.u16(12);
    w.u16(0);
    w.u32((16 + 12 * groups.len()) as u32);
    w.u32(0);
    w.u32(groups.len() as u32);
    for (start, end, glyph) in groups {
        w.u32(start);
        w.u32(end);
        w.u32(u32::from(glyph));
    }
    w.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: i32) -> Contours {
        let p = |x, y| ContourPoint {
            x,
            y,
            on_curve: true,
        };
        vec![vec![p(0, 0), p(0, size), p(size, size), p(size, 0)]]
    }

    fn sample_font(cmap: &BTreeMap<u32, u16>) -> Vec<u8> {
        let glyphs = vec![
            FontGlyph {
                contours: Vec::new(),
                advance: 500,
            },
            FontGlyph {
                contours: Vec::new(),
                advance: 250,
            },
            FontGlyph {
                contours: square(400),
                advance: 600,
            },
            FontGlyph {
                contours: square(300),
                advance: 400,
            },
        ];
        write_truetype(&FontSpec {
            family: "Ltr Test",
            units_per_em: 1000,
            glyphs: &glyphs,
            cmap,
            ascent: 800,
            descent: -200,
            fs_type: 8,
        })
    }

    #[test]
    fn written_fonts_read_back() {
        let cmap = BTreeMap::from([(0x20, 1), (0x41, 2), (0xF0000, 3)]);
        let bytes = sample_font(&cmap);
        let face = ttf_parser::Face::parse(&bytes, 0).expect("a valid font");
        assert_eq!(face.units_per_em(), 1000);
        assert_eq!(face.number_of_glyphs(), 4);
        let a = face.glyph_index('A').unwrap();
        assert_eq!(a.0, 2);
        assert_eq!(face.glyph_hor_advance(a), Some(600));
        let private = face.glyph_index(char::from_u32(0xF0000).unwrap()).unwrap();
        assert_eq!(private.0, 3);
        assert_eq!(face.glyph_bounding_box(private).map(|b| b.x_max), Some(300));
        let os2 = face.tables().os2.unwrap();
        assert_eq!(os2.permissions(), Some(ttf_parser::Permissions::Editable));
        let family = face
            .names()
            .into_iter()
            .find(|n| n.name_id == 1 && n.platform_id == ttf_parser::PlatformId::Windows)
            .and_then(|n| n.to_string());
        assert_eq!(family.as_deref(), Some("Ltr Test"));
    }

    #[test]
    fn latin_1_is_always_claimed() {
        let bytes = sample_font(&BTreeMap::from([(0x41, 2)]));
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let raw = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"OS/2"))
            .unwrap();
        // ulCodePageRange1 sits at byte 78
        assert_eq!(u32::from_be_bytes(raw[78..82].try_into().unwrap()) & 1, 1);
    }

    #[test]
    fn checksums_add_up() {
        let bytes = sample_font(&BTreeMap::from([(0x41, 2)]));
        assert_eq!(checksum(&bytes), 0xB1B0_AFBA);
    }

    #[test]
    fn large_coordinates_use_full_words() {
        let glyph = glyph_bytes(&square(1000));
        let font_glyph = vec![
            FontGlyph {
                contours: Vec::new(),
                advance: 0,
            },
            FontGlyph {
                contours: square(1000),
                advance: 1000,
            },
        ];
        let cmap = BTreeMap::from([(0x41, 1)]);
        let bytes = write_truetype(&FontSpec {
            family: "Big",
            units_per_em: 2048,
            glyphs: &font_glyph,
            cmap: &cmap,
            ascent: 1800,
            descent: -400,
            fs_type: 0,
        });
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert_eq!(
            face.glyph_bounding_box(ttf_parser::GlyphId(1))
                .map(|b| b.y_max),
            Some(1000)
        );
        assert!(!glyph.is_empty());
    }
}
