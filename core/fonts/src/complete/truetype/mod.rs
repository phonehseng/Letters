//! writing a completed font as a TrueType file.
//!
//! when the document's font came as a TrueType program, it is the base:
//! every one of its glyphs is copied byte for byte and keeps its number,
//! so text already in the document draws exactly as before (a composite
//! pdf font can keep using glyph numbers as cids). the new glyphs are
//! appended after the last one. the tables that list every glyph (loca,
//! hmtx, maxp, cmap, kern) are rebuilt to include them; hinting programs
//! are kept for the old glyphs, and the new ones are unhinted, with
//! clean outlines (extremes as points, TrueType direction).
//!
//! tables whose content depends on the glyph count and that a pdf or a
//! word processor doesn't need are left out (hdmx, LTSH, VDMX, vertical
//! metrics, layout tables, variations, signatures).
//!
//! when there is no TrueType base (a CFF program, or outlines handed
//! over), the font is written from the outlines alone.
//!
//! the name table always gets the neutral family name given: a
//! completed font is a derived font and doesn't carry the name of the
//! font it came from.

use std::collections::BTreeMap;

use crate::error::FontError;
use crate::matching::glyphs::{FontMetrics, GlyphData};
use crate::matching::shape::Shape;
use crate::sfnt::{
    checksum, read_i16, read_tables, read_u16, read_u32, table_directory, write_font, Tables,
};
use crate::subset::read_loca;

mod cmap;
mod glyph;
mod kern;
mod tables;
#[cfg(test)]
mod tests;

// the parts share this module's imports through `use super::*`; only
// `name_table` among them is public
use cmap::*;
use glyph::*;
use kern::*;
pub use tables::*;

/// everything needed to write a completed font.
pub struct FontToWrite<'a> {
    /// a plain single TrueType font whose glyphs are kept as they are
    pub base: Option<&'a [u8]>,
    pub metrics: &'a FontMetrics,
    /// characters the base already draws, and their glyph numbers
    pub original_ids: &'a BTreeMap<char, u16>,
    /// the original glyphs as outlines, used when there is no base
    pub original_glyphs: &'a BTreeMap<char, GlyphData>,
    /// the glyphs to add, in em units
    pub new_glyphs: &'a [(char, GlyphData)],
    /// kerning for pairs that involve a new glyph, em
    pub kerning: &'a [(char, char, f64)],
    /// the neutral family name for the name table
    pub family: &'a str,
    /// the licence bits the written font carries (the document font's)
    pub fs_type: u16,
}

/// a written font and where each character's glyph is.
#[derive(Clone, Debug, PartialEq)]
pub struct WrittenFont {
    pub bytes: Vec<u8>,
    pub char_to_glyph: BTreeMap<char, u16>,
    pub glyph_count: u16,
}

/// writes the font.
pub fn write_completed_font(input: &FontToWrite) -> Result<WrittenFont, FontError> {
    match input.base {
        Some(base) => write_on_base(base, input),
        None => write_from_outlines(input),
    }
}

fn damaged() -> FontError {
    FontError::Damaged("the document’s font".into())
}

/// the most glyphs a TrueType font can number (glyph ids are 16 bits).
const MAX_GLYPHS: usize = 65_535;

/// the document's TrueType program, read for appending.
struct Base {
    tables: Tables,
    units: f64,
    glyph_count: usize,
    /// where each glyph's data sits in glyf
    offsets: Vec<usize>,
    /// how many glyphs have a full hmtx entry
    metrics_count: usize,
}

