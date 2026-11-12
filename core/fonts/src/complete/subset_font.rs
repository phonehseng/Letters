//! the document's font as the completion pipeline sees it: the glyphs it
//! really has (subset fonts empty the ones they leave out), where each
//! sits in the font program, the widths the pdf gives, and the name the
//! pdf calls it by.

use std::collections::BTreeMap;

use sha1::{Digest, Sha1};
use ttf_parser::{Face, GlyphId};

use crate::error::FontError;
use crate::matching::donors::{family_from_font_name, TargetProfile};
use crate::matching::glyphs::{glyph_by_id, mapped_chars, metrics_of, GlyphData, GlyphSet};
use crate::matching::style::{horizontal_spans, x_height_of};
use crate::sfnt::{read_u16, single_font};

/// the document's font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubsetFont {
    /// the characters it can draw, in em units
    pub glyphs: GlyphSet,
    /// each character's glyph number in `truetype` (when it came as bytes)
    pub glyph_ids: BTreeMap<char, u16>,
    /// widths the pdf gives for characters, em; a pdf often lists widths
    /// for characters its subset doesn't draw, which are their true advances
    pub pdf_widths: BTreeMap<char, f64>,
    /// the plain TrueType program, when it has TrueType outlines: the
    /// completed font is built on it so its glyphs stay byte for byte
    pub truetype: Option<Vec<u8>>,
    /// the family the pdf names ("Arial" from "ABCDEF+Arial-BoldMT")
    pub name_hint: Option<String>,
    /// the font's licence bits, when it has an OS/2 table
    pub fs_type: Option<u16>,
}

