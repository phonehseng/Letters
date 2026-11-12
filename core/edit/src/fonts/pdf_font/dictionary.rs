//! reading the parts of a font dictionary: descendant fonts, /W widths,
//! /CIDToGIDMap, the glyph names of simple fonts, the descriptor's
//! /CharSet, and a guess at the style from the name and flags.

use super::*;

pub(super) fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

pub(super) fn descendant<'a>(doc: &'a Document, font: &'a Dictionary) -> Option<&'a Dictionary> {
    let descendants = dict_array(doc, font, b"DescendantFonts")?;
    resolve(doc, descendants.first()?).as_dict().ok()
}

/// reads a /W array: "c [w1 w2 ...]" or "first last w".
pub(super) fn read_cid_widths(doc: &Document, list: &[Object]) -> Vec<(u32, f32)> {
    let mut widths = Vec::new();
    let mut index = 0;
    while index < list.len() {
        let Some(first) = crate::pdf_values::number(doc, &list[index]) else {
            break;
        };
        match list.get(index + 1).map(|item| resolve(doc, item)) {
            Some(Object::Array(values)) => {
                for (offset, value) in values.iter().enumerate() {
                    if let Some(width) = crate::pdf_values::number(doc, value) {
                        widths.push((first as u32 + offset as u32, width));
                    }
                }
                index += 2;
            }
            Some(last) => {
                let (Some(last), Some(width)) = (
                    crate::pdf_values::number(doc, last),
                    list.get(index + 2)
                        .and_then(|width| crate::pdf_values::number(doc, width)),
                ) else {
                    break;
                };
                for cid in first as u32..=(last as u32).min(first as u32 + 65_535) {
                    widths.push((cid, width));
                }
                index += 3;
            }
            None => break,
        }
    }
    widths
}

/// a /CIDToGIDMap stream as a table; none means /Identity (or missing,
/// which readers also treat as identity for embedded TrueType).
pub(super) fn cid_to_gid_map(doc: &Document, descendant: &Dictionary) -> Option<HashMap<u16, u16>> {
    let map = descendant.get(b"CIDToGIDMap").ok()?;
    let bytes = stream_bytes(doc, map)?;
    Some(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .map(|(cid, glyph)| (cid as u16, u16::from_be_bytes(*glyph)))
            .collect(),
    )
}

/// the glyph name for every code of a simple font: base encoding first,
/// then /Differences on top.
pub(super) fn simple_glyph_names(
    doc: &Document,
    font: &Dictionary,
    kind: FontKind,
    symbolic: bool,
) -> Vec<Option<String>> {
    let encoding = font
        .get(b"Encoding")
        .ok()
        .map(|encoding| resolve(doc, encoding));
    let named = encoding
        .and_then(|encoding| encoding.as_name().ok())
        .and_then(BaseEncoding::from_name);
    let dictionary = encoding.and_then(|encoding| encoding.as_dict().ok());
    let base = named
        .or_else(|| {
            dictionary
                .and_then(|d| dict_name(doc, d, b"BaseEncoding"))
                .and_then(BaseEncoding::from_name)
        })
        .or(match (kind, symbolic) {
            // symbolic fonts have their own built-in encoding, unknown here
            (_, true) => None,
            (FontKind::TrueType, false) => Some(BaseEncoding::WinAnsi),
            _ => Some(BaseEncoding::Standard),
        });
    let mut names: Vec<Option<String>> = (0..=255u8)
        .map(|code| {
            base.and_then(|base| base.glyph_name(code))
                .map(str::to_owned)
        })
        .collect();
    if let Some(differences) = dictionary.and_then(|d| dict_array(doc, d, b"Differences")) {
        let mut code = 0usize;
        for item in differences {
            match resolve(doc, item) {
                Object::Integer(start) => code = (*start).clamp(0, 255) as usize,
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
    names
}

/// glyph names a Type1 descriptor's /CharSet lists ("/a/b/eacute").
pub(super) fn charset_names(doc: &Document, descriptor: &Dictionary) -> Option<HashSet<String>> {
    let text = resolve(doc, descriptor.get(b"CharSet").ok()?)
        .as_str()
        .ok()?;
    let text = String::from_utf8_lossy(text);
    Some(
        text.split('/')
            .filter(|name| !name.is_empty())
            .map(|name| name.trim().to_owned())
            .collect(),
    )
}

/// guesses serif, bold and italic from the font's name, flags and weight.
pub(super) fn guess_style(name: &str, flags: u32, weight: Option<f32>) -> (FontFamily, bool, bool) {
    const FIXED_PITCH: u32 = 1;
    const SERIF: u32 = 2;
    const ITALIC: u32 = 64;
    const FORCE_BOLD: u32 = 1 << 18;
    let lower = name.to_ascii_lowercase();
    let family = if flags & FIXED_PITCH != 0
        || ["courier", "mono", "consol", "typewriter"]
            .iter()
            .any(|word| lower.contains(word))
    {
        FontFamily::Monospace
    } else if [
        "sans",
        "arial",
        "helvetica",
        "verdana",
        "calibri",
        "segoe",
        "tahoma",
    ]
    .iter()
    .any(|word| lower.contains(word))
    {
        FontFamily::SansSerif
    } else if flags & SERIF != 0
        || [
            "times", "serif", "roman", "georgia", "garamond", "cambria", "minion",
        ]
        .iter()
        .any(|word| lower.contains(word))
    {
        FontFamily::Serif
    } else {
        FontFamily::SansSerif
    };
    let bold = flags & FORCE_BOLD != 0
        || weight.is_some_and(|weight| weight >= 600.0)
        || ["bold", "black", "heavy", "semibold", "demi"]
            .iter()
            .any(|word| lower.contains(word));
    let italic = flags & ITALIC != 0 || lower.contains("italic") || lower.contains("oblique");
    (family, bold, italic)
}
