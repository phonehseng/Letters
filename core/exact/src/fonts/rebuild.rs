//! embedded pdf font programs to clean truetype faces that word embeds and
//! uses (recipe 3).
//!
//! a pdf subset font is often unusable as it stands: no cmap or a symbolic
//! one, glyph names instead of unicode, cubic cff outlines word will not
//! embed, several subsets of one face. so each face is rebuilt from
//! scratch: only the glyphs the document draws, truetype outlines, a cmap
//! from the pdf's own tounicode or encoding, the pdf's widths as advances.
//! glyphs whose text is missing, ambiguous or a ligature get a private use
//! code point from U+F0000 up; the report counts them because their text in
//! the docx is not the real text.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::outline::{to_contours, Outline, Recorder};
use super::permissions::embedding_permission;
use super::source::GlyphSource;
use super::standins::stand_in_for;
use super::truetype::FontGlyph;
use crate::pdf::fonts::PdfFont;
use crate::units::round_half_even;

/// supplementary private use area a. word drew U+E000..U+F8FF in calibri
/// although the embedded font maps them; from U+F0000 it uses the font
/// (measured, 16.0.20527, round 4).
pub const PRIVATE_USE_START: u32 = 0xF0000;
/// cubic to quadratic tolerance: 1 unit per 1000 em (recipe 3.3).
pub const CUBIC_TOLERANCE_EM: f64 = 0.001;

/// one rebuilt face, in memory.
#[derive(Clone, Debug)]
pub struct RebuiltFace {
    pub face_key: String,
    pub family: String,
    /// the pdf's name for the face
    pub pdf_font: String,
    pub units_per_em: i64,
    /// glyph 0 is `.notdef`, glyph 1 the space
    pub glyphs: Vec<FontGlyph>,
    pub cmap: BTreeMap<u32, u16>,
    /// (pdf font key, code) to the char written in the docx
    pub char_for: HashMap<(String, u32), String>,
    pub ascent: i64,
    pub descent: i64,
    pub fs_type: Option<u16>,
    pub permission: &'static str,
    pub cubic_source: bool,
    /// bound on the cubic to quadratic error, in em (before rounding to units)
    pub outline_error_em: f64,
    pub private_use: usize,
    /// set when this is a stand-in for a font that isn't embedded
    pub substitute_for: Option<String>,
    pub problems: Vec<String>,
}

/// several subsets of one face share the name without the subset prefix
/// and the program kind (recipe 3).
pub fn face_key(font: &PdfFont) -> String {
    let kind = font.program_kind.map(|k| k.label()).unwrap_or("none");
    format!("{}|{}", font.base_name, kind)
}

/// a family name unique to the document, so word can't find an installed font.
pub fn unique_family(face: &str, document_tag: u64) -> String {
    let digest = fnv1a(format!("{document_tag:016x}|{face}").as_bytes());
    format!("Ltr {:08x}", digest as u32)
}

/// a small stable hash (fnv-1a, 64 bits).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

/// the start of a face: `.notdef` and a space, half and a quarter em wide.
fn empty_face(units_per_em: i64) -> (Vec<FontGlyph>, BTreeMap<u32, u16>) {
    let glyphs = vec![
        FontGlyph {
            contours: Vec::new(),
            advance: (units_per_em / 2) as u16,
        },
        FontGlyph {
            contours: Vec::new(),
            advance: (units_per_em / 4) as u16,
        },
    ];
    (glyphs, BTreeMap::from([(0x20, 1)]))
}

fn vertical(font: &PdfFont, units_per_em: i64) -> (i64, i64) {
    let mut ascent = round_half_even(font.ascent * units_per_em as f64 / 1000.0) as i64;
    let mut descent = round_half_even(font.descent * units_per_em as f64 / 1000.0) as i64;
    if ascent == 0 {
        ascent = round_half_even(0.8 * units_per_em as f64) as i64;
    }
    if descent == 0 {
        descent = -round_half_even(0.2 * units_per_em as f64) as i64;
    }
    (ascent, descent)
}

/// the advance of a pdf width in face units.
fn advance_units(width: f64, units_per_em: i64) -> u16 {
    round_half_even(width * units_per_em as f64 / 1000.0).clamp(0.0, 65535.0) as u16
}

