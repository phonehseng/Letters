//! the bytes to embed a font in a pdf or a document: a subset when the
//! font has TrueType outlines and its licence allows subsetting, else the
//! whole font. the licence (fsType) is checked first.
//!
//! the TrueType subsetter began as core/edit's (fonts/truetype.rs). it is
//! generalised here so the pdf and docx writers can share one; core/edit
//! should move to this crate's `embedding_data` later. glyphs that
//! aren't needed are emptied but every glyph keeps its number, so a
//! composite pdf font can use glyph numbers directly as cids
//! (CIDToGIDMap /Identity) and nothing is renumbered.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use ttf_parser::Face;

use crate::error::FontError;
use crate::model::Embedding;
use crate::sfnt::{read_u16, read_u32, single_font, table_directory, write_font, Tables};

/// what kind of font program the embedding holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmbedKind {
    /// TrueType outlines with only the glyphs used (pdf FontFile2)
    TrueTypeSubset,
    /// the whole TrueType font, because its licence bars subsetting
    WholeTrueType,
    /// the whole OpenType (CFF) font (pdf FontFile3 /OpenType)
    WholeOpenType,
}

/// a font program ready to embed.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedData {
    pub kind: EmbedKind,
    pub bytes: Vec<u8>,
    /// the glyphs the text needs, glyph 0 included
    pub glyphs: BTreeSet<u16>,
    pub postscript_name: String,
}

/// the font program to embed for `text`, or why it may not be embedded.
pub fn embedding_data(data: &[u8], index: u32, text: &str) -> Result<EmbedData, FontError> {
    let face = Face::parse(data, index).map_err(|_| FontError::Damaged("the font".into()))?;
    let os2 = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"OS/2"))
        .unwrap_or(&[]);
    let embedding = Embedding::from_fs_type(read_u16(os2, 8).unwrap_or(0));
    if !embedding.allows_embedding() {
        return Err(FontError::EmbeddingNotAllowed);
    }
    let mut glyphs: BTreeSet<u16> = text
        .chars()
        .filter_map(|c| face.glyph_index(c))
        .map(|glyph| glyph.0)
        .collect();
    glyphs.insert(0);
    let postscript_name =
        crate::facts::name(&face, ttf_parser::name_id::POST_SCRIPT_NAME).unwrap_or_default();
    let truetype = face.tables().glyf.is_some();
    let (kind, bytes) = if truetype && embedding.subsetting {
        (
            EmbedKind::TrueTypeSubset,
            subset_truetype(data, index, &glyphs)?,
        )
    } else {
        let whole =
            single_font(data, index).ok_or_else(|| FontError::Damaged("the font".into()))?;
        (
            if truetype {
                EmbedKind::WholeTrueType
            } else {
                EmbedKind::WholeOpenType
            },
            whole,
        )
    };
    Ok(EmbedData {
        kind,
        bytes,
        glyphs,
        postscript_name,
    })
}

/// a copy of the font that keeps only `glyphs` (and the glyphs they are
/// built from), with every glyph number unchanged.
pub fn subset_truetype(
    data: &[u8],
    index: u32,
    glyphs: &BTreeSet<u16>,
) -> Result<Vec<u8>, FontError> {
    let damaged = || FontError::Damaged("the font".into());
    let tables = table_directory(data, index).ok_or_else(damaged)?;
    let table = |tag: &[u8; 4]| tables.get(tag).map(|range| &data[range.clone()]);
    let (Some(head), Some(loca), Some(glyf), Some(maxp)) = (
        table(b"head"),
        table(b"loca"),
        table(b"glyf"),
        table(b"maxp"),
    ) else {
        return Err(FontError::CannotSubset);
    };
    if head.len() < 54 {
        return Err(damaged());
    }
    let glyph_count = usize::from(read_u16(maxp, 4).ok_or_else(damaged)?);
    let long_offsets = head.get(50..52).is_some_and(|format| format == [0, 1]);
    let offsets = read_loca(loca, glyph_count, long_offsets).ok_or_else(damaged)?;
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
    let mut output: Tables = BTreeMap::new();
    output.insert(*b"head", new_head);
    output.insert(*b"loca", new_loca);
    output.insert(*b"glyf", new_glyf);
    // the tables a pdf reader needs to draw TrueType glyphs, and cmap so
    // a docx or the web view can still map characters
    for tag in [
        b"hhea", b"hmtx", b"maxp", b"cvt ", b"fpgm", b"prep", b"OS/2", b"name", b"post", b"cmap",
    ] {
        if let Some(bytes) = table(tag) {
            output.insert(*tag, bytes.to_vec());
        }
    }
    Ok(write_font(0x0001_0000, &output))
}

/// the glyph offsets from a loca table (`glyph_count + 1` of them).
pub(crate) fn read_loca(loca: &[u8], glyph_count: usize, long: bool) -> Option<Vec<usize>> {
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
    fn broken_fonts_are_refused() {
        assert!(embedding_data(b"junk", 0, "abc").is_err());
        assert!(subset_truetype(&[0, 1, 0, 0, 0, 0], 0, &BTreeSet::new()).is_err());
    }
}
