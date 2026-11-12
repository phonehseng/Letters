//! a font as a pdf page uses it: how its codes become text and widths,
//! and exactly which characters it can still draw.
//!
//! "which characters it can draw" is the heart of keeping the original
//! font. a font that isn't embedded can draw anything its encoding holds
//! (the reader supplies the glyphs). an embedded font can only draw the
//! glyphs that are really in the embedded program, and a subset font
//! (name starting "ABCDEF+") usually holds only the characters the page
//! already uses. this module checks the program itself where it can.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::fonts::cmap::{self, CMap};
use crate::fonts::encodings::{chars_of_name, BaseEncoding};
use crate::fonts::standard::StandardFont;
use crate::fonts::truetype::FontProgram;
use crate::fonts::type1;
use crate::geometry::Matrix;
use crate::pdf_values::{
    dict_array, dict_dict, dict_name, dict_number, dict_numbers, resolve, stream_bytes,
};

/// the kind of font program, as the ui may want to show it.
mod dictionary;
mod program;

use dictionary::*;
use program::embedded_program;
pub(crate) use program::Program;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontKind {
    Type1,
    TrueType,
    Type3,
    /// a composite font with two-byte codes (Type0 / CID)
    Composite,
}

/// a guess at the look of a font, for picking a similar standard font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontFamily {
    Serif,
    SansSerif,
    Monospace,
}

/// one glyph a show operator draws: its code, its text and its width.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedGlyph {
    pub code: Vec<u8>,
    pub text: String,
    /// thousandths of the font size
    pub width: f32,
    /// a single-byte code 32, the only one word spacing applies to
    pub is_word_space: bool,
}

/// one character encoded for this font.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EncodedChar {
    pub code: Vec<u8>,
    pub width: f32,
    pub is_word_space: bool,
}

/// a font loaded from a page's resources.
#[derive(Clone, Debug)]
pub(crate) struct PdfFont {
    /// the font object, when it has one (fonts are nearly always objects)
    pub object: Option<ObjectId>,
    /// the /BaseFont name, with any subset prefix taken off
    pub name: String,
    pub subset: bool,
    pub embedded: bool,
    pub kind: FontKind,
    pub family: FontFamily,
    pub bold: bool,
    pub italic: bool,
    two_byte: bool,
    /// code to text, from the encoding and /ToUnicode
    texts: HashMap<Vec<u8>, String>,
    /// code to width in thousandths of the font size
    widths: HashMap<Vec<u8>, f32>,
    default_width: f32,
    /// character to the code that draws it, for characters the font can
    /// really draw
    drawable: BTreeMap<char, Vec<u8>>,
    /// ascent and descent in thousandths of the font size
    pub ascent: f32,
    pub descent: f32,
    /// glyph space to text space; [0.001 0 0 0.001 0 0] except for Type3
    pub font_matrix: Matrix,
    /// characters the font can draw but /ToUnicode doesn't list yet; a
    /// write adds them so copied text stays right
    pub missing_from_to_unicode: BTreeSet<char>,
}

/// the font's ascent and descent in thousandths of the size: from its
/// descriptor, else from the standard font of that name, else 750 and 250.
fn ascent_and_descent(doc: &Document, descriptor: Option<&Dictionary>, name: &str) -> (f32, f32) {
    let standard = StandardFont::from_name(name);
    let (default_ascent, default_descent) = standard
        .map(|font| font.ascent_and_descent())
        .unwrap_or((750.0, 250.0));
    let ascent = descriptor
        .and_then(|d| dict_number(doc, d, b"Ascent"))
        .filter(|a| *a > 0.0)
        .unwrap_or(default_ascent);
    let descent = descriptor
        .and_then(|d| dict_number(doc, d, b"Descent"))
        .map(f32::abs)
        .filter(|d| *d > 0.0)
        .unwrap_or(default_descent);
    (ascent, descent)
}

/// the subset prefix is six capital letters and a plus sign.
fn split_subset_prefix(base: &str) -> (bool, String) {
    let bytes = base.as_bytes();
    let prefixed =
        bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(u8::is_ascii_uppercase);
    if prefixed {
        (true, base[7..].to_owned())
    } else {
        (false, base.to_owned())
    }
}

