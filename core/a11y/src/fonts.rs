//! what a font tells us: how wide its characters are, whether it is
//! embedded, and whether its characters can be turned into real text.
//!
//! a screen reader does not see glyph shapes, only the characters behind
//! them. a pdf font maps byte codes to glyphs; to get characters back the
//! file needs a /ToUnicode map or an encoding whose glyph names are the
//! standard ones.

use lopdf::{Dictionary, Document, Encoding, Object};

use crate::objects::{get, get_array, get_dict, get_name, get_number, name_text, resolve};

/// how a font's codes turn into characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnicodeSource {
    /// a /ToUnicode map is present
    ToUnicodeMap,
    /// a standard encoding with standard glyph names
    StandardEncoding,
    /// nothing reliable: a screen reader would get nonsense or nothing
    Missing,
}

/// one font as used by text on a page.
pub struct FontInfo<'a> {
    pub base_font: String,
    /// codes are two bytes long (composite fonts)
    pub two_byte: bool,
    pub bold: bool,
    pub embedded: bool,
    pub unicode: UnicodeSource,
    first_char: u32,
    widths: Vec<f32>,
    cid_widths: Vec<(u32, u32, f32)>,
    missing_width: f32,
    /// glyph widths are in thousandths of the font size, except for type 3
    /// fonts, which bring their own scale
    width_scale: f32,
    encoding: Option<Encoding<'a>>,
}

/// standard glyph names a reader can turn into characters without a map.
/// this is a short list of the common ones plus the uniXXXX patterns, not
/// the whole adobe glyph list; unknown names are reported as missing,
/// which errs on the side of asking for a check.
const COMMON_GLYPH_NAMES: &[&str] = &[
    "space",
    "exclam",
    "quotedbl",
    "numbersign",
    "dollar",
    "percent",
    "ampersand",
    "quotesingle",
    "quoteright",
    "quoteleft",
    "parenleft",
    "parenright",
    "asterisk",
    "plus",
    "comma",
    "hyphen",
    "minus",
    "period",
    "slash",
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "colon",
    "semicolon",
    "less",
    "equal",
    "greater",
    "question",
    "at",
    "bracketleft",
    "backslash",
    "bracketright",
    "asciicircum",
    "underscore",
    "grave",
    "braceleft",
    "bar",
    "braceright",
    "asciitilde",
    "bullet",
    "endash",
    "emdash",
    "quotedblleft",
    "quotedblright",
    "ellipsis",
    "fi",
    "fl",
    "ff",
    "ffi",
    "ffl",
    "degree",
    "copyright",
    "registered",
    "trademark",
    "section",
    "paragraph",
    "dagger",
    "daggerdbl",
    "periodcentered",
    "multiply",
    "divide",
    "nbspace",
    "sterling",
    "yen",
    "Euro",
    "cent",
    "eacute",
    "egrave",
    "ecircumflex",
    "edieresis",
    "aacute",
    "agrave",
    "acircumflex",
    "adieresis",
    "atilde",
    "aring",
    "ccedilla",
    "iacute",
    "igrave",
    "icircumflex",
    "idieresis",
    "ntilde",
    "oacute",
    "ograve",
    "ocircumflex",
    "odieresis",
    "otilde",
    "oslash",
    "uacute",
    "ugrave",
    "ucircumflex",
    "udieresis",
    "yacute",
    "ydieresis",
    "germandbls",
    "ae",
    "oe",
    "Eacute",
    "Egrave",
    "Aacute",
    "Agrave",
    "Adieresis",
    "Odieresis",
    "Udieresis",
    "Ccedilla",
    "AE",
    "OE",
    "Oslash",
    "Aring",
    "Ntilde",
    "dotlessi",
    "exclamdown",
    "questiondown",
    "guillemotleft",
    "guillemotright",
    "quotesinglbase",
    "quotedblbase",
    "florin",
    "mu",
];

pub fn is_known_glyph_name(name: &str) -> bool {
    let is_hex = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit());
    if name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    if let Some(rest) = name.strip_prefix("uni") {
        return rest.len() >= 4 && rest.len() % 4 == 0 && is_hex(rest);
    }
    if let Some(rest) = name.strip_prefix('u') {
        if (4..=6).contains(&rest.len()) && is_hex(rest) {
            return true;
        }
    }
    // small caps and alternates like "a.sc" are still the letter "a"
    let base = name.split(['.', '_']).next().unwrap_or(name);
    if base != name && !base.is_empty() && is_known_glyph_name(base) {
        return true;
    }
    COMMON_GLYPH_NAMES.contains(&name)
}

