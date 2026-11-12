//! what the redaction engine needs to know about a font: how a string's
//! bytes split into character codes, how far each code moves the pen,
//! how tall the glyphs are, and which text each code stands for.
//!
//! the widths must be exactly the ones the viewer uses, because they
//! decide where every later glyph on the line is drawn. if a font's widths
//! cannot be known (`metrics_known` is false), the text engine treats its
//! text as one block rather than guessing glyph positions.

pub mod cmap;
mod glyph_names;
pub mod standard;
mod standard_metrics;

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object};

use crate::geometry::{Matrix, Rect};
use crate::objects::{
    as_dict, decoded_content, dict_array, dict_dict, dict_get, dict_name, dict_number, numbers,
    resolve,
};
use cmap::{CMap, CodeSpace};
use standard::{glyph_unicode, named_encoding, standard_font, strip_subset_prefix, StandardFont};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontKind {
    /// one byte per code: Type1, TrueType, MMType1
    Simple,
    /// glyphs drawn by small content streams inside the font
    Type3,
    /// Type0 fonts, with codes of one to four bytes mapped to cids
    Composite,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub name: String,
    pub kind: FontKind,
    /// false when the letter widths could not be found, so glyph
    /// positions after the first one are not known
    pub metrics_known: bool,
    /// true for vertical writing, where the pen moves down the page
    pub vertical: bool,
    /// top and bottom of the glyphs as a fraction of the font size
    pub ascent: f64,
    pub descent: f64,
    code_space: Option<CodeSpace>,
    simple_widths: Vec<Option<f64>>,
    missing_width: f64,
    cid_map: Option<CMap>,
    cid_is_code: bool,
    cid_widths: HashMap<u32, f64>,
    default_width: f64,
    glyph_names: Vec<Option<String>>,
    unicode: HashMap<u32, String>,
    code_is_unicode: bool,
    type3_matrix: Matrix,
    type3_bounds: Option<Rect>,
}

/// what a font is assumed to look like when nothing better is known.
const DEFAULT_ASCENT: f64 = 0.8;
const DEFAULT_DESCENT: f64 = -0.2;

impl Font {
    /// a stand-in for a font that is missing from the resources. its
    /// widths are unknown, so its text is only ever handled as a block.
    pub fn unknown(name: &str) -> Font {
        Font {
            name: name.to_owned(),
            kind: FontKind::Simple,
            metrics_known: false,
            vertical: false,
            ascent: DEFAULT_ASCENT,
            descent: DEFAULT_DESCENT,
            code_space: None,
            simple_widths: vec![None; 256],
            missing_width: 0.0,
            cid_map: None,
            cid_is_code: false,
            cid_widths: HashMap::new(),
            default_width: 1000.0,
            glyph_names: vec![None; 256],
            unicode: HashMap::new(),
            code_is_unicode: false,
            type3_matrix: Matrix::scale(0.001, 0.001),
            type3_bounds: None,
        }
    }

    pub fn load(doc: &Document, dict: &Dictionary) -> Font {
        let base_font = dict_name(doc, dict, b"BaseFont").unwrap_or(b"");
        let mut font = Font::unknown(&String::from_utf8_lossy(strip_subset_prefix(base_font)));
        match dict_name(doc, dict, b"Subtype").unwrap_or(b"") {
            b"Type0" => font.load_composite(doc, dict),
            b"Type3" => font.load_type3(doc, dict),
            _ => font.load_simple(doc, dict, base_font),
        }
        if let Some(stream) = dict_get(doc, dict, b"ToUnicode").and_then(|o| o.as_stream().ok()) {
            if let Ok(bytes) = decoded_content(stream) {
                font.unicode = CMap::parse(&bytes).unicode;
            }
        }
        font
    }

