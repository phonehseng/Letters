//! fonts that new text is written in, and adding fonts to the file.
//!
//! new text is written in one of three kinds of font: the original font
//! (when it can draw every character), a standard font (built into every
//! reader, nothing embedded), or a TrueType font copied into the file.
//! a copied font is subset to the glyphs the text uses and written as a
//! composite (CID) font with a /ToUnicode map, so the text can be copied
//! out again and searched.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::cmap;
use crate::fonts::encodings::win_ansi_code;
use crate::fonts::pdf_font::{EncodedChar, PdfFont};
use crate::fonts::standard::StandardFont;
use crate::fonts::truetype::{Embedding, FontProgram};
use crate::pdf_values::{name, number_array, real, stream_bytes};

/// a font that new text is written in.
#[derive(Clone, Debug)]
pub(crate) struct WriteFont {
    /// the font object, already in the file or added by this edit
    pub object: ObjectId,
    /// the name people know it by, for notices ("Helvetica")
    pub display_name: String,
    pub codes: BTreeMap<char, EncodedChar>,
    /// thousandths of the font size
    pub ascent: f32,
}

impl WriteFont {
    pub fn can_draw(&self, character: char) -> bool {
        self.codes.contains_key(&character)
    }

    /// a stand-in for a font that can't draw anything new.
    pub fn nothing(name: &str) -> WriteFont {
        WriteFont {
            object: (0, 0),
            display_name: name.to_owned(),
            codes: BTreeMap::new(),
            ascent: 750.0,
        }
    }

    /// the original font, limited to the characters it can draw.
    pub fn from_original(font: &PdfFont, object: ObjectId, needed: &BTreeSet<char>) -> WriteFont {
        let codes = needed
            .iter()
            .filter_map(|character| {
                let encoded = font.encode(&character.to_string()).ok()?.pop()?;
                Some((*character, encoded))
            })
            .collect();
        WriteFont {
            object,
            display_name: font.name.clone(),
            codes,
            ascent: font.ascent,
        }
    }
}

/// adds a standard font (WinAnsiEncoding, with its widths) to the file.
pub(crate) fn standard_font(draft: &mut Draft, font: StandardFont) -> WriteFont {
    let object = draft.add(Object::Dictionary(font.dictionary()));
    let codes = (32..=255u8)
        .filter_map(|code| {
            let character = crate::fonts::encodings::win_ansi_char(code)?;
            Some((
                character,
                EncodedChar {
                    code: vec![code],
                    width: font.char_width(code),
                    is_word_space: code == 32,
                },
            ))
        })
        .collect();
    let (ascent, _) = font.ascent_and_descent();
    WriteFont {
        object,
        display_name: font.base_font_name().to_owned(),
        codes,
        ascent,
    }
}

/// the characters of `text` that a standard font can't draw.
pub(crate) fn not_in_standard_fonts(text: &str) -> Vec<char> {
    let mut missing = Vec::new();
    for character in text.chars() {
        if win_ansi_code(character).is_none() && !missing.contains(&character) {
            missing.push(character);
        }
    }
    missing
}

/// six capital letters that tag a subset font, made from what it holds
/// so the same subset always gets the same tag.
fn subset_tag(name: &str, glyphs: &BTreeSet<u16>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    glyphs.hash(&mut hasher);
    let mut value = hasher.finish();
    (0..6)
        .map(|_| {
            let letter = char::from(b'A' + (value % 26) as u8);
            value /= 26;
            letter
        })
        .collect()
}

/// copies the glyphs `characters` need from a TrueType font into the
/// file as a composite font, respecting the font's licence bits.
pub(crate) fn truetype_font(
    draft: &mut Draft,
    program: &FontProgram,
    characters: &BTreeSet<char>,
    display_name: &str,
) -> Result<WriteFont, EditError> {
    let embedding = program.embedding();
    if embedding == Embedding::Forbidden {
        return Err(EditError::EmbeddingNotAllowed(display_name.to_owned()));
    }
    if !program.is_truetype_outlines() {
        return Err(EditError::UnsupportedFont(
            "Only fonts with TrueType outlines (most .ttf files) can be added.".into(),
        ));
    }
    let mut glyphs = BTreeMap::new();
    let mut missing = String::new();
    for character in characters {
        match program.glyph_for_char(*character) {
            Some(glyph) => {
                glyphs.insert(*character, glyph);
            }
            None if character.is_whitespace() => {}
            None => missing.push(*character),
        }
    }
    if !missing.is_empty() {
        return Err(EditError::UnsupportedCharacters(missing));
    }
    let used: BTreeSet<u16> = glyphs.values().copied().collect();
    let file = match embedding {
        Embedding::WholeFontOnly => program.single_font_bytes()?,
        _ => program.subset(&used)?,
    };
    let postscript = program
        .postscript_name()
        .unwrap_or_else(|| display_name.replace(' ', ""))
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "");
    let base_font = format!("{}+{postscript}", subset_tag(&postscript, &used));
    let object = add_composite_font(draft, program, &file, &base_font, &glyphs);
    let codes = glyphs
        .iter()
        .map(|(character, glyph)| {
            (
                *character,
                EncodedChar {
                    code: glyph.to_be_bytes().to_vec(),
                    width: program.advance(*glyph),
                    is_word_space: false,
                },
            )
        })
        .collect();
    Ok(WriteFont {
        object,
        display_name: display_name.to_owned(),
        codes,
        ascent: program.scale(program.face.ascender()),
    })
}

