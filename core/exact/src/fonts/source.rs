//! drawing the glyphs of one embedded font program by char code (recipe
//! 3.1 and 3.2).

use std::collections::HashMap;

use ttf_parser::{GlyphId, Tag};

use super::outline::{Outline, Recorder};
use super::type1::Type1Font;
use super::type3::type3_outline;
use crate::pdf::fonts::{PdfFont, ProgramKind};

/// a glyph in its program: a glyph index, a glyph name or a type 3 code.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GlyphRef {
    Index(u16),
    Name(String),
    Type3(u32),
}

enum Program {
    /// truetype or opentype: parsed again for each use (cheap) so the bytes can be owned
    Sfnt(Vec<u8>),
    Cff(Vec<u8>),
    Type1(Type1Font),
    Type3,
}

/// one font's program, ready to look glyphs up and draw them.
pub struct GlyphSource<'f> {
    font: &'f PdfFont,
    program: Program,
    pub units_per_em: f64,
    /// cubic outlines (cff, type 1 and type 3) are converted to quadratic
    pub cubic: bool,
    /// the os/2 embedding permission, if the program states one
    pub fs_type: Option<u16>,
    /// cid-keyed cff: cid to glyph index
    cid_glyphs: HashMap<u16, u16>,
}

impl<'f> GlyphSource<'f> {
    pub fn new(font: &'f PdfFont) -> Result<Self, String> {
        let bytes = font.program.clone().unwrap_or_default();
        match font.program_kind {
            Some(ProgramKind::TrueType) | Some(ProgramKind::OpenType) => {
                let face = ttf_parser::Face::parse(&bytes, 0)
                    .map_err(|e| format!("the font program can’t be read ({e})"))?;
                let units_per_em = f64::from(face.units_per_em());
                let cubic = face.tables().cff.is_some();
                let fs_type = face
                    .raw_face()
                    .table(Tag::from_bytes(b"OS/2"))
                    .and_then(|os2| os2.get(8..10).map(|b| u16::from_be_bytes([b[0], b[1]])));
                Ok(Self {
                    font,
                    program: Program::Sfnt(bytes),
                    units_per_em,
                    cubic,
                    fs_type,
                    cid_glyphs: HashMap::new(),
                })
            }
            Some(ProgramKind::Cff) => {
                let table =
                    ttf_parser::cff::Table::parse(&bytes).ok_or("the cff program can’t be read")?;
                let scale = table.matrix().sx;
                let units_per_em = if scale > 0.0 {
                    (1.0 / f64::from(scale)).round()
                } else {
                    1000.0
                };
                let mut cid_glyphs = HashMap::new();
                for index in 0..table.number_of_glyphs() {
                    if let Some(cid) = table.glyph_cid(GlyphId(index)) {
                        cid_glyphs.insert(cid, index);
                    }
                }
                Ok(Self {
                    font,
                    program: Program::Cff(bytes),
                    units_per_em,
                    cubic: true,
                    fs_type: None,
                    cid_glyphs,
                })
            }
            Some(ProgramKind::Type1) => {
                let parsed = Type1Font::parse(&bytes)?;
                let units_per_em = parsed.units_per_em;
                Ok(Self {
                    font,
                    program: Program::Type1(parsed),
                    units_per_em,
                    cubic: true,
                    fs_type: None,
                    cid_glyphs: HashMap::new(),
                })
            }
            Some(ProgramKind::Type3) => Ok(Self {
                font,
                program: Program::Type3,
                units_per_em: 1000.0,
                cubic: true,
                fs_type: None,
                cid_glyphs: HashMap::new(),
            }),
            None => Err(format!("no usable program for {}", font.base_name)),
        }
    }

