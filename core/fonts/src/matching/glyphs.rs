//! the glyphs of a font as this module works with them: per character,
//! its outline in em units and its advance, plus the font's vertical
//! metrics. read from any font ttf-parser accepts (TrueType or CFF,
//! variable fonts at chosen axis values).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ttf_parser::{Face, GlyphId, Tag};

use super::shape::{Shape, ShapeBuilder};

/// one glyph: its outline (em units, origin at 0,0) and advance (em).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlyphData {
    pub shape: Shape,
    pub advance: f64,
}

/// the font-wide numbers, in em units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontMetrics {
    pub units_per_em: u16,
    pub ascender: f64,
    pub descender: f64,
    pub line_gap: f64,
    /// what the font says (OS/2), when it says
    pub x_height: Option<f64>,
    pub cap_height: Option<f64>,
    /// degrees, negative leaning right, as the post table stores it
    pub italic_angle: f64,
    pub weight_class: u16,
    pub italic: bool,
}

impl Default for FontMetrics {
    fn default() -> Self {
        FontMetrics {
            units_per_em: 1000,
            ascender: 0.8,
            descender: -0.2,
            line_gap: 0.0,
            x_height: None,
            cap_height: None,
            italic_angle: 0.0,
            weight_class: 400,
            italic: false,
        }
    }
}

/// the glyphs of some characters of one font.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GlyphSet {
    pub glyphs: BTreeMap<char, GlyphData>,
    pub metrics: FontMetrics,
}

impl GlyphSet {
    pub fn get(&self, c: char) -> Option<&GlyphData> {
        self.glyphs.get(&c)
    }

    pub fn has(&self, c: char) -> bool {
        self.glyphs.contains_key(&c)
    }
}

/// the most glyphs read from one font at once; far more than a page of
/// text needs, and it keeps a hostile font from costing much.
pub const MAX_GLYPHS_READ: usize = 4096;

/// the font-wide metrics of a face.
pub fn metrics_of(face: &Face) -> FontMetrics {
    let units = f64::from(face.units_per_em().max(16));
    let em = |value: i16| f64::from(value) / units;
    FontMetrics {
        units_per_em: face.units_per_em().max(16),
        ascender: em(face.ascender()),
        descender: em(face.descender()),
        line_gap: em(face.line_gap()),
        x_height: face.x_height().filter(|h| *h > 0).map(em),
        cap_height: face.capital_height().filter(|h| *h > 0).map(em),
        italic_angle: f64::from(face.italic_angle()),
        weight_class: face.weight().to_number(),
        italic: face.is_italic() || face.is_oblique(),
    }
}

/// the outline of one glyph in em units.
pub fn glyph_shape(face: &Face, glyph: GlyphId) -> Shape {
    let units = f64::from(face.units_per_em().max(16));
    let mut builder = ShapeBuilder::new(1.0 / units);
    face.outline_glyph(glyph, &mut builder);
    builder.finish().cleaned().with_truetype_direction()
}

/// the glyph of one character, or None when the font has none for it.
/// spaces and other blank characters keep their (empty) glyph.
pub fn glyph_of(face: &Face, c: char) -> Option<GlyphData> {
    let glyph = face.glyph_index(c).filter(|glyph| glyph.0 != 0)?;
    glyph_by_id(face, glyph, c.is_whitespace())
}

/// a glyph by number; None when it has no outline and isn't meant to be
/// blank (subset fonts empty the glyphs they leave out).
pub fn glyph_by_id(face: &Face, glyph: GlyphId, blank_allowed: bool) -> Option<GlyphData> {
    let units = f64::from(face.units_per_em().max(16));
    let shape = glyph_shape(face, glyph);
    if shape.is_empty() && !blank_allowed {
        return None;
    }
    let advance = f64::from(face.glyph_hor_advance(glyph).unwrap_or(0)) / units;
    Some(GlyphData { shape, advance })
}