impl Base {
    fn read(bytes: &[u8]) -> Result<Base, FontError> {
        let tables = read_tables(bytes, 0).ok_or_else(damaged)?;
        let required = [b"head", b"maxp", b"hhea", b"hmtx", b"loca", b"glyf"];
        if required.iter().any(|tag| !tables.contains_key(*tag)) {
            return Err(FontError::CannotSubset);
        }
        let (head, maxp, hhea) = (&tables[b"head"], &tables[b"maxp"], &tables[b"hhea"]);
        // the fixed sizes of head, hhea and the start of maxp
        if head.len() < 54 || hhea.len() < 36 || maxp.len() < 6 {
            return Err(damaged());
        }
        let units = read_u16(head, 18)
            .filter(|u| *u >= 16)
            .ok_or_else(damaged)?;
        let glyph_count = usize::from(read_u16(maxp, 4).ok_or_else(damaged)?);
        let long_offsets = read_i16(head, 50) == Some(1);
        let offsets = read_loca(&tables[b"loca"], glyph_count, long_offsets).ok_or_else(damaged)?;
        let metrics_count =
            usize::from(read_u16(hhea, 34).unwrap_or(0)).clamp(1, glyph_count.max(1));
        Ok(Base {
            units: f64::from(units),
            glyph_count,
            offsets,
            metrics_count,
            tables,
        })
    }

    fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.tables.get(tag).map(Vec::as_slice)
    }

    /// every old glyph, byte for byte, with its advance and side bearing.
    fn old_glyphs(&self) -> (Vec<Vec<u8>>, Vec<u16>, Vec<i16>) {
        let glyf = &self.tables[b"glyf"];
        let hmtx = &self.tables[b"hmtx"];
        let mut glyphs = Vec::with_capacity(self.glyph_count);
        let mut advances = Vec::with_capacity(self.glyph_count);
        let mut lsbs = Vec::with_capacity(self.glyph_count);
        for glyph in 0..self.glyph_count {
            let range = self.offsets[glyph]..self.offsets[glyph + 1];
            glyphs.push(glyf.get(range).unwrap_or(&[]).to_vec());
            let (advance, lsb) = old_metrics(hmtx, self.metrics_count, glyph);
            advances.push(advance);
            lsbs.push(lsb);
        }
        (glyphs, advances, lsbs)
    }
}

/// appends the new glyphs to the document's own TrueType program.
fn write_on_base(bytes: &[u8], input: &FontToWrite) -> Result<WrittenFont, FontError> {
    let base = Base::read(bytes)?;
    if base.glyph_count + input.new_glyphs.len() > MAX_GLYPHS {
        return Err(FontError::CannotSubset);
    }
    let (mut glyph_bytes, mut advances, mut lsbs) = base.old_glyphs();
    let mut char_to_glyph = input.original_ids.clone();
    let mut new_encoded = Vec::new();
    for (c, glyph) in input.new_glyphs {
        let encoded = encode_glyph(&glyph.shape, glyph.advance, base.units);
        char_to_glyph.insert(*c, glyph_bytes.len() as u16);
        glyph_bytes.push(encoded.bytes.clone());
        advances.push(encoded.advance);
        lsbs.push(encoded.lsb);
        new_encoded.push(encoded);
    }
    let total = glyph_bytes.len();
    let mut output: Tables = BTreeMap::new();
    let (new_glyf, new_loca) = glyf_and_loca(&glyph_bytes);
    output.insert(*b"glyf", new_glyf);
    output.insert(*b"loca", new_loca);
    output.insert(*b"hmtx", hmtx_table(&advances, &lsbs));
    output.insert(*b"head", updated_head(&base.tables[b"head"], &new_encoded));
    output.insert(
        *b"hhea",
        updated_hhea(&base.tables[b"hhea"], total, &new_encoded),
    );
    output.insert(
        *b"maxp",
        updated_maxp(&base.tables[b"maxp"], total, &new_encoded),
    );
    add_font_wide_tables(&mut output, &base, input, &char_to_glyph, &advances);
    Ok(WrittenFont {
        bytes: finish(&output),
        char_to_glyph,
        glyph_count: total as u16,
    })
}