/// chooses the char a glyph is written as (recipe 3.4).
struct CharChooser {
    cmap: BTreeMap<u32, u16>,
    /// glyph to the first char mapped to it
    first_char: HashMap<u16, u32>,
    next_private: u32,
}

impl CharChooser {
    fn new(cmap: BTreeMap<u32, u16>) -> Self {
        let first_char = cmap.iter().map(|(&c, &g)| (g, c)).collect();
        Self {
            cmap,
            first_char,
            next_private: PRIVATE_USE_START,
        }
    }

    fn char_for(&mut self, glyph: u16, text: &str) -> String {
        if let Some(&existing) = self.first_char.get(&glyph) {
            return char::from_u32(existing)
                .map(String::from)
                .unwrap_or_default();
        }
        let chars: Vec<char> = text.chars().collect();
        // a control char cannot go into docx xml (a browser mapped a glyph to U+0000)
        let point = match chars.as_slice() {
            [single] if !self.cmap.contains_key(&(*single as u32)) && *single as u32 >= 0x20 => {
                *single as u32
            }
            _ => {
                // no text, a ligature or a second glyph for the same char
                let point = self.next_private;
                self.next_private += 1;
                point
            }
        };
        self.cmap.insert(point, glyph);
        self.first_char.insert(glyph, point);
        char::from_u32(point).map(String::from).unwrap_or_default()
    }

    fn private_use(&self) -> usize {
        (self.next_private - PRIVATE_USE_START) as usize
    }
}

/// one truetype face from every subset of a face, with only the glyphs in use.
pub fn rebuild_face(
    fonts: &[&PdfFont],
    used: &HashMap<String, BTreeSet<u32>>,
    family: &str,
) -> Result<RebuiltFace, String> {
    let mut sources = Vec::new();
    for font in fonts {
        sources.push(GlyphSource::new(font)?);
    }
    let units_per_em = sources[0].units_per_em.round() as i64;
    let cubic_source = sources.iter().any(|s| s.cubic);
    let (mut glyphs, cmap) = empty_face(units_per_em);
    let mut chooser = CharChooser::new(cmap);
    let mut by_outline: HashMap<(Vec<i64>, i64), u16> = HashMap::new();
    let mut char_for = HashMap::new();
    let mut problems = Vec::new();
    for (font, source) in fonts.iter().zip(&sources) {
        let scale = units_per_em as f64 / source.units_per_em;
        let tolerance = CUBIC_TOLERANCE_EM * units_per_em as f64;
        for &code in used.get(&font.key).into_iter().flatten() {
            let text = font.text(code);
            let Some(glyph_ref) = source.glyph_for(code) else {
                problems.push(format!(
                    "{}: code {code} has no glyph in the embedded program",
                    font.base_name
                ));
                continue;
            };
            let outline = source.outline(&glyph_ref, &mut problems);
            if text == " " || (outline.ops.is_empty() && text.trim().is_empty()) {
                char_for.insert((font.key.clone(), code), " ".to_string());
                continue;
            }
            let signature = (
                outline.signature(),
                round_half_even(font.width(code)) as i64,
            );
            let glyph = match by_outline.get(&signature) {
                Some(&glyph) => glyph,
                None => {
                    let glyph = glyphs.len() as u16;
                    glyphs.push(FontGlyph {
                        contours: to_contours(&outline, scale, tolerance, source.cubic),
                        advance: advance_units(font.width(code), units_per_em),
                    });
                    by_outline.insert(signature, glyph);
                    glyph
                }
            };
            char_for.insert((font.key.clone(), code), chooser.char_for(glyph, &text));
        }
    }
    let fs_type = sources[0].fs_type;
    let (ascent, descent) = vertical(fonts[0], units_per_em);
    Ok(RebuiltFace {
        face_key: face_key(fonts[0]),
        family: family.to_string(),
        pdf_font: fonts[0].base_name.clone(),
        units_per_em,
        glyphs,
        private_use: chooser.private_use(),
        cmap: chooser.cmap,
        char_for,
        ascent,
        descent,
        fs_type,
        permission: embedding_permission(fs_type),
        cubic_source,
        outline_error_em: if cubic_source {
            CUBIC_TOLERANCE_EM
        } else {
            0.0
        },
        substitute_for: None,
        problems,
    })
}

