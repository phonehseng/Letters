//! the embedded font program (TrueType, Type1 or other) and checking
//! which glyphs it really holds.

use super::*;

/// an embedded font program and what it is.
pub(crate) enum Program {
    TrueType(Vec<u8>),
    Type1(Vec<u8>, usize),
    /// CFF or OpenType: kept, but only OpenType can be looked inside
    Other,
}

pub(super) fn embedded_program(doc: &Document, descriptor: &Dictionary) -> Option<Program> {
    if let Some(bytes) = descriptor
        .get(b"FontFile2")
        .ok()
        .and_then(|file| stream_bytes(doc, file))
    {
        return Some(Program::TrueType(bytes));
    }
    if let Ok(file) = descriptor.get(b"FontFile") {
        let clear_length = resolve(doc, file)
            .as_stream()
            .ok()
            .and_then(|stream| dict_number(doc, &stream.dict, b"Length1"))
            .unwrap_or(0.0) as usize;
        return stream_bytes(doc, file).map(|bytes| Program::Type1(bytes, clear_length));
    }
    let file = descriptor.get(b"FontFile3").ok()?;
    let subtype = resolve(doc, file)
        .as_stream()
        .ok()
        .and_then(|stream| dict_name(doc, &stream.dict, b"Subtype"));
    let bytes = stream_bytes(doc, file)?;
    Some(if subtype == Some(b"OpenType") {
        Program::TrueType(bytes)
    } else {
        Program::Other
    })
}

/// a way of checking glyphs in an embedded program.
pub(crate) enum GlyphCheck<'a> {
    TrueType(Box<FontProgram<'a>>, bool),
    Named(HashSet<String>),
}

impl Program {
    pub(super) fn truetype(&self) -> Option<FontProgram<'_>> {
        match self {
            Program::TrueType(bytes) => FontProgram::parse(bytes, 0),
            _ => None,
        }
    }

    pub(super) fn glyph_check(&self) -> Option<GlyphCheck<'_>> {
        match self {
            Program::TrueType(bytes) => FontProgram::parse(bytes, 0).map(|face| {
                let symbolic_cmap = face.face.tables().cmap.is_some_and(|cmap| {
                    !cmap.subtables.into_iter().any(|table| table.is_unicode())
                });
                GlyphCheck::TrueType(Box::new(face), symbolic_cmap)
            }),
            Program::Type1(bytes, clear_length) => {
                type1::glyph_names(bytes, *clear_length).map(GlyphCheck::Named)
            }
            Program::Other => None,
        }
    }
}

impl GlyphCheck<'_> {
    /// whether a simple font's code really has a glyph in the program.
    pub(super) fn has_simple_glyph(
        &self,
        code: u8,
        glyph_name: Option<&str>,
        character: char,
    ) -> bool {
        match self {
            GlyphCheck::TrueType(face, only_symbolic) => {
                // readers try the unicode cmap for named glyphs first and
                // the symbolic or mac cmap by code otherwise
                let glyph = if *only_symbolic {
                    face.glyph_for_code(code)
                } else {
                    face.glyph_for_char(character)
                        .or_else(|| face.glyph_for_code(code))
                };
                glyph.is_some_and(|glyph| {
                    face.has_outline(glyph)
                        || (character.is_whitespace() && face.glyph_exists(glyph))
                })
            }
            GlyphCheck::Named(names) => glyph_name.is_some_and(|name| names.contains(name)),
        }
    }
}