const STANDARD_FONT_NAMES: &[&str] = &[
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

/// a name with any subset prefix ("ABCDEF+Arial") removed.
fn without_subset_prefix(name: &str) -> &str {
    match name.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.chars().all(|c| c.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => name,
    }
}

impl<'a> FontInfo<'a> {
    pub fn read(doc: &'a Document, font: &'a Dictionary) -> Self {
        let base_font = get_name(doc, font, b"BaseFont")
            .map(name_text)
            .unwrap_or_default();
        let subtype = get_name(doc, font, b"Subtype")
            .map(name_text)
            .unwrap_or_default();
        let two_byte = subtype == "Type0";
        let descendant = if two_byte {
            get_array(doc, font, b"DescendantFonts")
                .and_then(|list| list.first())
                .and_then(|first| resolve(doc, first).as_dict().ok())
        } else {
            None
        };
        let descriptor = get_dict(doc, descendant.unwrap_or(font), b"FontDescriptor");

        let mut info = FontInfo {
            bold: is_bold(doc, &base_font, descriptor),
            embedded: is_embedded(doc, &subtype, descriptor),
            unicode: unicode_source(doc, font, &subtype, &base_font, descriptor),
            first_char: get_number(doc, font, b"FirstChar").unwrap_or(0.0).max(0.0) as u32,
            widths: get_array(doc, font, b"Widths")
                .map(|list| {
                    list.iter()
                        .map(|w| resolve(doc, w).as_float().unwrap_or(0.0))
                        .collect()
                })
                .unwrap_or_default(),
            cid_widths: descendant
                .map(|d| read_cid_widths(doc, d))
                .unwrap_or_default(),
            missing_width: missing_width(doc, &base_font, descendant, descriptor),
            width_scale: type3_scale(doc, font, &subtype),
            encoding: text_encoding(doc, font),
            base_font,
            two_byte,
        };
        if info.widths.is_empty() && !info.two_byte {
            info.first_char = 0;
        }
        info
    }

    /// the character codes in a shown string.
    pub fn codes(&self, bytes: &[u8]) -> Vec<u32> {
        if self.two_byte {
            bytes
                .chunks(2)
                .map(|pair| {
                    pair.iter()
                        .fold(0u32, |code, &byte| (code << 8) | byte as u32)
                })
                .collect()
        } else {
            bytes.iter().map(|&byte| byte as u32).collect()
        }
    }

    /// how far one character moves the pen, for a font size of 1.
    pub fn advance(&self, code: u32) -> f32 {
        let width = if self.two_byte {
            self.cid_widths
                .iter()
                .find(|(first, last, _)| (*first..=*last).contains(&code))
                .map(|(_, _, width)| *width)
                .unwrap_or(self.missing_width)
        } else {
            code.checked_sub(self.first_char)
                .and_then(|index| self.widths.get(index as usize))
                .copied()
                .filter(|width| *width > 0.0)
                .unwrap_or(if code == 32 {
                    278.0
                } else {
                    self.missing_width
                })
        };
        width * self.width_scale
    }

    /// the characters behind a shown string, as well as the font allows.
    pub fn decode(&self, bytes: &[u8]) -> String {
        if let Some(encoding) = &self.encoding {
            if let Ok(text) = encoding.bytes_to_string(bytes) {
                return text;
            }
        }
        if self.two_byte {
            return String::new();
        }
        bytes
            .iter()
            .map(|&byte| {
                if byte.is_ascii_graphic() || byte == b' ' {
                    byte as char
                } else {
                    '\u{fffd}'
                }
            })
            .collect()
    }

    /// a short name for messages: the font's name without subset prefix.
    pub fn display_name(&self) -> String {
        let name = without_subset_prefix(&self.base_font);
        if name.is_empty() {
            "an unnamed font".to_owned()
        } else {
            name.to_owned()
        }
    }
}

fn is_bold(doc: &Document, base_font: &str, descriptor: Option<&Dictionary>) -> bool {
    let lower = base_font.to_ascii_lowercase();
    if ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return true;
    }
    let Some(descriptor) = descriptor else {
        return false;
    };
    let weight = get_number(doc, descriptor, b"FontWeight").unwrap_or(0.0);
    // flag bit 19 is "force bold"
    let flags = get_number(doc, descriptor, b"Flags").unwrap_or(0.0) as u32;
    weight >= 600.0 || flags & (1 << 18) != 0
}