    fn load_simple(&mut self, doc: &Document, dict: &Dictionary, base_font: &[u8]) {
        self.kind = FontKind::Simple;
        let standard = standard_font(base_font);
        self.glyph_names = simple_glyph_names(doc, dict, standard.as_ref());
        let descriptor = dict_dict(doc, dict, b"FontDescriptor");
        self.missing_width = descriptor
            .and_then(|descriptor| dict_number(doc, descriptor, b"MissingWidth"))
            .unwrap_or(0.0);
        if let Some(widths) = dict_array(doc, dict, b"Widths") {
            let first = dict_number(doc, dict, b"FirstChar").unwrap_or(0.0).max(0.0) as usize;
            for (offset, width) in widths.iter().enumerate() {
                if let (Some(slot), Some(width)) = (
                    self.simple_widths.get_mut(first + offset),
                    crate::objects::number(doc, width),
                ) {
                    *slot = Some(width);
                }
            }
            self.metrics_known = true;
        } else if let Some(standard) = standard {
            for code in 0..256 {
                let width = self.glyph_names[code]
                    .as_deref()
                    .and_then(|glyph| standard.width(glyph));
                self.simple_widths[code] = width;
            }
            self.metrics_known = true;
        }
        let (ascent, descent) = standard
            .map(|standard| (standard.ascent / 1000.0, standard.descent / 1000.0))
            .unwrap_or((DEFAULT_ASCENT, DEFAULT_DESCENT));
        self.set_heights(doc, descriptor, ascent, descent);
    }

    fn load_type3(&mut self, doc: &Document, dict: &Dictionary) {
        self.kind = FontKind::Type3;
        self.name = "Type 3 font".to_owned();
        self.type3_matrix = dict_array(doc, dict, b"FontMatrix")
            .and_then(|array| Matrix::from_numbers(&numbers(doc, array)))
            .unwrap_or(Matrix::scale(0.001, 0.001));
        self.glyph_names = simple_glyph_names(doc, dict, None);
        let bounds = dict_array(doc, dict, b"FontBBox")
            .map(|array| numbers(doc, array))
            .and_then(|values| match values.as_slice() {
                [x0, y0, x1, y1] => Some(Rect::from_corners(*x0, *y0, *x1, *y1)),
                _ => None,
            })
            .filter(|rect| !rect.is_empty());
        self.type3_bounds = bounds.map(|rect| rect.transform(&self.type3_matrix).bounds());
        if let Some(widths) = dict_array(doc, dict, b"Widths") {
            let first = dict_number(doc, dict, b"FirstChar").unwrap_or(0.0).max(0.0) as usize;
            for (offset, width) in widths.iter().enumerate() {
                if let (Some(slot), Some(width)) = (
                    self.simple_widths.get_mut(first + offset),
                    crate::objects::number(doc, width),
                ) {
                    *slot = Some(width);
                }
            }
            self.metrics_known = true;
        }
        if let Some(bounds) = self.type3_bounds {
            self.ascent = bounds.y1;
            self.descent = bounds.y0;
        }
    }

    fn load_composite(&mut self, doc: &Document, dict: &Dictionary) {
        self.kind = FontKind::Composite;
        self.read_composite_encoding(doc, dict);
        let descendant = dict_array(doc, dict, b"DescendantFonts")
            .and_then(|fonts| fonts.first())
            .and_then(|font| as_dict(resolve(doc, font)));
        let Some(descendant) = descendant else {
            self.metrics_known = false;
            return;
        };
        self.default_width = dict_number(doc, descendant, b"DW").unwrap_or(1000.0);
        if let Some(widths) = dict_array(doc, descendant, b"W") {
            self.cid_widths = read_cid_widths(doc, widths);
        }
        let descriptor = dict_dict(doc, descendant, b"FontDescriptor");
        self.set_heights(doc, descriptor, DEFAULT_ASCENT, DEFAULT_DESCENT);
    }