/// a font that isn't embedded (or may not be): the stand-in's glyphs with
/// the pdf's widths, so positions stay exact.
pub fn rebuild_substitute(
    fonts: &[&PdfFont],
    used: &HashMap<String, BTreeSet<u32>>,
    family: &str,
) -> RebuiltFace {
    let stand_in = stand_in_for(&fonts[0].base_name, fonts[0].serif);
    let face =
        ttf_parser::Face::parse(stand_in.bytes, 0).expect("the bundled stand-in fonts are valid");
    let units_per_em = i64::from(face.units_per_em());
    let (mut glyphs, cmap) = empty_face(units_per_em);
    let mut cmap = cmap;
    let mut char_for = HashMap::new();
    let mut problems = Vec::new();
    for font in fonts {
        for &code in used.get(&font.key).into_iter().flatten() {
            let text = font.text(code);
            if text == " " {
                char_for.insert((font.key.clone(), code), " ".to_string());
                continue;
            }
            let chars: Vec<char> = text.chars().collect();
            let found = match chars.as_slice() {
                [single] => face.glyph_index(*single).map(|g| (*single, g)),
                _ => None,
            };
            let Some((char, stand_in_glyph)) = found else {
                problems.push(format!(
                    "{}: {text:?} missing in the stand-in",
                    font.base_name
                ));
                continue;
            };
            if cmap.contains_key(&(char as u32)) {
                char_for.insert((font.key.clone(), code), text);
                continue;
            }
            let mut recorder = Recorder::default();
            face.outline_glyph(stand_in_glyph, &mut recorder);
            let outline: Outline = recorder.outline;
            let glyph = glyphs.len() as u16;
            glyphs.push(FontGlyph {
                contours: to_contours(&outline, 1.0, 1.0, false),
                advance: advance_units(font.width(code), units_per_em),
            });
            cmap.insert(char as u32, glyph);
            char_for.insert((font.key.clone(), code), text);
        }
    }
    let fs_type = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"OS/2"))
        .and_then(|os2| os2.get(8..10).map(|b| u16::from_be_bytes([b[0], b[1]])));
    let (ascent, descent) = vertical(fonts[0], units_per_em);
    RebuiltFace {
        face_key: face_key(fonts[0]),
        family: family.to_string(),
        pdf_font: fonts[0].base_name.clone(),
        units_per_em,
        glyphs,
        cmap,
        char_for,
        ascent,
        descent,
        fs_type,
        permission: embedding_permission(fs_type),
        cubic_source: false,
        outline_error_em: 0.0,
        private_use: 0,
        substitute_for: Some(format!(
            "{} -> {} ({})",
            fonts[0].base_name, stand_in.file, stand_in.note
        )),
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_family_is_stable_and_short() {
        let name = unique_family("Lato|truetype", 7);
        assert_eq!(name, unique_family("Lato|truetype", 7));
        assert_ne!(name, unique_family("Lato|truetype", 8));
        assert!(name.len() <= 31);
        assert!(name.starts_with("Ltr "));
    }

    #[test]
    fn chars_fall_back_to_private_use_and_never_to_controls() {
        let mut chooser = CharChooser::new(BTreeMap::from([(0x20, 1)]));
        assert_eq!(chooser.char_for(2, "A"), "A");
        // a second glyph for the same text, a ligature and a nul all go private
        assert_eq!(chooser.char_for(3, "A"), "\u{F0000}");
        assert_eq!(chooser.char_for(4, "fi"), "\u{F0001}");
        assert_eq!(chooser.char_for(5, "\u{0}"), "\u{F0002}");
        assert_eq!(chooser.char_for(6, ""), "\u{F0003}");
        // the same glyph again keeps its char
        assert_eq!(chooser.char_for(4, "fi"), "\u{F0001}");
        assert_eq!(chooser.private_use(), 4);
    }

    #[test]
    fn advances_are_the_pdf_widths() {
        assert_eq!(advance_units(500.0, 2048), 1024);
        assert_eq!(advance_units(250.0, 1000), 250);
    }
}
