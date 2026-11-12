//! what a font dictionary says about itself, for the checks: its name and
//! kind, whether its program is embedded, whether it lists widths, and
//! whether its text can be read out as unicode.

use lopdf::{Dictionary, Document};

use crate::objects::{get, get_array, get_dict, get_name, resolve};

#[derive(Clone, Debug, PartialEq)]
pub struct FontFacts {
    pub base_font: String,
    pub is_type3: bool,
    /// Type1, MMType1, TrueType or Type3: one byte per character
    pub is_simple: bool,
    pub embedded: bool,
    /// embedded as a TrueType program (FontFile2), whose widths can be read
    pub embedded_truetype: bool,
    pub has_widths: bool,
    /// a ToUnicode map, or an encoding whose characters are known
    pub unicode_known: bool,
}

/// encodings whose characters are known without a ToUnicode map
/// (19005-2 6.2.11.7.2).
const KNOWN_ENCODINGS: [&[u8]; 3] = [
    b"WinAnsiEncoding",
    b"MacRomanEncoding",
    b"MacExpertEncoding",
];
/// character collections whose cids map to unicode by published tables.
const KNOWN_ORDERINGS: [&[u8]; 4] = [b"GB1", b"CNS1", b"Japan1", b"Korea1"];

pub fn font_facts(doc: &Document, font: &Dictionary) -> FontFacts {
    let subtype = get_name(doc, font, b"Subtype").unwrap_or_default();
    let base_font = get_name(doc, font, b"BaseFont")
        .map(|name| String::from_utf8_lossy(&name).into_owned())
        .unwrap_or_else(|| "(unnamed)".to_owned());
    let is_type3 = subtype == b"Type3";
    let is_simple = subtype != b"Type0";
    let descriptor = if is_simple {
        get_dict(doc, font, b"FontDescriptor")
    } else {
        descendant(doc, font).and_then(|cid_font| get_dict(doc, cid_font, b"FontDescriptor"))
    };
    let has_file = |key: &[u8]| descriptor.is_some_and(|descriptor| descriptor.has(key));
    let embedded = has_file(b"FontFile") || has_file(b"FontFile2") || has_file(b"FontFile3");
    let has_widths = font.has(b"Widths") && font.has(b"FirstChar") && font.has(b"LastChar");
    FontFacts {
        base_font,
        is_type3,
        is_simple,
        embedded,
        embedded_truetype: has_file(b"FontFile2"),
        has_widths,
        unicode_known: font.has(b"ToUnicode") || known_encoding(doc, font, is_simple),
    }
}

/// a Type0 font's one descendant cid font.
pub fn descendant<'a>(doc: &'a Document, font: &'a Dictionary) -> Option<&'a Dictionary> {
    let first = get_array(doc, font, b"DescendantFonts")?.first()?;
    resolve(doc, first).as_dict().ok()
}

fn known_encoding(doc: &Document, font: &Dictionary, is_simple: bool) -> bool {
    if !is_simple {
        let ordering = descendant(doc, font)
            .and_then(|cid_font| get_dict(doc, cid_font, b"CIDSystemInfo"))
            .and_then(|info| get(doc, info, b"Ordering"))
            .and_then(|value| value.as_str().ok())
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        return KNOWN_ORDERINGS.contains(&ordering.as_slice());
    }
    match get(doc, font, b"Encoding") {
        Some(lopdf::Object::Name(name)) => KNOWN_ENCODINGS.contains(&name.as_slice()),
        Some(lopdf::Object::Dictionary(encoding)) => {
            let base_known = get_name(doc, encoding, b"BaseEncoding")
                .is_some_and(|base| KNOWN_ENCODINGS.contains(&base.as_slice()));
            base_known && differences_use_glyph_list_names(doc, encoding)
        }
        _ => false,
    }
}

/// names like g12 or glyph42 aren't in the adobe glyph list, so their
/// characters are unknown; real names (a, Aacute, uni00E9) are.
fn differences_use_glyph_list_names(doc: &Document, encoding: &Dictionary) -> bool {
    get_array(doc, encoding, b"Differences")
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_name().ok())
        .all(|name| {
            let text = String::from_utf8_lossy(name);
            let numbered = |prefix: &str| {
                text.strip_prefix(prefix).is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
                })
            };
            !(numbered("g") || numbered("G") || numbered("glyph") || numbered("cid"))
        })
}