impl PdfFont {
    /// reads a font dictionary. anything missing or odd is filled with
    /// what readers assume, so text can still be read and measured.
    pub fn load(doc: &Document, object: Option<ObjectId>, font: &Dictionary) -> PdfFont {
        let subtype = dict_name(doc, font, b"Subtype").unwrap_or(b"Type1");
        let base = dict_name(doc, font, b"BaseFont")
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .unwrap_or_else(|| "Unnamed".into());
        let (subset, name) = split_subset_prefix(&base);
        let kind = match subtype {
            b"Type0" => FontKind::Composite,
            b"TrueType" => FontKind::TrueType,
            b"Type3" => FontKind::Type3,
            _ => FontKind::Type1,
        };
        let descriptor_owner = match kind {
            FontKind::Composite => descendant(doc, font).unwrap_or(font),
            _ => font,
        };
        let descriptor = dict_dict(doc, descriptor_owner, b"FontDescriptor");
        let flags = descriptor
            .and_then(|d| dict_number(doc, d, b"Flags"))
            .unwrap_or(0.0) as u32;
        let (family, bold, italic) = guess_style(
            &name,
            flags,
            descriptor.and_then(|d| dict_number(doc, d, b"FontWeight")),
        );
        let (ascent, descent) = ascent_and_descent(doc, descriptor, &name);
        let program = descriptor.and_then(|d| embedded_program(doc, d));
        let to_unicode = font
            .get(b"ToUnicode")
            .ok()
            .and_then(|stream| stream_bytes(doc, stream))
            .map(|bytes| cmap::parse(&bytes));
        let mut loaded = PdfFont {
            object,
            name,
            subset,
            embedded: program.is_some(),
            kind,
            family,
            bold,
            italic,
            two_byte: kind == FontKind::Composite,
            texts: HashMap::new(),
            widths: HashMap::new(),
            default_width: 0.0,
            drawable: BTreeMap::new(),
            ascent,
            descent,
            font_matrix: Matrix::new(0.001, 0.0, 0.0, 0.001, 0.0, 0.0),
            missing_from_to_unicode: BTreeSet::new(),
        };
        match kind {
            FontKind::Composite => {
                loaded.load_composite(doc, font, program.as_ref(), to_unicode.as_ref())
            }
            _ => loaded.load_simple(
                doc,
                font,
                descriptor,
                program.as_ref(),
                to_unicode.as_ref(),
                flags,
            ),
        }
        loaded
    }

