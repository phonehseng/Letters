//! fonts for the pdf: each face used is embedded as a composite
//! (cid-keyed) font with only the glyphs used, and a /ToUnicode map so the
//! text can be searched and copied.
//!
//! truetype faces are subset by emptying the glyphs that are not used
//! while every glyph keeps its number, so glyph numbers can be the codes
//! (/Identity-H with /CIDToGIDMap /Identity) and nothing is renumbered.
//! faces with cff outlines are embedded whole as opentype (FontFile3).

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::fonts::LoadedFace;

/// what a page needs from one embedded font: the glyphs it used and the
/// text each one stands for.
#[derive(Default)]
pub struct FontUse {
    pub glyphs: BTreeMap<u16, String>,
}

/// may the font be embedded at all (os/2 fsType, opentype spec)?
/// bit 1 alone is "restricted licence": no embedding.
pub fn may_embed(face: &LoadedFace) -> bool {
    let bits = face.metrics.embedding_bits;
    !(bits & 0x000f == 0x0002)
}

/// may it be subset (bit 8 forbids subsetting)?
fn may_subset(face: &LoadedFace) -> bool {
    face.metrics.embedding_bits & 0x0100 == 0
}

fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

/// adds the objects of one composite font and returns the font object.
pub fn add_font(
    document: &mut lopdf::Document,
    face: &LoadedFace,
    uses: &FontUse,
    tag: &str,
) -> ObjectId {
    let used: BTreeSet<u16> = uses.glyphs.keys().copied().collect();
    let base_font = format!("{tag}+{}", sanitise(&face.postscript_name));
    let scale = 1000.0 / face.metrics.units_per_em.max(1.0);
    let embedded = may_embed(face);
    let program = if embedded {
        font_program(face, &used)
    } else {
        None
    };
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", name("FontDescriptor"));
    descriptor.set("FontName", name(&base_font));
    descriptor.set("Flags", Object::Integer(32));
    let parsed = face.parse();
    let bbox = parsed.as_ref().map(|f| f.global_bounding_box());
    descriptor.set(
        "FontBBox",
        Object::Array(match bbox {
            Some(b) => vec![
                real(f64::from(b.x_min) * scale),
                real(f64::from(b.y_min) * scale),
                real(f64::from(b.x_max) * scale),
                real(f64::from(b.y_max) * scale),
            ],
            None => vec![real(0.0), real(-200.0), real(1000.0), real(800.0)],
        }),
    );
    descriptor.set("ItalicAngle", real(face.metrics.italic_angle));
    descriptor.set("Ascent", real(face.metrics.hhea_ascent * scale));
    descriptor.set("Descent", real(face.metrics.hhea_descent * scale));
    descriptor.set("CapHeight", real(face.metrics.cap_height * scale));
    descriptor.set("StemV", Object::Integer(80));
    let cff = !face.has_glyf_outlines;
    if let Some(bytes) = program {
        let mut stream_dict = Dictionary::new();
        if cff {
            stream_dict.set("Subtype", name("OpenType"));
        } else {
            stream_dict.set("Length1", Object::Integer(bytes.len() as i64));
        }
        let mut stream = Stream::new(stream_dict, bytes);
        let _ = stream.compress();
        let file = document.add_object(stream);
        descriptor.set(
            if cff { "FontFile3" } else { "FontFile2" },
            Object::Reference(file),
        );
    }
    let descriptor_id = document.add_object(descriptor);
    let mut widths = Vec::new();
    for glyph in &used {
        widths.push(Object::Integer(i64::from(*glyph)));
        let width = (f64::from(face.advance_of_glyph(*glyph)) * scale).round() as i64;
        widths.push(Object::Array(vec![Object::Integer(width)]));
    }
    let mut system_info = Dictionary::new();
    system_info.set("Registry", Object::string_literal("Adobe"));
    system_info.set("Ordering", Object::string_literal("Identity"));
    system_info.set("Supplement", Object::Integer(0));
    let mut descendant = Dictionary::new();
    descendant.set("Type", name("Font"));
    descendant.set(
        "Subtype",
        name(if cff { "CIDFontType0" } else { "CIDFontType2" }),
    );
    descendant.set("BaseFont", name(&base_font));
    descendant.set("CIDSystemInfo", Object::Dictionary(system_info));
    descendant.set("FontDescriptor", Object::Reference(descriptor_id));
    descendant.set("DW", Object::Integer(1000));
    descendant.set("W", Object::Array(widths));
    if !cff {
        descendant.set("CIDToGIDMap", name("Identity"));
    }
    let descendant_id = document.add_object(descendant);
    let mut to_unicode = Stream::new(Dictionary::new(), to_unicode_cmap(&uses.glyphs));
    let _ = to_unicode.compress();
    let to_unicode_id = document.add_object(to_unicode);
    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type0"));
    font.set("BaseFont", name(&base_font));
    font.set("Encoding", name("Identity-H"));
    font.set(
        "DescendantFonts",
        Object::Array(vec![Object::Reference(descendant_id)]),
    );
    font.set("ToUnicode", Object::Reference(to_unicode_id));
    document.add_object(font)
}