fn is_embedded(doc: &Document, subtype: &str, descriptor: Option<&Dictionary>) -> bool {
    // a type 3 font draws its glyphs with pdf operators, so it lives in the file
    if subtype == "Type3" {
        return true;
    }
    descriptor.is_some_and(|descriptor| {
        [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
            .iter()
            .any(|key| get(doc, descriptor, key).is_some())
    })
}

fn unicode_source(
    doc: &Document,
    font: &Dictionary,
    subtype: &str,
    base_font: &str,
    descriptor: Option<&Dictionary>,
) -> UnicodeSource {
    if get(doc, font, b"ToUnicode").is_some_and(|map| map.as_stream().is_ok()) {
        return UnicodeSource::ToUnicodeMap;
    }
    let good = |ok: bool| {
        if ok {
            UnicodeSource::StandardEncoding
        } else {
            UnicodeSource::Missing
        }
    };
    match get(doc, font, b"Encoding") {
        Some(Object::Name(name)) if subtype == "Type0" => {
            // the predefined unicode cmaps (UniGB-UCS2-H and friends) name
            // characters directly; identity-h only names glyphs
            good(name.starts_with(b"Uni"))
        }
        Some(Object::Name(name)) => good(matches!(
            name.as_slice(),
            b"WinAnsiEncoding" | b"MacRomanEncoding" | b"MacExpertEncoding" | b"StandardEncoding"
        )),
        Some(Object::Dictionary(encoding)) if subtype != "Type0" => {
            let names_known = get_array(doc, encoding, b"Differences")
                .unwrap_or_default()
                .iter()
                .filter_map(|item| item.as_name().ok())
                .all(|name| is_known_glyph_name(&name_text(name)));
            good(names_known)
        }
        Some(_) => UnicodeSource::Missing,
        None if subtype == "Type0" || subtype == "Type3" => UnicodeSource::Missing,
        None => {
            let plain_name = without_subset_prefix(base_font);
            if STANDARD_FONT_NAMES.contains(&plain_name) {
                return UnicodeSource::StandardEncoding;
            }
            // flag bit 3 marks a symbolic font, whose codes mean nothing
            // outside the font itself
            let flags = descriptor
                .and_then(|d| get_number(doc, d, b"Flags"))
                .unwrap_or(0.0) as u32;
            good(flags & (1 << 2) == 0)
        }
    }
}

/// the encoding used to turn codes into text.
///
/// lopdf prefers /Encoding over /ToUnicode, but the map is the more
/// reliable of the two, so a copy of the font without its /Encoding is
/// handed over when a map exists. the map it returns is owned, so it can
/// outlive that copy.
fn text_encoding<'a>(doc: &'a Document, font: &'a Dictionary) -> Option<Encoding<'a>> {
    if font.has(b"ToUnicode") {
        let mut copy = font.clone();
        copy.remove(b"Encoding");
        copy.set("Type", Object::Name(b"Font".to_vec()));
        if let Ok(Encoding::UnicodeMapEncoding(map)) = copy.get_font_encoding(doc) {
            return Some(Encoding::UnicodeMapEncoding(map));
        }
    }
    // lopdf only decodes dictionaries marked as fonts; an unmarked one
    // falls back to reading the bytes as plain ascii
    if font.has_type(b"Font") {
        return font.get_font_encoding(doc).ok();
    }
    None
}

fn read_cid_widths(doc: &Document, descendant: &Dictionary) -> Vec<(u32, u32, f32)> {
    let mut ranges = Vec::new();
    let Some(list) = get_array(doc, descendant, b"W") else {
        return ranges;
    };
    let number = |object: &Object| resolve(doc, object).as_float().ok();
    let mut index = 0;
    // /W holds "first [w1 w2 …]" or "first last w" groups
    while index < list.len() {
        let Some(first) = number(&list[index]) else {
            break;
        };
        let first = first.max(0.0) as u32;
        match list.get(index + 1).map(|next| resolve(doc, next)) {
            Some(Object::Array(widths)) => {
                for (offset, width) in widths.iter().enumerate() {
                    let code = first + offset as u32;
                    ranges.push((code, code, number(width).unwrap_or(0.0)));
                }
                index += 2;
            }
            Some(last) => {
                let (Some(last), Some(width)) =
                    (last.as_float().ok(), list.get(index + 2).and_then(number))
                else {
                    break;
                };
                ranges.push((first, last.max(0.0) as u32, width));
                index += 3;
            }
            None => break,
        }
        // a damaged list must not make this slow
        if ranges.len() > 100_000 {
            break;
        }
    }
    ranges
}

fn missing_width(
    doc: &Document,
    base_font: &str,
    descendant: Option<&Dictionary>,
    descriptor: Option<&Dictionary>,
) -> f32 {
    if let Some(descendant) = descendant {
        return get_number(doc, descendant, b"DW").unwrap_or(1000.0);
    }
    if let Some(width) = descriptor.and_then(|d| get_number(doc, d, b"MissingWidth")) {
        if width > 0.0 {
            return width;
        }
    }
    // the standard fonts carry no widths in the file; these averages are
    // close enough to place text for checking
    if base_font.starts_with("Courier") {
        600.0
    } else {
        520.0
    }
}

fn type3_scale(doc: &Document, font: &Dictionary, subtype: &str) -> f32 {
    if subtype != "Type3" {
        return 0.001;
    }
    get_array(doc, font, b"FontMatrix")
        .and_then(|matrix| matrix.first())
        .and_then(|first| resolve(doc, first).as_float().ok())
        .filter(|scale| scale.is_finite() && *scale != 0.0)
        .unwrap_or(0.001)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_and_unicode_glyph_names_are_known() {
        for name in ["a", "Z", "space", "uni0041", "u1F600", "a.sc", "eacute"] {
            assert!(is_known_glyph_name(name), "{name}");
        }
    }

    #[test]
    fn made_up_glyph_names_are_not_known() {
        for name in ["g12", "glyph45", "cid0033", "G1", "uniXYZ"] {
            assert!(!is_known_glyph_name(name), "{name}");
        }
    }

    #[test]
    fn subset_prefixes_are_dropped_from_names() {
        assert_eq!(without_subset_prefix("ABCDEF+Arial-Bold"), "Arial-Bold");
        assert_eq!(without_subset_prefix("My+Font"), "My+Font");
    }
}