/// writes the objects of a composite TrueType font: the Type0 font, its
/// CIDFontType2 descendant, the descriptor, the font file and the
/// /ToUnicode map. glyph numbers are the codes (Identity-H and Identity
/// CIDToGIDMap), which works because the subset keeps glyph numbers.
fn add_composite_font(
    draft: &mut Draft,
    program: &FontProgram,
    file: &[u8],
    base_font: &str,
    glyphs: &BTreeMap<char, u16>,
) -> ObjectId {
    let mut file_dict = Dictionary::new();
    file_dict.set("Length1", Object::Integer(file.len() as i64));
    let mut file_stream = Stream::new(file_dict, file.to_vec());
    let _ = file_stream.compress();
    let file_id = draft.add(Object::Stream(file_stream));

    let face = &program.face;
    let bbox = face.global_bounding_box();
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", name("FontDescriptor"));
    descriptor.set("FontName", name(base_font));
    // nonsymbolic: the glyphs are ordinary latin-style characters
    descriptor.set("Flags", Object::Integer(32));
    descriptor.set(
        "FontBBox",
        number_array(&[
            program.scale(bbox.x_min),
            program.scale(bbox.y_min),
            program.scale(bbox.x_max),
            program.scale(bbox.y_max),
        ]),
    );
    descriptor.set("ItalicAngle", real(face.italic_angle()));
    descriptor.set("Ascent", real(program.scale(face.ascender())));
    descriptor.set("Descent", real(program.scale(face.descender())));
    descriptor.set(
        "CapHeight",
        real(program.scale(face.capital_height().unwrap_or(face.ascender()))),
    );
    descriptor.set("StemV", Object::Integer(80));
    descriptor.set("FontFile2", Object::Reference(file_id));
    let descriptor_id = draft.add(Object::Dictionary(descriptor));

    let mut widths = Vec::new();
    for glyph in glyphs.values().collect::<BTreeSet<_>>() {
        widths.push(Object::Integer(i64::from(*glyph)));
        widths.push(Object::Array(vec![real(program.advance(*glyph))]));
    }
    let mut system_info = Dictionary::new();
    system_info.set("Registry", Object::string_literal("Adobe"));
    system_info.set("Ordering", Object::string_literal("Identity"));
    system_info.set("Supplement", Object::Integer(0));
    let mut descendant = Dictionary::new();
    descendant.set("Type", name("Font"));
    descendant.set("Subtype", name("CIDFontType2"));
    descendant.set("BaseFont", name(base_font));
    descendant.set("CIDSystemInfo", Object::Dictionary(system_info));
    descendant.set("FontDescriptor", Object::Reference(descriptor_id));
    descendant.set("DW", Object::Integer(1000));
    descendant.set("W", Object::Array(widths));
    descendant.set("CIDToGIDMap", name("Identity"));
    let descendant_id = draft.add(Object::Dictionary(descendant));

    let entries: BTreeMap<u16, String> = glyphs
        .iter()
        .map(|(character, glyph)| (*glyph, character.to_string()))
        .collect();
    let mut to_unicode = Stream::new(Dictionary::new(), cmap::write_two_byte_to_unicode(&entries));
    let _ = to_unicode.compress();
    let to_unicode_id = draft.add(Object::Stream(to_unicode));

    let mut font = Dictionary::new();
    font.set("Type", name("Font"));
    font.set("Subtype", name("Type0"));
    font.set("BaseFont", name(base_font));
    font.set("Encoding", name("Identity-H"));
    font.set(
        "DescendantFonts",
        Object::Array(vec![Object::Reference(descendant_id)]),
    );
    font.set("ToUnicode", Object::Reference(to_unicode_id));
    draft.add(Object::Dictionary(font))
}

/// adds characters to a composite font's /ToUnicode map, for glyphs the
/// font could draw but the map didn't list yet.
pub(crate) fn extend_to_unicode(
    draft: &mut Draft,
    font_object: ObjectId,
    additions: &BTreeMap<u16, String>,
) {
    let Some(font) = draft.dictionary(font_object).cloned() else {
        return;
    };
    let existing = font
        .get(b"ToUnicode")
        .ok()
        .and_then(|stream| stream_bytes(draft.doc, draft.resolve(stream)))
        .map(|bytes| cmap::parse(&bytes))
        .unwrap_or_default();
    let mut entries: BTreeMap<u16, String> = existing
        .to_text
        .iter()
        .filter(|((length, _), _)| *length == 2)
        .map(|((_, code), text)| (*code as u16, text.clone()))
        .collect();
    entries.extend(additions.clone());
    let mut stream = Stream::new(Dictionary::new(), cmap::write_two_byte_to_unicode(&entries));
    let _ = stream.compress();
    // a new map object, so a map shared with another font is left alone
    let map_id = draft.add(Object::Stream(stream));
    let mut font = font;
    font.set("ToUnicode", Object::Reference(map_id));
    draft.set(font_object, Object::Dictionary(font));
}
