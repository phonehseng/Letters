//! reading and subsetting TrueType font programs.
//!
//! reading uses ttf-parser (MIT or Apache-2.0). subsetting is done here:
//! the glyphs that aren't needed are emptied but every glyph keeps its
//! number, so a composite font can use glyph numbers directly as CIDs
//! (CIDToGIDMap /Identity) and nothing has to be renumbered.

use std::collections::{BTreeMap, BTreeSet};

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::error::EditError;

/// a TrueType or OpenType font, ready to be asked about its glyphs.
pub(crate) struct FontProgram<'a> {
    pub face: Face<'a>,
    pub data: &'a [u8],
    pub index: u32,
}

/// what the font's licence (the OS/2 fsType bits) allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Embedding {
    Allowed,
    /// may be embedded, but only whole: the font forbids subsetting
    WholeFontOnly,
    Forbidden,
}

struct Ignore;

impl OutlineBuilder for Ignore {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {}
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    fn close(&mut self) {}
}

impl<'a> FontProgram<'a> {
    pub fn parse(data: &'a [u8], index: u32) -> Option<Self> {
        Some(Self {
            face: Face::parse(data, index).ok()?,
            data,
            index,
        })
    }

    /// true when the glyph exists and draws something. white space has no
    /// outline but still exists, so the caller allows spaces separately.
    pub fn has_outline(&self, glyph: u16) -> bool {
        glyph != 0
            && glyph < self.face.number_of_glyphs()
            && self
                .face
                .outline_glyph(GlyphId(glyph), &mut Ignore)
                .is_some()
    }

    pub fn glyph_exists(&self, glyph: u16) -> bool {
        glyph != 0 && glyph < self.face.number_of_glyphs()
    }

    /// the glyph for a character through the font's unicode cmap.
    pub fn glyph_for_char(&self, character: char) -> Option<u16> {
        self.face
            .glyph_index(character)
            .map(|glyph| glyph.0)
            .filter(|glyph| *glyph != 0)
    }

    /// the glyph a one-byte code reaches through the font's own (symbolic
    /// or mac) cmaps, the way readers look it up for symbolic fonts.
    pub fn glyph_for_code(&self, code: u8) -> Option<u16> {
        let cmap = self.face.tables().cmap?;
        let lookups = [
            (3, 0, 0xf000 + u32::from(code)),
            (3, 0, u32::from(code)),
            (1, 0, u32::from(code)),
        ];
        lookups.iter().find_map(|(platform, encoding, value)| {
            cmap.subtables
                .into_iter()
                .filter(|table| {
                    table.platform_id as u16 == *platform && table.encoding_id == *encoding
                })
                .find_map(|table| table.glyph_index(*value))
                .map(|glyph| glyph.0)
                .filter(|glyph| *glyph != 0)
        })
    }

    /// a glyph's advance in thousandths of the font size, as pdf widths are.
    pub fn advance(&self, glyph: u16) -> f32 {
        let units = f32::from(self.face.units_per_em().max(1));
        let advance = self.face.glyph_hor_advance(GlyphId(glyph)).unwrap_or(0);
        f32::from(advance) * 1000.0 / units
    }

    pub fn scale(&self, value: i16) -> f32 {
        f32::from(value) * 1000.0 / f32::from(self.face.units_per_em().max(1))
    }

    pub fn embedding(&self) -> Embedding {
        use ttf_parser::Permissions;
        if !self.face.is_outline_embedding_allowed() {
            return Embedding::Forbidden;
        }
        match self.face.permissions() {
            Some(Permissions::Restricted) => Embedding::Forbidden,
            _ if !self.face.is_subsetting_allowed() => Embedding::WholeFontOnly,
            _ => Embedding::Allowed,
        }
    }

    /// the font's PostScript name, e.g. "ArialMT".
    pub fn postscript_name(&self) -> Option<String> {
        self.name(ttf_parser::name_id::POST_SCRIPT_NAME)
    }

    pub fn family_name(&self) -> Option<String> {
        self.name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
            .or_else(|| self.name(ttf_parser::name_id::FAMILY))
    }

    fn name(&self, id: u16) -> Option<String> {
        self.face
            .names()
            .into_iter()
            .filter(|name| name.name_id == id)
            .find_map(|name| name.to_string())
    }

    pub fn is_truetype_outlines(&self) -> bool {
        self.face.tables().glyf.is_some()
    }