    fn read_composite_encoding(&mut self, doc: &Document, dict: &Dictionary) {
        match dict_get(doc, dict, b"Encoding") {
            Some(Object::Name(name)) => self.use_named_cmap(name),
            Some(Object::Stream(stream)) => {
                let cmap = decoded_content(stream)
                    .map(|bytes| CMap::parse(&bytes))
                    .unwrap_or_default();
                let builds_on_identity = cmap
                    .uses
                    .as_deref()
                    .is_some_and(|name| name.starts_with(b"Identity"));
                self.vertical = cmap.vertical;
                self.code_space = Some(if cmap.code_space.is_empty() {
                    CodeSpace::two_bytes()
                } else {
                    cmap.code_space.clone()
                });
                self.cid_is_code = builds_on_identity;
                self.metrics_known =
                    !cmap.cids.is_empty() || !cmap.cid_ranges.is_empty() || builds_on_identity;
                self.cid_map = Some(cmap);
            }
            _ => self.use_named_cmap(b"Identity-H"),
        }
    }

    /// predefined cmaps. only the identity ones say which cid each code
    /// draws without their published tables, which Letters does not ship;
    /// for the others the code lengths are right for unicode based cmaps
    /// but the widths are not known.
    fn use_named_cmap(&mut self, name: &[u8]) {
        self.vertical = name.ends_with(b"-V");
        self.code_space = Some(CodeSpace::two_bytes());
        if name.starts_with(b"Identity") {
            self.cid_is_code = true;
            self.metrics_known = true;
        } else {
            self.metrics_known = false;
            let text = String::from_utf8_lossy(name);
            self.code_is_unicode = text.contains("UCS2") || text.contains("UTF16");
        }
    }

    fn set_heights(
        &mut self,
        doc: &Document,
        descriptor: Option<&Dictionary>,
        ascent: f64,
        descent: f64,
    ) {
        let read = |key: &[u8]| {
            descriptor
                .and_then(|d| dict_number(doc, d, key))
                .map(|value| value / 1000.0)
        };
        let bbox_top = descriptor
            .and_then(|d| dict_array(doc, d, b"FontBBox"))
            .map(|array| numbers(doc, array))
            .and_then(|values| values.get(3).copied())
            .map(|top| top / 1000.0);
        let ascent = read(b"Ascent")
            .filter(|value| *value > 0.0)
            .or(bbox_top)
            .unwrap_or(ascent);
        let descent = read(b"Descent")
            .filter(|value| *value < 0.0)
            .unwrap_or(descent);
        self.ascent = ascent.clamp(0.3, 1.5);
        self.descent = descent.clamp(-0.6, 0.0);
    }

    /// splits a shown string into (code, byte length) pairs.
    pub fn codes(&self, bytes: &[u8]) -> Vec<(u32, usize)> {
        match &self.code_space {
            Some(space) => space.split(bytes),
            None => bytes.iter().map(|&byte| (u32::from(byte), 1)).collect(),
        }
    }

    /// how far the pen moves for a code, in text space units for a font
    /// size of 1 (before character spacing and scaling).
    pub fn advance(&self, code: u32) -> f64 {
        match self.kind {
            FontKind::Type3 => {
                self.type3_matrix
                    .apply_vector(self.simple_width(code), 0.0)
                    .x
            }
            FontKind::Simple => self.simple_width(code) / 1000.0,
            FontKind::Composite => self.cid_width(code) / 1000.0,
        }
    }

    fn simple_width(&self, code: u32) -> f64 {
        self.simple_widths
            .get(code as usize)
            .copied()
            .flatten()
            .unwrap_or(self.missing_width)
    }

    fn cid_width(&self, code: u32) -> f64 {
        let cid = if self.cid_is_code {
            Some(code)
        } else {
            self.cid_map.as_ref().and_then(|cmap| cmap.cid(code))
        };
        match cid {
            Some(cid) => self
                .cid_widths
                .get(&cid)
                .copied()
                .unwrap_or(self.default_width),
            None => self.default_width,
        }
    }