    fn load_simple(
        &mut self,
        doc: &Document,
        font: &Dictionary,
        descriptor: Option<&Dictionary>,
        program: Option<&Program>,
        to_unicode: Option<&CMap>,
        flags: u32,
    ) {
        const SYMBOLIC: u32 = 4;
        let standard = StandardFont::from_name(&self.name);
        let names = simple_glyph_names(doc, font, self.kind, flags & SYMBOLIC != 0);
        if self.kind == FontKind::Type3 {
            if let Some(matrix) = dict_numbers(doc, font, b"FontMatrix")
                .and_then(|values| Matrix::from_slice(&values))
            {
                self.font_matrix = matrix;
            }
        }
        // widths are stored in glyph space; type3 glyph space is scaled
        // by its font matrix, so convert to the usual thousandths
        let glyph_scale = self.font_matrix.a * 1000.0;
        let first = dict_number(doc, font, b"FirstChar").unwrap_or(0.0) as i64;
        let widths = dict_numbers(doc, font, b"Widths");
        let missing = descriptor
            .and_then(|d| dict_number(doc, d, b"MissingWidth"))
            .unwrap_or(0.0);
        self.default_width = missing * glyph_scale;
        let program_glyphs = program.and_then(|program| program.glyph_check());
        let type3_glyphs: Option<HashSet<Vec<u8>>> = (self.kind == FontKind::Type3).then(|| {
            dict_dict(doc, font, b"CharProcs")
                .map(|procs| procs.iter().map(|(name, _)| name.clone()).collect())
                .unwrap_or_default()
        });

        for code in 0..=255u8 {
            let glyph_name = names[usize::from(code)].as_deref();
            let text = to_unicode
                .and_then(|map| map.to_text.get(&(1, u32::from(code))).cloned())
                .or_else(|| glyph_name.and_then(chars_of_name));
            let width = match &widths {
                Some(list) => {
                    let index = i64::from(code) - first;
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| list.get(index))
                        .map(|width| width * glyph_scale)
                }
                None => standard.map(|font| font.char_width(code)),
            };
            if let Some(width) = width {
                self.widths.insert(vec![code], width);
            }
            if let Some(text) = &text {
                self.texts.insert(vec![code], text.clone());
            }
            let Some(character) = text.as_deref().and_then(single_char) else {
                continue;
            };
            let available = match (&program_glyphs, &type3_glyphs) {
                (_, Some(procs)) => glyph_name.is_some_and(|name| procs.contains(name.as_bytes())),
                (Some(check), _) => check.has_simple_glyph(code, glyph_name, character),
                // not embedded: the reader draws anything the encoding holds
                (None, None) => true,
            };
            let has_width = width.is_some_and(|width| width > 0.0) || character.is_whitespace();
            if available && has_width && (glyph_name.is_some() || to_unicode.is_some()) {
                self.drawable.entry(character).or_insert_with(|| vec![code]);
            }
        }
        if self.embedded && program_glyphs.is_none() {
            // an embedded program Letters can't look inside (a bare CFF
            // font): only characters it is seen drawing count, see `seen`
            self.drawable.clear();
            if let Some(names_found) = descriptor.and_then(|d| charset_names(doc, d)) {
                self.allow_named(&names_found, &names);
            }
        }
    }

    /// marks characters as drawable whose glyph name is in `available`.
    fn allow_named(&mut self, available: &HashSet<String>, names: &[Option<String>]) {
        for (code, name) in names.iter().enumerate() {
            let (Some(name), Some(text)) = (name, self.texts.get(&vec![code as u8])) else {
                continue;
            };
            if available.contains(name) {
                if let Some(character) = single_char(text) {
                    self.drawable
                        .entry(character)
                        .or_insert_with(|| vec![code as u8]);
                }
            }
        }
    }

    fn load_composite(
        &mut self,
        doc: &Document,
        font: &Dictionary,
        program: Option<&Program>,
        to_unicode: Option<&CMap>,
    ) {
        let Some(descendant) = descendant(doc, font) else {
            return;
        };
        let encoding = dict_name(doc, font, b"Encoding").unwrap_or(b"Identity-H");
        let identity = matches!(encoding, b"Identity-H" | b"Identity-V");
        self.default_width = dict_number(doc, descendant, b"DW").unwrap_or(1000.0);
        if let Some(list) = dict_array(doc, descendant, b"W") {
            for (cid, width) in read_cid_widths(doc, list) {
                self.widths
                    .insert((cid as u16).to_be_bytes().to_vec(), width);
            }
        }
        if let Some(map) = to_unicode {
            for ((length, code), text) in &map.to_text {
                if *length == 2 {
                    self.texts
                        .insert((*code as u16).to_be_bytes().to_vec(), text.clone());
                }
            }
        }
        if !identity {
            // codes of other cmaps can't be written back reliably, so the
            // font is read but never written to
            return;
        }
        let cid_to_gid = cid_to_gid_map(doc, descendant);
        let truetype = program.and_then(Program::truetype);
        for (code, text) in &self.texts {
            let Some(character) = single_char(text) else {
                continue;
            };
            let cid = u16::from_be_bytes([code[0], code[1]]);
            let glyph = cid_to_gid
                .as_ref()
                .map_or(cid, |map| map.get(&cid).copied().unwrap_or(0));
            let available = match &truetype {
                Some(face) => {
                    face.has_outline(glyph)
                        || (character.is_whitespace() && face.glyph_exists(glyph))
                }
                // other programs (or none): what the page shows exists
                None => true,
            };
            if available {
                self.drawable
                    .entry(character)
                    .or_insert_with(|| code.clone());
            }
        }
        // a full embedded font can draw characters the page never used,
        // through its own cmap, when glyph numbers are the codes
        if let (Some(face), None) = (&truetype, &cid_to_gid) {
            for table in face
                .face
                .tables()
                .cmap
                .into_iter()
                .flat_map(|cmap| cmap.subtables)
            {
                if !table.is_unicode() {
                    continue;
                }
                table.codepoints(|codepoint| {
                    let Some(character) = char::from_u32(codepoint) else {
                        return;
                    };
                    if self.drawable.contains_key(&character) {
                        return;
                    }
                    if let Some(glyph) = table
                        .glyph_index(codepoint)
                        .map(|g| g.0)
                        .filter(|g| face.has_outline(*g) || character == ' ')
                    {
                        self.drawable
                            .insert(character, glyph.to_be_bytes().to_vec());
                        self.widths
                            .entry(glyph.to_be_bytes().to_vec())
                            .or_insert_with(|| face.advance(glyph));
                        self.missing_from_to_unicode.insert(character);
                    }
                });
            }
        }
    }

    /// records that the page draws this code, which proves the glyph is
    /// there even when the program can't be read.
    pub fn seen(&mut self, code: &[u8]) {
        if !self.embedded || self.kind == FontKind::Composite {
            return;
        }
        if let Some(character) = self.texts.get(code).and_then(|text| single_char(text)) {
            self.drawable
                .entry(character)
                .or_insert_with(|| code.to_vec());
        }
    }

    /// a code's width in thousandths of the font size.
    pub fn width_of(&self, code: &[u8]) -> f32 {
        self.widths.get(code).copied().unwrap_or(self.default_width)
    }

    /// each character the font can really draw, and the code that draws it.
    pub fn drawable_codes(&self) -> &BTreeMap<char, Vec<u8>> {
        &self.drawable
    }

    /// splits a show operator's string into glyphs.
    pub fn decode(&self, bytes: &[u8]) -> Vec<DecodedGlyph> {
        let step = if self.two_byte { 2 } else { 1 };
        bytes
            .chunks(step)
            .map(|code| DecodedGlyph {
                code: code.to_vec(),
                text: self.texts.get(code).cloned().unwrap_or_default(),
                width: self.width_of(code),
                is_word_space: code == [32],
            })
            .collect()
    }

    /// encodes text, or returns the characters this font can't draw
    /// (each listed once, in the order they first appear).
    pub fn encode(&self, text: &str) -> Result<Vec<EncodedChar>, Vec<char>> {
        let mut encoded = Vec::new();
        let mut missing = Vec::new();
        for character in text.chars() {
            match self.drawable.get(&character) {
                Some(code) => encoded.push(EncodedChar {
                    width: self.width_of(code),
                    is_word_space: code == &[32],
                    code: code.clone(),
                }),
                None if !missing.contains(&character) => missing.push(character),
                None => {}
            }
        }
        if missing.is_empty() {
            Ok(encoded)
        } else {
            Err(missing)
        }
    }

    /// every character the font can still draw, in code point order.
    pub fn drawable_chars(&self) -> String {
        self.drawable.keys().collect()
    }

    /// whether new text can be written in this font at all.
    pub fn writable(&self) -> bool {
        !self.drawable.is_empty() && (self.kind != FontKind::Composite || self.two_byte)
    }

    pub fn closest_standard_font(&self) -> StandardFont {
        let family = match self.family {
            FontFamily::Serif => "times",
            FontFamily::Monospace => "courier",
            FontFamily::SansSerif => "helvetica",
        };
        let name = format!(
            "{family}{}{}",
            if self.bold { " bold" } else { "" },
            if self.italic { " italic" } else { "" }
        );
        StandardFont::from_name(&name).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_values::name;

    fn font(entries: &[(&str, Object)]) -> Dictionary {
        let mut font = Dictionary::new();
        font.set("Type", name("Font"));
        for (key, value) in entries {
            font.set(*key, value.clone());
        }
        font
    }

    #[test]
    fn a_standard_font_that_is_not_embedded_draws_its_whole_encoding() {
        let doc = Document::new();
        let helvetica = PdfFont::load(
            &doc,
            None,
            &font(&[
                ("Subtype", name("Type1")),
                ("BaseFont", name("Helvetica")),
                ("Encoding", name("WinAnsiEncoding")),
            ]),
        );
        let drawable = helvetica.drawable_chars();
        assert!(drawable.contains('é') && drawable.contains('€') && !drawable.contains('東'));
        let encoded = helvetica.encode("Hé").unwrap();
        assert_eq!(encoded[1].code, [0xe9]);
        assert_eq!(
            encoded[0].width, 722.0,
            "helvetica H from the standard widths"
        );
        assert_eq!(helvetica.decode(b"H")[0].text, "H");
        assert_eq!(helvetica.family, FontFamily::SansSerif);
    }

    #[test]
    fn differences_rename_codes() {
        let doc = Document::new();
        let mut encoding = Dictionary::new();
        encoding.set("BaseEncoding", name("WinAnsiEncoding"));
        encoding.set(
            "Differences",
            Object::Array(vec![Object::Integer(1), name("H"), name("eacute")]),
        );
        let custom = PdfFont::load(
            &doc,
            None,
            &font(&[
                ("Subtype", name("Type1")),
                ("BaseFont", name("ABCDEF+Times-Bold")),
                ("Encoding", Object::Dictionary(encoding)),
                ("FirstChar", Object::Integer(1)),
                (
                    "Widths",
                    Object::Array(vec![Object::Integer(778), Object::Integer(444)]),
                ),
            ]),
        );
        assert!(custom.subset && custom.name == "Times-Bold" && custom.bold);
        assert_eq!(
            custom
                .decode(b"\x01\x02")
                .iter()
                .map(|g| g.text.as_str())
                .collect::<String>(),
            "Hé"
        );
        assert_eq!(custom.encode("é").unwrap()[0].code, [2]);
        assert_eq!(custom.closest_standard_font(), StandardFont::TimesBold);
    }

    #[test]
    fn style_is_guessed_from_names_and_flags() {
        assert_eq!(
            guess_style("CourierNewPSMT", 0, None).0,
            FontFamily::Monospace
        );
        assert_eq!(
            guess_style("Georgia-BoldItalic", 0, None),
            (FontFamily::Serif, true, true)
        );
        assert_eq!(
            guess_style("Unknown", 2, Some(700.0)),
            (FontFamily::Serif, true, false)
        );
    }
}