/// cmap, name, post, OS/2 and kern for a font written on a base, and the
/// base's hinting tables (its old glyphs' instructions call into them).
fn add_font_wide_tables(
    output: &mut Tables,
    base: &Base,
    input: &FontToWrite,
    char_to_glyph: &BTreeMap<char, u16>,
    advances: &[u16],
) {
    let old_cmap = base.table(b"cmap").unwrap_or(&[]);
    output.insert(*b"cmap", cmap_table(char_to_glyph, old_cmap));
    output.insert(*b"name", name_table(input.family, "Regular"));
    output.insert(
        *b"post",
        post_table(base.table(b"post"), input.metrics, base.units),
    );
    // an OS/2 table shorter than version 0's 68 bytes can't be updated
    let os2 = match base.table(b"OS/2").filter(|os2| os2.len() >= 68) {
        Some(os2) => updated_os2(os2, char_to_glyph),
        None => os2_table(input, base.units, char_to_glyph, advances),
    };
    output.insert(*b"OS/2", os2);
    let old_pairs = base.table(b"kern").map(kern_pairs).unwrap_or_default();
    let pairs = merged_kerning(old_pairs, input.kerning, char_to_glyph, base.units);
    if !pairs.is_empty() {
        output.insert(*b"kern", kern_table(&pairs));
    }
    for tag in [b"cvt ", b"fpgm", b"prep", b"gasp"] {
        if let Some(bytes) = base.table(tag) {
            output.insert(*tag, bytes.to_vec());
        }
    }
}

/// a glyph's advance and left side bearing from hmtx: glyphs past the
/// last full entry share its advance and have only a side bearing.
fn old_metrics(hmtx: &[u8], metrics_count: usize, glyph: usize) -> (u16, i16) {
    if glyph < metrics_count {
        (
            read_u16(hmtx, glyph * 4).unwrap_or(0),
            read_i16(hmtx, glyph * 4 + 2).unwrap_or(0),
        )
    } else {
        (
            read_u16(hmtx, (metrics_count - 1) * 4).unwrap_or(0),
            read_i16(hmtx, metrics_count * 4 + (glyph - metrics_count) * 2).unwrap_or(0),
        )
    }
}

/// writes a font made only from outlines: glyph 0 empty, then the
/// original characters, then the new ones.
fn write_from_outlines(input: &FontToWrite) -> Result<WrittenFont, FontError> {
    let units = f64::from(input.metrics.units_per_em.max(16));
    let mut encoded = vec![EncodedGlyph::empty(to_units(0.5, units) as u16)];
    let mut char_to_glyph = BTreeMap::new();
    let originals = input.original_glyphs.iter().map(|(c, g)| (*c, g));
    let added = input.new_glyphs.iter().map(|(c, g)| (*c, g));
    for (c, glyph) in originals.chain(added) {
        if char_to_glyph.contains_key(&c) {
            continue;
        }
        char_to_glyph.insert(c, encoded.len() as u16);
        encoded.push(encode_glyph(&glyph.shape, glyph.advance, units));
    }
    if encoded.len() > 65_535 {
        return Err(FontError::CannotSubset);
    }
    let glyph_bytes: Vec<Vec<u8>> = encoded.iter().map(|g| g.bytes.clone()).collect();
    let advances: Vec<u16> = encoded.iter().map(|g| g.advance).collect();
    let lsbs: Vec<i16> = encoded.iter().map(|g| g.lsb).collect();
    let mut output: Tables = BTreeMap::new();
    let (glyf, loca) = glyf_and_loca(&glyph_bytes);
    output.insert(*b"glyf", glyf);
    output.insert(*b"loca", loca);
    output.insert(*b"hmtx", hmtx_table(&advances, &lsbs));
    output.insert(*b"head", new_head(units as u16, &encoded, input.metrics));
    output.insert(*b"hhea", new_hhea(input.metrics, units, &encoded));
    output.insert(*b"maxp", new_maxp(&encoded));
    output.insert(*b"cmap", cmap_table(&char_to_glyph, &[]));
    output.insert(*b"name", name_table(input.family, "Regular"));
    output.insert(*b"post", post_table(None, input.metrics, units));
    output.insert(*b"OS/2", os2_table(input, units, &char_to_glyph, &advances));
    let pairs = merged_kerning(Vec::new(), input.kerning, &char_to_glyph, units);
    if !pairs.is_empty() {
        output.insert(*b"kern", kern_table(&pairs));
    }
    Ok(WrittenFont {
        bytes: finish(&output),
        char_to_glyph,
        glyph_count: encoded.len() as u16,
    })
}