    /// the box a glyph covers, in text space units for a font size of 1,
    /// with the pen at the origin.
    pub fn glyph_box(&self, code: u32) -> Rect {
        if self.vertical {
            let half = self.advance(code).abs().max(0.5) / 2.0;
            return Rect::from_corners(-half, -1.0, half, 0.0);
        }
        if let (FontKind::Type3, Some(bounds)) = (self.kind, self.type3_bounds) {
            return bounds;
        }
        let width = self.advance(code);
        // a glyph that does not move the pen (an accent, say) still draws
        // ink, usually around the pen position
        let (x0, x1) = if width.abs() < 0.05 {
            (-0.25, 0.25)
        } else {
            (0.0, width)
        };
        Rect::from_corners(x0, self.descent, x1, self.ascent)
    }

    /// the text a code stands for, or the replacement character when the
    /// font does not say.
    pub fn text(&self, code: u32) -> String {
        if let Some(text) = self.unicode.get(&code) {
            return text.clone();
        }
        if self.kind != FontKind::Composite {
            if let Some(text) = self
                .glyph_names
                .get(code as usize)
                .cloned()
                .flatten()
                .and_then(|name| glyph_unicode(&name))
            {
                return text;
            }
        }
        if self.code_is_unicode {
            if let Some(letter) = char::from_u32(code) {
                return letter.to_string();
            }
        }
        '\u{FFFD}'.to_string()
    }

    /// word spacing (Tw) applies to the single byte code 32 only.
    pub fn is_word_space(&self, code: u32, length: usize) -> bool {
        code == 32 && length == 1
    }
}

/// the glyph name winansi encoding gives a byte, if it gives one.
pub fn standard_metrics_winansi_name(byte: u8) -> Option<&'static str> {
    let name = standard_metrics::WIN_ANSI_ENCODING[byte as usize];
    (!name.is_empty()).then_some(name)
}

/// the glyph name of each of the 256 codes of a simple font: its base
/// encoding with the /Differences applied on top.
fn simple_glyph_names(
    doc: &Document,
    dict: &Dictionary,
    standard: Option<&StandardFont>,
) -> Vec<Option<String>> {
    let builtin = standard.map(|font| font.builtin_encoding);
    let encoding = dict_get(doc, dict, b"Encoding");
    let base = match encoding {
        Some(Object::Name(name)) => named_encoding(name).or(builtin),
        Some(Object::Dictionary(encoding)) => dict_name(doc, encoding, b"BaseEncoding")
            .and_then(named_encoding)
            .or(builtin),
        _ => builtin,
    };
    let base = base.unwrap_or(&standard_metrics::STANDARD_ENCODING);
    let mut names: Vec<Option<String>> = base
        .iter()
        .map(|name| (!name.is_empty()).then(|| (*name).to_owned()))
        .collect();
    if let Some(Object::Dictionary(encoding)) = encoding {
        if let Some(differences) = dict_array(doc, encoding, b"Differences") {
            apply_differences(doc, differences, &mut names);
        }
    }
    names
}

/// /Differences is `[code /name /name ... code /name ...]`: each number
/// says where the following names start.
fn apply_differences(doc: &Document, differences: &[Object], names: &mut [Option<String>]) {
    let mut code = 0usize;
    for item in differences {
        match resolve(doc, item) {
            Object::Integer(start) => code = (*start).max(0) as usize,
            Object::Name(name) => {
                if let Some(slot) = names.get_mut(code) {
                    *slot = Some(String::from_utf8_lossy(name).into_owned());
                }
                code += 1;
            }
            _ => {}
        }
    }
}