    /// the glyph a char code draws, in the order the pdf spec gives.
    pub fn glyph_for(&self, code: u32) -> Option<GlyphRef> {
        let font = self.font;
        match &self.program {
            Program::Type3 => font
                .type3_procs
                .contains_key(&code)
                .then_some(GlyphRef::Type3(code)),
            Program::Type1(type1) => {
                let name = font
                    .glyph_names
                    .get(&code)
                    .filter(|n| type1.charstrings.contains_key(*n));
                let name = name.or_else(|| {
                    type1
                        .encoding
                        .get(&code)
                        .filter(|n| type1.charstrings.contains_key(*n))
                });
                name.map(|n| GlyphRef::Name(n.clone()))
            }
            Program::Cff(bytes) => {
                let table = ttf_parser::cff::Table::parse(bytes)?;
                if font.is_cid() {
                    // cid-keyed cff: the cid through the charset; otherwise the cid is the glyph index
                    if !self.cid_glyphs.is_empty() {
                        return self
                            .cid_glyphs
                            .get(&(code as u16))
                            .map(|&g| GlyphRef::Index(g));
                    }
                    return (code < u32::from(table.number_of_glyphs()))
                        .then_some(GlyphRef::Index(code as u16));
                }
                if let Some(name) = font.glyph_names.get(&code) {
                    if let Some(glyph) = table.glyph_index_by_name(name) {
                        return Some(GlyphRef::Index(glyph.0));
                    }
                }
                // the program's built-in encoding
                u8::try_from(code)
                    .ok()
                    .and_then(|c| table.glyph_index(c))
                    .map(|g| GlyphRef::Index(g.0))
            }
            Program::Sfnt(bytes) => {
                let face = ttf_parser::Face::parse(bytes, 0).ok()?;
                self.sfnt_glyph(&face, code).map(GlyphRef::Index)
            }
        }
    }

    fn sfnt_glyph(&self, face: &ttf_parser::Face, code: u32) -> Option<u16> {
        let font = self.font;
        if font.is_cid() {
            let gid = match &font.cid_to_gid {
                Some(map) if 2 * code as usize + 1 < map.len() => {
                    u16::from_be_bytes([map[2 * code as usize], map[2 * code as usize + 1]])
                }
                _ => code as u16,
            };
            return (gid < face.number_of_glyphs()).then_some(gid);
        }
        let cmaps: Vec<ttf_parser::cmap::Subtable> = face
            .tables()
            .cmap
            .map(|cmap| cmap.subtables.into_iter().collect())
            .unwrap_or_default();
        let lookup = |platform: u16, encoding: u16, value: u32| -> Option<u16> {
            cmaps
                .iter()
                .find(|s| s.platform_id as u16 == platform && s.encoding_id == encoding)
                .and_then(|s| s.glyph_index(value))
                .map(|g| g.0)
        };
        let name = font.glyph_names.get(&code);
        // simple truetype: (3,0) with the code, then 0xF000 + code, then (1,0) (pdf 1.7, 9.6.6.4)
        for (platform, encoding, value) in [(3u16, 0u16, code), (3, 0, 0xF000 + code), (1, 0, code)]
        {
            let symbolic_ok = font.symbolic || name.is_none() || (platform, encoding) != (3, 0);
            if symbolic_ok {
                if let Some(glyph) = lookup(platform, encoding, value) {
                    return Some(glyph);
                }
            }
        }
        // then the (3,1) cmap by the code's unicode
        let text: Vec<char> = font.text(code).chars().collect();
        if let [single] = text.as_slice() {
            if let Some(glyph) = lookup(3, 1, *single as u32) {
                return Some(glyph);
            }
        }
        // then the glyph name from the encoding
        if let Some(glyph) = name.and_then(|n| face.glyph_index_by_name(n)) {
            return Some(glyph.0);
        }
        // last: any cmap that knows the code
        cmaps.iter().find_map(|s| s.glyph_index(code)).map(|g| g.0)
    }

    /// the outline of a glyph in the program's own units.
    pub fn outline(&self, glyph: &GlyphRef, problems: &mut Vec<String>) -> Outline {
        let mut recorder = Recorder::default();
        match (&self.program, glyph) {
            (Program::Sfnt(bytes), GlyphRef::Index(index)) => {
                if let Ok(face) = ttf_parser::Face::parse(bytes, 0) {
                    face.outline_glyph(GlyphId(*index), &mut recorder);
                }
            }
            (Program::Cff(bytes), GlyphRef::Index(index)) => {
                if let Some(table) = ttf_parser::cff::Table::parse(bytes) {
                    let _ = table.outline(GlyphId(*index), &mut recorder);
                }
            }
            (Program::Type1(type1), GlyphRef::Name(name)) => {
                return type1.outline(name).unwrap_or_default();
            }
            (Program::Type3, GlyphRef::Type3(code)) => {
                if let Some(procedure) = self.font.type3_procs.get(code) {
                    return type3_outline(procedure, &self.font.font_matrix, problems);
                }
            }
            _ => {}
        }
        recorder.outline
    }
}