/// reads `chars` from a font at the given variation axis values
/// ("wght" 700...). characters the font lacks are left out.
pub fn read_glyphs(
    data: &[u8],
    index: u32,
    axes: &[(String, f32)],
    chars: impl IntoIterator<Item = char>,
) -> Option<GlyphSet> {
    let mut face = Face::parse(data, index).ok()?;
    for (tag, value) in axes {
        if let Ok(bytes) = <[u8; 4]>::try_from(tag.as_bytes()) {
            face.set_variation(Tag::from_bytes(&bytes), *value);
        }
    }
    let mut set = GlyphSet {
        metrics: metrics_of(&face),
        ..GlyphSet::default()
    };
    for c in chars.into_iter().take(MAX_GLYPHS_READ) {
        if let Some(glyph) = glyph_of(&face, c) {
            set.glyphs.insert(c, glyph);
        }
    }
    Some(set)
}

/// every character the font maps, for asking what a donor can give.
pub fn mapped_chars(face: &Face) -> Vec<char> {
    let mut chars = Vec::new();
    if let Some(cmap) = face.tables().cmap {
        for subtable in cmap.subtables.into_iter().filter(|t| t.is_unicode()) {
            subtable.codepoints(|point| {
                if chars.len() < 70_000 {
                    if let Some(c) = char::from_u32(point) {
                        chars.push(c);
                    }
                }
            });
        }
    }
    chars.sort_unstable();
    chars.dedup();
    chars
}

/// the kerning between two glyphs, from the old kern table or from
/// GPOS pair adjustments, in font units (negative pulls them together).
pub fn pair_kerning(face: &Face, left: GlyphId, right: GlyphId) -> i32 {
    if let Some(kern) = face.tables().kern {
        let found = kern
            .subtables
            .into_iter()
            .filter(|table| table.horizontal && !table.variable)
            .find_map(|table| table.glyphs_kerning(left, right));
        if let Some(value) = found {
            return i32::from(value);
        }
    }
    gpos_kerning(face, left, right).unwrap_or(0)
}

fn gpos_kerning(face: &Face, left: GlyphId, right: GlyphId) -> Option<i32> {
    use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};
    let gpos = face.tables().gpos?;
    // only the lookups of the "kern" feature: other pair lookups (such
    // as for small caps or vertical text) don't apply to plain text
    let mut lookups: Vec<u16> = Vec::new();
    for feature in gpos.features {
        if feature.tag == Tag::from_bytes(b"kern") {
            lookups.extend(feature.lookup_indices);
        }
    }
    lookups.sort_unstable();
    lookups.dedup();
    for lookup_index in lookups.into_iter().take(64) {
        let lookup = gpos.lookups.get(lookup_index)?;
        for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
            let PositioningSubtable::Pair(pair) = subtable else {
                continue;
            };
            let value = match pair {
                PairAdjustment::Format1 { coverage, sets } => coverage
                    .get(left)
                    .and_then(|at| sets.get(at))
                    .and_then(|set| set.get(right))
                    .map(|(first, _)| first.x_advance),
                PairAdjustment::Format2 {
                    coverage,
                    classes,
                    matrix,
                } => {
                    if coverage.contains(left) {
                        matrix
                            .get((classes.0.get(left), classes.1.get(right)))
                            .map(|(first, _)| first.x_advance)
                    } else {
                        None
                    }
                }
            };
            if let Some(value) = value.filter(|value| *value != 0) {
                return Some(i32::from(value));
            }
        }
    }
    None
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
    fn glyphs_come_out_in_em_units() {
        let set = read_glyphs(&atkinson(), 0, &[], "Hn ".chars()).unwrap();
        let h = set.get('H').unwrap();
        let bounds = h.shape.bounds().unwrap();
        assert!(bounds.y_max > 0.6 && bounds.y_max < 0.8, "{bounds:?}");
        assert!(h.advance > 0.5 && h.advance < 0.9);
        // the space has no outline but is kept
        assert!(set.get(' ').unwrap().shape.is_empty());
        assert!(set.metrics.ascender > 0.7);
    }

    #[test]
    fn broken_fonts_give_nothing() {
        assert!(read_glyphs(b"not a font", 0, &[], "a".chars()).is_none());
    }
}