/// /W is `[first [w w w ...] first last w ...]`.
fn read_cid_widths(doc: &Document, widths: &[Object]) -> HashMap<u32, f64> {
    let mut map = HashMap::new();
    let mut index = 0;
    while index < widths.len() {
        let Some(first) = crate::objects::number(doc, &widths[index]) else {
            index += 1;
            continue;
        };
        let first = first.max(0.0) as u32;
        match widths.get(index + 1).map(|value| resolve(doc, value)) {
            Some(Object::Array(list)) => {
                for (offset, width) in list.iter().enumerate() {
                    if let Some(width) = crate::objects::number(doc, width) {
                        map.insert(first + offset as u32, width);
                    }
                }
                index += 2;
            }
            Some(_) => {
                let last = widths
                    .get(index + 1)
                    .and_then(|v| crate::objects::number(doc, v));
                let width = widths
                    .get(index + 2)
                    .and_then(|v| crate::objects::number(doc, v));
                if let (Some(last), Some(width)) = (last, width) {
                    let last = (last.max(0.0) as u32).min(first.saturating_add(65_536));
                    for cid in first..=last {
                        map.insert(cid, width);
                    }
                }
                index += 3;
            }
            None => break,
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn standard_fonts_without_widths_use_the_built_in_metrics() {
        let doc = Document::new();
        let dict =
            dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" };
        let font = Font::load(&doc, &dict);
        assert!(font.metrics_known);
        assert!((font.advance(b'A' as u32) - 0.667).abs() < 1e-9);
        assert_eq!(font.text(b'A' as u32), "A");
    }

    #[test]
    fn widths_and_differences_come_from_the_dictionary() {
        let doc = Document::new();
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Unusual",
            "FirstChar" => 65, "Widths" => vec![500.into(), 250.into()],
            "Encoding" => dictionary! { "Differences" => vec![66.into(), Object::Name(b"eacute".to_vec())] },
        };
        let font = Font::load(&doc, &dict);
        assert!((font.advance(66) - 0.25).abs() < 1e-9);
        assert_eq!(font.text(66), "é");
        assert_eq!(font.advance(10), 0.0);
    }

    #[test]
    fn identity_composite_fonts_read_two_byte_codes_and_cid_widths() {
        let doc = Document::new();
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Noto", "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Dictionary(dictionary! {
                "Type" => "Font", "Subtype" => "CIDFontType2", "DW" => 1000,
                "W" => vec![5.into(), vec![400.into(), 450.into()].into(), 10.into(), 20.into(), 300.into()],
            })],
        };
        let font = Font::load(&doc, &dict);
        assert_eq!(font.codes(&[0, 5, 0, 6]), vec![(5, 2), (6, 2)]);
        assert!((font.advance(6) - 0.45).abs() < 1e-9);
        assert!((font.advance(15) - 0.3).abs() < 1e-9);
        assert!((font.advance(99) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn an_embedded_cmap_decides_code_lengths_and_cids() {
        let mut doc = Document::new();
        let cmap = b"begincmap 2 begincodespacerange <00> <7F> <8000> <FFFF> endcodespacerange \
                     1 begincidrange <41> <5A> 1 endcidrange 1 begincidchar <8001> 100 endcidchar endcmap";
        doc.objects.insert(
            (1, 0),
            Object::Stream(lopdf::Stream::new(Dictionary::new(), cmap.to_vec())),
        );
        let dict = dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Mixed", "Encoding" => Object::Reference((1, 0)),
            "DescendantFonts" => vec![Object::Dictionary(dictionary! {
                "Type" => "Font", "Subtype" => "CIDFontType0", "DW" => 500,
                "W" => vec![1.into(), vec![700.into()].into(), 100.into(), vec![900.into()].into()],
            })],
        };
        let font = Font::load(&doc, &dict);
        assert!(font.metrics_known);
        assert_eq!(
            font.codes(&[0x41, 0x80, 0x01, 0x42]),
            vec![(0x41, 1), (0x8001, 2), (0x42, 1)]
        );
        assert!((font.advance(0x41) - 0.7).abs() < 1e-9, "A is cid 1");
        assert!((font.advance(0x8001) - 0.9).abs() < 1e-9, "8001 is cid 100");
        assert!(
            (font.advance(0x42) - 0.5).abs() < 1e-9,
            "B is cid 2, the default width"
        );
    }
}
