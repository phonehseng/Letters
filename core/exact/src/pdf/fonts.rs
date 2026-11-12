//! pdf font dictionaries, read once into one plain description per font.
//!
//! everything a later stage needs is resolved here: how many bytes a char
//! code takes, each code's width in 1/1000 em, its unicode text and, for
//! simple fonts, its glyph name. the embedded font program is kept as bytes.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Dictionary, Document, Object};

use super::encoding::{base_encoding, glyph_name_to_text};
use super::objects::{
    get, get_array, get_dict, get_name, get_number, get_stream, number, numbers, resolve,
    stream_bytes, text_string,
};
use super::tables::STANDARD_14_WIDTHS;
use super::to_unicode::parse_to_unicode;

pub const STANDARD_14: [&str; 14] = [
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];
const SYMBOLIC_FLAG: i64 = 1 << 2;

/// the kind of embedded font program (recipe 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgramKind {
    /// `FontFile2`
    TrueType,
    /// `FontFile3/OpenType`: an sfnt that may hold truetype or cff outlines
    OpenType,
    /// `FontFile3/Type1C` or `/CIDFontType0C`: a bare cff
    Cff,
    /// `FontFile`: a type 1 program with an eexec section
    Type1,
    /// the glyph procedures of a type 3 font
    Type3,
}

impl ProgramKind {
    pub fn label(&self) -> &'static str {
        match self {
            ProgramKind::TrueType => "truetype",
            ProgramKind::OpenType => "opentype",
            ProgramKind::Cff => "cff",
            ProgramKind::Type1 => "type1",
            ProgramKind::Type3 => "type3",
        }
    }
}

/// one pdf font, resolved.
#[derive(Clone, Debug)]
pub struct PdfFont {
    /// object number and generation, or a name for fonts written inline
    pub key: String,
    /// the base font name without a subset prefix such as `ABCDEF+`
    pub base_name: String,
    pub subtype: String,
    /// 1 for simple fonts, 2 for (identity) cid fonts
    pub code_bytes: usize,
    pub widths: HashMap<u32, f64>,
    pub default_width: f64,
    pub to_unicode: HashMap<u32, String>,
    /// simple fonts only: code to glyph name from the encoding
    pub glyph_names: HashMap<u32, String>,
    pub program: Option<Vec<u8>>,
    pub program_kind: Option<ProgramKind>,
    /// cid fonts: the CIDToGIDMap stream, `None` for identity
    pub cid_to_gid: Option<Vec<u8>>,
    pub symbolic: bool,
    pub ascent: f64,
    pub descent: f64,
    /// the descriptor's flags say serif (bit 2), for picking a stand-in
    pub serif: bool,
    pub problems: Vec<String>,
    /// type 3 only: code to glyph procedure
    pub type3_procs: BTreeMap<u32, Vec<u8>>,
    pub font_matrix: [f64; 6],
}

impl PdfFont {
    pub fn embedded(&self) -> bool {
        self.program.is_some()
    }

    pub fn is_cid(&self) -> bool {
        self.subtype == "Type0"
    }

    pub fn width(&self, code: u32) -> f64 {
        self.widths
            .get(&code)
            .copied()
            .unwrap_or(self.default_width)
    }

    /// unicode text for a code: tounicode first, then the encoding's glyph name.
    pub fn text(&self, code: u32) -> String {
        if let Some(text) = self.to_unicode.get(&code) {
            return text.clone();
        }
        self.glyph_names
            .get(&code)
            .map(|name| glyph_name_to_text(name))
            .unwrap_or_default()
    }

    pub fn split_codes(&self, data: &[u8]) -> Vec<u32> {
        if self.code_bytes == 2 {
            data.as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u32::from(pair[0]) << 8 | u32::from(pair[1]))
                .collect()
        } else {
            data.iter().map(|&b| u32::from(b)).collect()
        }
    }
}

/// splits `ABCDEF+Lato-Bold` into the name and the subset tag.
pub fn split_subset_prefix(base_font: &str) -> (String, String) {
    let bytes = base_font.as_bytes();
    if bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(u8::is_ascii_uppercase) {
        (base_font[7..].to_string(), base_font[..6].to_string())
    } else {
        (base_font.to_string(), String::new())
    }
}

