//! one loaded font face: its bytes, the metrics layout reads from it, and
//! a fast character to glyph table.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// where a face's bytes came from, for the report and for embedding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FontOrigin {
    /// installed on this computer
    System { path: PathBuf },
    /// carried inside the document
    Embedded,
    /// handed to the engine by the caller (a font library, a test)
    Provided,
}

/// the raw numbers of a face that line height and decoration depend on,
/// in font units. which of the three ascent/descent pairs a word processor
/// uses is a flavor rule (see `metrics.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceMetrics {
    pub units_per_em: f64,
    /// os/2 usWinAscent and usWinDescent (descent positive, below the baseline)
    pub win_ascent: f64,
    pub win_descent: f64,
    /// os/2 sTypoAscender, sTypoDescender (negative), sTypoLineGap
    pub typo_ascent: f64,
    pub typo_descent: f64,
    pub typo_line_gap: f64,
    /// hhea ascender, descender (negative), lineGap
    pub hhea_ascent: f64,
    pub hhea_descent: f64,
    pub hhea_line_gap: f64,
    /// os/2 fsSelection bit 7: the typo values are the ones to use
    pub use_typo_metrics: bool,
    pub underline_position: f64,
    pub underline_thickness: f64,
    pub strikeout_position: f64,
    pub strikeout_thickness: f64,
    pub cap_height: f64,
    pub x_height: f64,
    pub italic_angle: f64,
    /// the os/2 fsType embedding bits
    pub embedding_bits: u16,
}

/// characters below this are looked up once when the face loads; the
/// rest are looked up on demand
const TABLE_SIZE: usize = 0x2500;

/// a face ready for layout.
#[derive(Debug)]
pub struct LoadedFace {
    /// the face's own family name (what it really is, not what was asked)
    pub family: String,
    pub postscript_name: String,
    pub bold: bool,
    pub italic: bool,
    pub origin: FontOrigin,
    pub bytes: Arc<Vec<u8>>,
    pub index: u32,
    pub metrics: FaceMetrics,
    pub has_glyf_outlines: bool,
    pub glyph_count: u16,
    /// the variation instance laid out (a variable font at another weight)
    pub variations: Vec<([u8; 4], f32)>,
    /// the instance is bolder than the outlines the file draws by default:
    /// the pdf strokes its glyphs, as for a made-up bold (the advances are
    /// the instance's own, so lines break as with the real bold)
    pub stroke_for_weight: bool,
    /// glyph id and advance (font units) for each character below TABLE_SIZE;
    /// glyph 0 means the face cannot draw it
    table: Vec<(u16, u16)>,
}

impl LoadedFace {
    /// parses a face; `None` when the bytes are not a usable font.
    pub fn load(bytes: Arc<Vec<u8>>, index: u32, origin: FontOrigin) -> Option<LoadedFace> {
        LoadedFace::load_with(bytes, index, origin, Vec::new())
    }

    /// a variable font at one weight: advances (hvar) of that instance.
    pub fn load_instance(
        bytes: Arc<Vec<u8>>,
        index: u32,
        origin: FontOrigin,
        weight: f32,
    ) -> Option<LoadedFace> {
        let mut face = LoadedFace::load_with(bytes, index, origin, vec![(*b"wght", weight)])?;
        face.bold = weight >= 600.0;
        face.stroke_for_weight = weight >= 600.0;
        Some(face)
    }

    fn load_with(
        bytes: Arc<Vec<u8>>,
        index: u32,
        origin: FontOrigin,
        variations: Vec<([u8; 4], f32)>,
    ) -> Option<LoadedFace> {
        let mut face = ttf_parser::Face::parse(&bytes, index).ok()?;
        for (tag, value) in &variations {
            face.set_variation(ttf_parser::Tag::from_bytes(tag), *value);
        }
        let metrics = read_metrics(&face);
        let mut table = vec![(0u16, 0u16); TABLE_SIZE];
        for (code, slot) in table.iter_mut().enumerate() {
            if let Some(ch) = char::from_u32(code as u32) {
                if let Some(glyph) = face.glyph_index(ch) {
                    *slot = (glyph.0, face.glyph_hor_advance(glyph).unwrap_or(0));
                }
            }
        }
        let family = face_name(&face, &[1, 16]).unwrap_or_default();
        let postscript_name = face_name(&face, &[6]).unwrap_or_else(|| family.replace(' ', ""));
        let has_glyf_outlines = face.tables().glyf.is_some();
        let glyph_count = face.number_of_glyphs();
        let (bold, italic) = (face.is_bold(), face.is_italic());
        Some(LoadedFace {
            family,
            postscript_name,
            bold,
            italic,
            origin,
            bytes,
            index,
            metrics,
            has_glyf_outlines,
            glyph_count,
            variations,
            stroke_for_weight: false,
            table,
        })
    }