impl SubsetFont {
    /// reads a font program from a pdf. `char_map` says which glyph draws
    /// each character, as the pdf's encoding or ToUnicode map tells; when
    /// it is None the font's own unicode cmap is used. glyphs without an
    /// outline (emptied by subsetting) don't count, except for spaces.
    pub fn from_font_bytes(
        bytes: &[u8],
        index: u32,
        char_map: Option<&BTreeMap<char, u16>>,
    ) -> Result<SubsetFont, FontError> {
        let damaged = || FontError::Damaged("the document’s font".into());
        let face = Face::parse(bytes, index).map_err(|_| damaged())?;
        let map: BTreeMap<char, u16> = match char_map {
            Some(map) => map.clone(),
            None => mapped_chars(&face)
                .into_iter()
                .filter_map(|c| Some((c, face.glyph_index(c)?.0)))
                .collect(),
        };
        let mut font = SubsetFont {
            glyphs: GlyphSet {
                metrics: metrics_of(&face),
                ..GlyphSet::default()
            },
            ..SubsetFont::default()
        };
        for (c, glyph) in map.iter().take(crate::matching::glyphs::MAX_GLYPHS_READ) {
            if *glyph == 0 || *glyph >= face.number_of_glyphs() {
                continue;
            }
            if let Some(data) = glyph_by_id(&face, GlyphId(*glyph), c.is_whitespace()) {
                font.glyphs.glyphs.insert(*c, data);
                font.glyph_ids.insert(*c, *glyph);
            }
        }
        if face.tables().glyf.is_some() {
            font.truetype = single_font(bytes, index);
        }
        font.fs_type = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"OS/2"))
            .and_then(|os2| read_u16(os2, 8));
        Ok(font)
    }

    /// a font known only by its outlines (for example rebuilt by another
    /// part of Letters), in em units.
    pub fn from_glyphs(glyphs: GlyphSet) -> SubsetFont {
        SubsetFont {
            glyphs,
            ..SubsetFont::default()
        }
    }

    /// adds the pdf's widths, in thousandths of the em as /Widths gives them.
    pub fn with_pdf_widths(mut self, widths: &BTreeMap<char, f64>) -> SubsetFont {
        self.pdf_widths = widths
            .iter()
            .filter(|(_, width)| width.is_finite() && **width > 0.0 && **width < 5000.0)
            .map(|(c, width)| (*c, width / 1000.0))
            .collect();
        self
    }

    /// adds the pdf's name for the font ("ABCDEF+Arial-BoldMT").
    pub fn with_name(mut self, base_font: &str) -> SubsetFont {
        let family = family_from_font_name(base_font);
        self.name_hint = (!family.is_empty()).then_some(family);
        self
    }

    pub fn has(&self, c: char) -> bool {
        self.glyphs.has(c)
    }

    pub fn glyph(&self, c: char) -> Option<&GlyphData> {
        self.glyphs.get(c)
    }

    /// a short fingerprint of the outlines and advances, so a cache can
    /// tell this font apart from any other (and from itself edited).
    pub fn outline_hash(&self) -> String {
        let mut hasher = Sha1::new();
        let round = |value: f64| ((value * 100_000.0).round() as i64).to_be_bytes();
        hasher.update(self.glyphs.metrics.units_per_em.to_be_bytes());
        for (c, glyph) in &self.glyphs.glyphs {
            hasher.update(u32::from(*c).to_be_bytes());
            hasher.update(round(glyph.advance));
            for contour in &glyph.shape.contours {
                hasher.update(b"c");
                for node in &contour.nodes {
                    hasher.update(round(node.x));
                    hasher.update(round(node.y));
                    hasher.update([u8::from(node.on)]);
                }
            }
        }
        for (c, width) in &self.pdf_widths {
            hasher.update(u32::from(*c).to_be_bytes());
            hasher.update(round(*width));
        }
        let digest = hasher.finalize();
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// what is quickly known about the font, to shortlist donors.
    pub fn profile(&self) -> TargetProfile {
        let ascii_widths = self
            .glyphs
            .glyphs
            .iter()
            .filter(|(c, _)| (' '..='~').contains(*c))
            .map(|(c, glyph)| (*c, glyph.advance * 1000.0))
            .chain(
                self.pdf_widths
                    .iter()
                    .filter(|(c, _)| (' '..='~').contains(*c) && !self.has(**c))
                    .map(|(c, width)| (*c, width * 1000.0)),
            )
            .collect();
        let stem_of_o = self.glyph('o').and_then(|o| {
            let bounds = o.shape.bounds()?;
            let spans = horizontal_spans(&o.shape, (bounds.y_min + bounds.y_max) / 2.0);
            spans.first().map(|span| span.1 - span.0)
        });
        let style = crate::matching::style::measure_style(&self.glyphs);
        TargetProfile {
            ascii_widths,
            x_height: x_height_of(&self.glyphs),
            stem_of_o,
            italic: self.glyphs.metrics.italic || style.slant_degrees.abs() > 4.0,
            serif: (style.glyphs_measured >= 3).then_some(style.serif),
            name_hint: self.name_hint.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atkinson() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/AtkinsonHyperlegible-Regular.ttf"
        ))
        .unwrap()
    }

    #[test]
    fn emptied_glyphs_do_not_count() {
        let bytes = atkinson();
        let face = Face::parse(&bytes, 0).unwrap();
        let keep: std::collections::BTreeSet<u16> = "Hi "
            .chars()
            .map(|c| face.glyph_index(c).unwrap().0)
            .collect();
        let subset = crate::subset::subset_truetype(&bytes, 0, &keep).unwrap();
        let font = SubsetFont::from_font_bytes(&subset, 0, None).unwrap();
        assert!(font.has('H') && font.has('i') && font.has(' '));
        assert!(!font.has('x'), "an emptied glyph isn't drawable");
        assert!(font.truetype.is_some());
        assert_eq!(font.fs_type, Some(0));
    }

    #[test]
    fn the_hash_changes_with_the_outlines() {
        let bytes = atkinson();
        let a = SubsetFont::from_font_bytes(&bytes, 0, None).unwrap();
        let mut b = a.clone();
        assert_eq!(a.outline_hash(), b.outline_hash());
        b.glyphs.glyphs.get_mut(&'a').unwrap().advance += 0.001;
        assert_ne!(a.outline_hash(), b.outline_hash());
        assert_eq!(a.outline_hash().len(), 16);
    }

    #[test]
    fn names_and_widths_are_cleaned() {
        let widths = BTreeMap::from([('a', 556.0), ('b', -3.0)]);
        let font = SubsetFont::default()
            .with_name("QWERTY+Calibri-Bold")
            .with_pdf_widths(&widths);
        assert_eq!(font.name_hint.as_deref(), Some("Calibri"));
        assert_eq!(font.pdf_widths.get(&'a'), Some(&0.556));
        assert!(!font.pdf_widths.contains_key(&'b'));
    }
}