fn name_text(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

/// reads one font dictionary. `key` identifies it (its object id).
pub fn read_font(document: &Document, font: &Dictionary, key: String) -> PdfFont {
    let subtype = get_name(document, font, b"Subtype")
        .map(name_text)
        .unwrap_or_default();
    let base_font = get_name(document, font, b"BaseFont").map(name_text);
    let (base_name, _) = split_subset_prefix(base_font.as_deref().unwrap_or("unnamed"));
    let to_unicode = get_stream(document, font, b"ToUnicode")
        .and_then(|stream| stream_bytes(stream).ok())
        .map(|bytes| parse_to_unicode(&bytes))
        .unwrap_or_default();
    if subtype == "Type0" {
        return read_cid_font(document, font, key, base_name, to_unicode);
    }
    read_simple_font(
        document,
        font,
        key,
        subtype,
        base_name,
        base_font.is_some(),
        to_unicode,
    )
}

fn read_cid_font(
    document: &Document,
    font: &Dictionary,
    key: String,
    base_name: String,
    to_unicode: HashMap<u32, String>,
) -> PdfFont {
    let mut problems = Vec::new();
    let empty = Dictionary::new();
    let cid_font = get_array(document, font, b"DescendantFonts")
        .and_then(|fonts| fonts.first())
        .and_then(|first| match resolve(document, first) {
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .unwrap_or(&empty);
    let descriptor = get_dict(document, cid_font, b"FontDescriptor");
    let encoding = get_name(document, font, b"Encoding")
        .map(name_text)
        .unwrap_or_default();
    if !encoding.starts_with("Identity") {
        problems.push(format!("cmap {encoding} read as 2-byte identity"));
    }
    let cid_to_gid =
        get_stream(document, cid_font, b"CIDToGIDMap").and_then(|s| stream_bytes(s).ok());
    let (program, program_kind) = font_program(document, descriptor);
    let (ascent, descent) = vertical_metrics(document, descriptor);
    PdfFont {
        key,
        base_name,
        subtype: "Type0".into(),
        code_bytes: 2,
        widths: cid_widths(document, cid_font),
        default_width: get_number(document, cid_font, b"DW").unwrap_or(1000.0),
        to_unicode,
        glyph_names: HashMap::new(),
        program,
        program_kind,
        cid_to_gid,
        symbolic: true,
        ascent,
        descent,
        serif: is_serif(document, descriptor),
        problems,
        type3_procs: BTreeMap::new(),
        font_matrix: [0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
    }
}

fn read_simple_font(
    document: &Document,
    font: &Dictionary,
    key: String,
    subtype: String,
    mut base_name: String,
    has_base_font: bool,
    to_unicode: HashMap<u32, String>,
) -> PdfFont {
    let mut problems = Vec::new();
    let descriptor = get_dict(document, font, b"FontDescriptor");
    let (mut program, mut program_kind) = font_program(document, descriptor);
    let glyph_names = simple_encoding(document, font);
    let mut type3_procs = BTreeMap::new();
    let mut font_matrix = [0.001, 0.0, 0.0, 0.001, 0.0, 0.0];
    let mut widths = simple_widths(document, font);
    if subtype == "Type3" {
        // the glyph procedures are the font program: kept as bytes, rebuilt as outlines later
        if let Some(procs) = get_dict(document, font, b"CharProcs") {
            for (code, glyph_name) in &glyph_names {
                if let Some(stream) = get_stream(document, procs, glyph_name.as_bytes()) {
                    if let Ok(bytes) = stream_bytes(stream) {
                        type3_procs.insert(*code, bytes);
                    }
                }
            }
        }
        if let Some(matrix) = get_array(document, font, b"FontMatrix") {
            let values = numbers(document, matrix);
            if values.len() == 6 {
                font_matrix.copy_from_slice(&values);
            }
        }
        program = Some(Vec::new());
        program_kind = Some(ProgramKind::Type3);
        let resources = get_dict(document, font, b"Resources");
        if resources.is_some_and(|r| r.has(b"XObject")) {
            problems.push("type3 glyphs use images or forms, which are not rebuilt".into());
        }
        // type 3 widths are in glyph space; everything downstream works in 1/1000 em
        for width in widths.values_mut() {
            *width *= font_matrix[0] * 1000.0;
        }
        if !has_base_font {
            // browsers leave type 3 fonts unnamed; the descriptor's family keeps one face's subsets together
            let family = descriptor
                .and_then(|d| match get(document, d, b"FontFamily") {
                    Some(Object::String(bytes, _)) => Some(text_string(bytes)),
                    _ => None,
                })
                .unwrap_or_default()
                .replace(' ', "");
            let family = if family.is_empty() {
                "Type3".to_string()
            } else {
                family
            };
            let number = key.split(' ').next().unwrap_or("0");
            base_name = format!("{family}-type3-{number}");
        }
    }
    if widths.is_empty() && program.is_none() {
        if let Some(standard) = standard_14_widths(&base_name) {
            widths = standard;
        }
    }
    let flags = descriptor
        .and_then(|d| get_number(document, d, b"Flags"))
        .unwrap_or(0.0) as i64;
    let (ascent, descent) = vertical_metrics(document, descriptor);
    PdfFont {
        key,
        base_name,
        subtype,
        code_bytes: 1,
        widths,
        default_width: descriptor
            .and_then(|d| get_number(document, d, b"MissingWidth"))
            .unwrap_or(0.0),
        to_unicode,
        glyph_names,
        program,
        program_kind,
        cid_to_gid: None,
        symbolic: flags & SYMBOLIC_FLAG != 0,
        ascent,
        descent,
        serif: flags & 2 != 0,
        problems,
        type3_procs,
        font_matrix,
    }
}

fn is_serif(document: &Document, descriptor: Option<&Dictionary>) -> bool {
    let flags = descriptor
        .and_then(|d| get_number(document, d, b"Flags"))
        .unwrap_or(0.0) as i64;
    flags & 2 != 0
}

fn vertical_metrics(document: &Document, descriptor: Option<&Dictionary>) -> (f64, f64) {
    match descriptor {
        None => (750.0, -250.0),
        Some(d) => (
            get_number(document, d, b"Ascent").unwrap_or(750.0),
            get_number(document, d, b"Descent").unwrap_or(-250.0),
        ),
    }
}

/// the encoding of a simple font: no entry is the standard encoding, a name
/// is a base encoding, a dictionary is a base encoding plus differences.
fn simple_encoding(document: &Document, font: &Dictionary) -> HashMap<u32, String> {
    match get(document, font, b"Encoding") {
        None => base_encoding(None),
        Some(Object::Name(name)) => base_encoding(Some(name)),
        Some(Object::Dictionary(encoding)) => {
            let mut names = base_encoding(get_name(document, encoding, b"BaseEncoding"));
            let mut code = 0u32;
            for item in get_array(document, encoding, b"Differences")
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                match resolve(document, item) {
                    Object::Name(name) => {
                        names.insert(code, name_text(name));
                        code += 1;
                    }
                    other => {
                        if let Some(value) = number(other) {
                            code = value as u32;
                        }
                    }
                }
            }
            names
        }
        Some(_) => base_encoding(None),
    }
}

fn simple_widths(document: &Document, font: &Dictionary) -> HashMap<u32, f64> {
    let first = get_number(document, font, b"FirstChar").unwrap_or(0.0) as u32;
    get_array(document, font, b"Widths")
        .map(|widths| {
            widths
                .iter()
                .enumerate()
                .filter_map(|(i, w)| number(resolve(document, w)).map(|w| (first + i as u32, w)))
                .collect()
        })
        .unwrap_or_default()
}

fn cid_widths(document: &Document, cid_font: &Dictionary) -> HashMap<u32, f64> {
    let mut widths = HashMap::new();
    let Some(array) = get_array(document, cid_font, b"W") else {
        return widths;
    };
    let items: Vec<&Object> = array.iter().map(|item| resolve(document, item)).collect();
    let mut i = 0;
    while i + 1 < items.len() {
        let Some(first) = number(items[i]) else {
            i += 1;
            continue;
        };
        let first = first as u32;
        if let Object::Array(list) = items[i + 1] {
            for (offset, width) in numbers(document, list).into_iter().enumerate() {
                widths.insert(first + offset as u32, width);
            }
            i += 2;
        } else if i + 2 < items.len() {
            let last = number(items[i + 1]).unwrap_or(first as f64) as u32;
            let width = number(items[i + 2]).unwrap_or(0.0);
            for cid in first..=last.min(first + 0xFFFF) {
                widths.insert(cid, width);
            }
            i += 3;
        } else {
            break;
        }
    }
    widths
}

fn font_program(
    document: &Document,
    descriptor: Option<&Dictionary>,
) -> (Option<Vec<u8>>, Option<ProgramKind>) {
    let Some(descriptor) = descriptor else {
        return (None, None);
    };
    if let Some(stream) = get_stream(document, descriptor, b"FontFile2") {
        return (stream_bytes(stream).ok(), Some(ProgramKind::TrueType));
    }
    if let Some(stream) = get_stream(document, descriptor, b"FontFile3") {
        let kind = match get_name(document, &stream.dict, b"Subtype") {
            Some(b"OpenType") => ProgramKind::OpenType,
            _ => ProgramKind::Cff,
        };
        return (stream_bytes(stream).ok(), Some(kind));
    }
    if let Some(stream) = get_stream(document, descriptor, b"FontFile") {
        return (stream_bytes(stream).ok(), Some(ProgramKind::Type1));
    }
    (None, None)
}

/// widths of a standard font the pdf did not list, from adobe's metrics,
/// for the winansi codes (as the prototype reads them).
fn standard_14_widths(base: &str) -> Option<HashMap<u32, f64>> {
    let (_, table) = STANDARD_14_WIDTHS.iter().find(|(name, _)| *name == base)?;
    Some(
        table
            .iter()
            .enumerate()
            .filter(|(_, width)| **width != 0)
            .map(|(code, width)| (code as u32, f64::from(*width)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subset_prefix() {
        assert_eq!(
            split_subset_prefix("ABCDEF+Lato-Bold"),
            ("Lato-Bold".to_string(), "ABCDEF".to_string())
        );
        assert_eq!(
            split_subset_prefix("Helvetica"),
            ("Helvetica".to_string(), String::new())
        );
    }

    #[test]
    fn standard_widths_are_known() {
        let widths = standard_14_widths("Helvetica").unwrap();
        assert_eq!(widths[&32], 278.0);
        assert_eq!(widths[&0x41], 667.0);
        assert!(standard_14_widths("Lato").is_none());
    }
}