    /// a ttf-parser view of the face (at its instance), for the rarer
    /// questions.
    pub fn parse(&self) -> Option<ttf_parser::Face<'_>> {
        let mut face = ttf_parser::Face::parse(&self.bytes, self.index).ok()?;
        for (tag, value) in &self.variations {
            face.set_variation(ttf_parser::Tag::from_bytes(tag), *value);
        }
        Some(face)
    }

    /// glyph id and advance in font units, or `None` when the face has no
    /// glyph for the character.
    pub fn glyph(&self, ch: char) -> Option<(u16, u16)> {
        let code = ch as usize;
        if code < TABLE_SIZE {
            let entry = self.table[code];
            return (entry.0 != 0).then_some(entry);
        }
        let face = self.parse()?;
        let glyph = face.glyph_index(ch)?;
        Some((glyph.0, face.glyph_hor_advance(glyph).unwrap_or(0)))
    }

    pub fn has_char(&self, ch: char) -> bool {
        self.glyph(ch).is_some()
    }

    /// the advance of a glyph id in font units.
    pub fn advance_of_glyph(&self, glyph: u16) -> u16 {
        self.parse()
            .and_then(|face| face.glyph_hor_advance(ttf_parser::GlyphId(glyph)))
            .unwrap_or(0)
    }

    /// points per font unit at a size in points.
    pub fn scale(&self, size: f64) -> f64 {
        size / self.metrics.units_per_em.max(1.0)
    }
}

fn read_metrics(face: &ttf_parser::Face) -> FaceMetrics {
    let tables = face.tables();
    let hhea = tables.hhea;
    let mut metrics = FaceMetrics {
        units_per_em: f64::from(face.units_per_em()),
        hhea_ascent: f64::from(hhea.ascender),
        hhea_descent: f64::from(hhea.descender),
        hhea_line_gap: f64::from(hhea.line_gap),
        italic_angle: f64::from(face.italic_angle()),
        ..FaceMetrics::default()
    };
    match tables.os2 {
        Some(os2) => {
            metrics.win_ascent = f64::from(os2.windows_ascender());
            // ttf-parser returns usWinDescent negated; layout wants it positive
            metrics.win_descent = f64::from(os2.windows_descender()).abs();
            metrics.typo_ascent = f64::from(os2.typographic_ascender());
            metrics.typo_descent = f64::from(os2.typographic_descender());
            metrics.typo_line_gap = f64::from(os2.typographic_line_gap());
            metrics.use_typo_metrics = os2.use_typographic_metrics();
            metrics.embedding_bits = os2_embedding_bits(face);
        }
        None => {
            metrics.win_ascent = metrics.hhea_ascent;
            metrics.win_descent = -metrics.hhea_descent;
            metrics.typo_ascent = metrics.hhea_ascent;
            metrics.typo_descent = metrics.hhea_descent;
            metrics.typo_line_gap = metrics.hhea_line_gap;
        }
    }
    let upem = metrics.units_per_em;
    let underline = face.underline_metrics();
    metrics.underline_position = underline.map_or(-0.1 * upem, |m| f64::from(m.position));
    metrics.underline_thickness = underline.map_or(0.05 * upem, |m| f64::from(m.thickness));
    let strike = face.strikeout_metrics();
    metrics.strikeout_position = strike.map_or(0.25 * upem, |m| f64::from(m.position));
    metrics.strikeout_thickness = strike.map_or(0.05 * upem, |m| f64::from(m.thickness));
    metrics.cap_height = face.capital_height().map_or(0.7 * upem, f64::from);
    metrics.x_height = face.x_height().map_or(0.5 * upem, f64::from);
    metrics
}

/// fsType sits at byte 8 of the os/2 table; ttf-parser exposes it only
/// as a permissions enum, the raw bits are what embedding rules need.
fn os2_embedding_bits(face: &ttf_parser::Face) -> u16 {
    face.raw_face()
        .table(ttf_parser::Tag::from_bytes(b"OS/2"))
        .and_then(|table| super::system::u16_at(table, 8))
        .unwrap_or(0)
}

fn face_name(face: &ttf_parser::Face, ids: &[u16]) -> Option<String> {
    for id in ids {
        for name in face.names() {
            if name.name_id == *id && name.is_unicode() {
                if let Some(text) = name.to_string() {
                    return Some(text);
                }
            }
        }
    }
    None
}