    /// the bytes of this one font, even when it came from a collection.
    pub fn single_font_bytes(&self) -> Result<Vec<u8>, EditError> {
        if self.index == 0 && !self.data.starts_with(b"ttcf") {
            return Ok(self.data.to_vec());
        }
        let tables = table_directory(self.data, self.index).ok_or_else(damaged_font)?;
        Ok(write_font(
            &tables
                .into_iter()
                .map(|(tag, range)| (tag, self.data[range].to_vec()))
                .collect(),
        ))
    }

    /// a copy of the font that keeps only `glyphs` (and the glyphs they
    /// are built from), with every glyph number unchanged.
    pub fn subset(&self, glyphs: &BTreeSet<u16>) -> Result<Vec<u8>, EditError> {
        let tables = table_directory(self.data, self.index).ok_or_else(damaged_font)?;
        let table = |tag: &[u8; 4]| tables.get(tag).map(|range| &self.data[range.clone()]);
        let (Some(head), Some(loca), Some(glyf), Some(maxp)) = (
            table(b"head"),
            table(b"loca"),
            table(b"glyf"),
            table(b"maxp"),
        ) else {
            return Err(EditError::UnsupportedFont(
                "Only fonts with TrueType outlines (most .ttf files) can be added.".into(),
            ));
        };
        let glyph_count = usize::from(u16::from_be_bytes([maxp[4], maxp[5]]));
        let long_offsets = head.get(50..52).is_some_and(|format| format == [0, 1]);
        let offsets = read_loca(loca, glyph_count, long_offsets).ok_or_else(damaged_font)?;
        let keep = with_components(glyf, &offsets, glyphs);

        let mut new_glyf = Vec::new();
        let mut new_loca = Vec::with_capacity((glyph_count + 1) * 4);
        for glyph in 0..glyph_count {
            new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
            // glyph 0 (the "missing" glyph) is always kept, as readers expect
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
        // long loca offsets, and a checksum adjustment readers recompute
        new_head[50..52].copy_from_slice(&[0, 1]);
        new_head[8..12].copy_from_slice(&[0; 4]);
        let mut output: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
        output.insert(*b"head", new_head);
        output.insert(*b"loca", new_loca);
        output.insert(*b"glyf", new_glyf);
        // the tables a pdf reader needs to draw TrueType glyphs
        for tag in [
            b"hhea", b"hmtx", b"maxp", b"cvt ", b"fpgm", b"prep", b"OS/2", b"name", b"post",
        ] {
            if let Some(bytes) = table(tag) {
                output.insert(*tag, bytes.to_vec());
            }
        }
        Ok(write_font(&output))
    }
}

fn damaged_font() -> EditError {
    EditError::UnsupportedFont("The font file is damaged.".into())
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// where each table of the font at `index` sits in the file.
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

fn read_loca(loca: &[u8], glyph_count: usize, long: bool) -> Option<Vec<usize>> {
    (0..=glyph_count)
        .map(|glyph| {
            if long {
                read_u32(loca, glyph * 4).map(|offset| offset as usize)
            } else {
                read_u16(loca, glyph * 2).map(|offset| usize::from(offset) * 2)
            }
        })
        .collect()
}

/// the wanted glyphs plus every glyph a composite glyph is built from.
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
        let Some(bytes) = glyf.get(range[0]..range[1]) else {
            continue;
        };
        waiting.extend(components(bytes));
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

/// writes tables into a font file, with the directory readers expect.
pub(crate) fn write_font(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_glyphs_name_their_parts() {
        // a composite glyph header (-1 contours), then two components:
        // glyph 7 with byte arguments and more to come, then glyph 9
        let mut glyph = vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
        glyph.extend_from_slice(&[0x00, 0x20, 0x00, 0x07, 1, 2]);
        glyph.extend_from_slice(&[0x00, 0x00, 0x00, 0x09, 3, 4]);
        assert_eq!(components(&glyph), [7, 9]);
        assert!(components(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0]).is_empty());
    }

    #[test]
    fn written_fonts_list_their_tables() {
        let tables = BTreeMap::from([(*b"abcd", vec![1, 2, 3]), (*b"wxyz", vec![9; 8])]);
        let file = write_font(&tables);
        let directory = table_directory(&file, 0).unwrap();
        assert_eq!(&file[directory[b"abcd"].clone()], &[1, 2, 3]);
        assert_eq!(&file[directory[b"wxyz"].clone()], &[9; 8]);
    }
}