fn sanitise(text: &str) -> String {
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if clean.is_empty() {
        "Font".into()
    } else {
        clean
    }
}

/// the bytes to embed: a subset for truetype outlines (when the licence
/// allows), the whole single font otherwise.
fn font_program(face: &LoadedFace, used: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let tables = table_directory(&face.bytes, face.index)?;
    if face.has_glyf_outlines && may_subset(face) {
        return subset(&face.bytes, &tables, used);
    }
    Some(write_font(
        &tables
            .into_iter()
            .map(|(tag, range)| (tag, face.bytes[range].to_vec()))
            .collect(),
    ))
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// where each table of the face at `index` sits in the file.
fn table_directory(data: &[u8], index: u32) -> Option<BTreeMap<[u8; 4], std::ops::Range<usize>>> {
    let start = if data.starts_with(b"ttcf") {
        read_u32(data, 12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let count = usize::from(read_u16(data, start + 4)?);
    let mut tables = BTreeMap::new();
    for record in 0..count {
        let at = start + 12 + record * 16;
        let tag: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
        let offset = read_u32(data, at + 8)? as usize;
        let length = read_u32(data, at + 12)? as usize;
        if offset.checked_add(length)? <= data.len() {
            tables.insert(tag, offset..offset + length);
        }
    }
    Some(tables)
}

/// a copy keeping only `used` glyphs (and the glyphs composites are built
/// from), every glyph number unchanged.
fn subset(
    data: &[u8],
    tables: &BTreeMap<[u8; 4], std::ops::Range<usize>>,
    used: &BTreeSet<u16>,
) -> Option<Vec<u8>> {
    let table = |tag: &[u8; 4]| tables.get(tag).map(|range| &data[range.clone()]);
    let (head, loca, glyf, maxp) = (
        table(b"head")?,
        table(b"loca")?,
        table(b"glyf")?,
        table(b"maxp")?,
    );
    let glyph_count = usize::from(read_u16(maxp, 4)?);
    let long_offsets = head.get(50..52).is_some_and(|format| format == [0, 1]);
    let offsets: Vec<usize> = (0..=glyph_count)
        .map(|glyph| {
            if long_offsets {
                read_u32(loca, glyph * 4).map(|o| o as usize)
            } else {
                read_u16(loca, glyph * 2).map(|o| usize::from(o) * 2)
            }
        })
        .collect::<Option<Vec<usize>>>()?;
    let keep = with_components(glyf, &offsets, used);
    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((glyph_count + 1) * 4);
    for glyph in 0..glyph_count {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        // glyph 0, the "missing" glyph, is always kept
        if glyph == 0 || keep.contains(&(glyph as u16)) {
            if let Some(bytes) = glyf.get(offsets[glyph]..offsets[glyph + 1]) {
                new_glyf.extend_from_slice(bytes);
                while new_glyf.len() % 4 != 0 {
                    new_glyf.push(0);
                }
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head.to_vec();
    if new_head.len() >= 54 {
        new_head[50..52].copy_from_slice(&[0, 1]);
        new_head[8..12].copy_from_slice(&[0; 4]);
    }
    let mut output: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
    output.insert(*b"head", new_head);
    output.insert(*b"loca", new_loca);
    output.insert(*b"glyf", new_glyf);
    // the tables a pdf reader needs to draw truetype glyphs
    for tag in [
        b"hhea", b"hmtx", b"maxp", b"cvt ", b"fpgm", b"prep", b"OS/2", b"name", b"post",
    ] {
        if let Some(bytes) = table(tag) {
            output.insert(*tag, bytes.to_vec());
        }
    }
    Some(write_font(&output))
}

fn with_components(glyf: &[u8], offsets: &[usize], wanted: &BTreeSet<u16>) -> BTreeSet<u16> {
    let mut keep = BTreeSet::new();
    let mut waiting: Vec<u16> = wanted.iter().copied().collect();
    while let Some(glyph) = waiting.pop() {
        if !keep.insert(glyph) {
            continue;
        }
        let Some(range) = offsets.get(usize::from(glyph)..usize::from(glyph) + 2) else {
            continue;
        };
        if let Some(bytes) = glyf.get(range[0]..range[1]) {
            waiting.extend(components(bytes));
        }
    }
    keep
}

/// the glyphs a composite glyph refers to (none for a simple glyph).
fn components(glyph: &[u8]) -> Vec<u16> {
    const ARGS_ARE_WORDS: u16 = 0x1;
    const HAS_SCALE: u16 = 0x8;
    const MORE_COMPONENTS: u16 = 0x20;
    const HAS_XY_SCALE: u16 = 0x40;
    const HAS_TWO_BY_TWO: u16 = 0x80;
    let mut found = Vec::new();
    if glyph.len() < 10 || i16::from_be_bytes([glyph[0], glyph[1]]) >= 0 {
        return found;
    }
    let mut at = 10;
    while let (Some(flags), Some(component)) = (read_u16(glyph, at), read_u16(glyph, at + 2)) {
        found.push(component);
        at += 4 + if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
        at += if flags & HAS_SCALE != 0 {
            2
        } else if flags & HAS_XY_SCALE != 0 {
            4
        } else if flags & HAS_TWO_BY_TWO != 0 {
            8
        } else {
            0
        };
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    found
}

/// writes tables into a font file with the directory readers expect.
fn write_font(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut power = 1u16;
    while power * 2 <= count {
        power *= 2;
    }
    let mut file = Vec::new();
    file.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    file.extend_from_slice(&count.to_be_bytes());
    file.extend_from_slice(&(power * 16).to_be_bytes());
    file.extend_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    file.extend_from_slice(&(count * 16 - power * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, bytes) in tables {
        file.extend_from_slice(tag);
        file.extend_from_slice(&checksum(bytes).to_be_bytes());
        file.extend_from_slice(&(offset as u32).to_be_bytes());
        file.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(bytes);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    file.extend_from_slice(&body);
    file
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// a /ToUnicode cmap from two-byte glyph codes to their text.
pub fn to_unicode_cmap(glyphs: &BTreeMap<u16, String>) -> Vec<u8> {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &String)> =
        glyphs.iter().filter(|(_, text)| !text.is_empty()).collect();
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, text) in chunk {
            let hex: String = text
                .encode_utf16()
                .map(|unit| format!("{unit:04X}"))
                .collect();
            out.push_str(&format!("<{glyph:04X}> <{hex}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_unicode_lists_every_glyph() {
        let mut glyphs = BTreeMap::new();
        glyphs.insert(3u16, " ".to_string());
        glyphs.insert(68u16, "a".to_string());
        glyphs.insert(500u16, "fi".to_string());
        let text = String::from_utf8(to_unicode_cmap(&glyphs)).unwrap();
        assert!(text.contains("3 beginbfchar"));
        assert!(text.contains("<0044> <0061>"));
        assert!(text.contains("<01F4> <00660069>"));
    }

    #[test]
    fn written_fonts_list_their_tables() {
        let mut tables = BTreeMap::new();
        tables.insert(*b"head", vec![0u8; 54]);
        tables.insert(*b"maxp", vec![0u8; 6]);
        let font = write_font(&tables);
        assert_eq!(read_u16(&font, 4), Some(2));
        assert_eq!(&font[12..16], b"head");
    }
}
