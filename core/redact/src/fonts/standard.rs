//! the 14 fonts every pdf reader has built in, the standard encodings,
//! and glyph names as unicode.

use super::glyph_names::GLYPH_UNICODE;
use super::standard_metrics::*;

/// metrics of one of the 14 standard fonts.
#[derive(Clone, Copy, Debug)]
pub struct StandardFont {
    /// none for the courier family, where every glyph is 600 wide
    widths: Option<WidthTable>,
    pub ascent: f64,
    pub descent: f64,
    /// symbol and zapf dingbats have their own encoding; the others use
    /// standard encoding unless the pdf says otherwise
    pub builtin_encoding: &'static EncodingTable,
}

impl StandardFont {
    /// the width of a glyph in thousandths of the font size.
    pub fn width(&self, glyph: &str) -> Option<f64> {
        match self.widths {
            None => Some(600.0),
            Some(table) => table
                .binary_search_by(|(name, _)| (*name).cmp(glyph))
                .ok()
                .map(|index| f64::from(table[index].1)),
        }
    }
}

/// finds a standard font from a /BaseFont name, including the names
/// office programs use for the same fonts (Arial is Helvetica, and so on).
pub fn standard_font(base_font: &[u8]) -> Option<StandardFont> {
    let name = String::from_utf8_lossy(strip_subset_prefix(base_font)).to_ascii_lowercase();
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let bold = name.contains("bold");
    let italic = name.contains("italic") || name.contains("oblique");
    let latin = |widths: Option<WidthTable>, ascent: f64, descent: f64| StandardFont {
        widths,
        ascent,
        descent,
        builtin_encoding: &STANDARD_ENCODING,
    };
    if name.starts_with("courier") {
        return Some(latin(None, 629.0, -157.0));
    }
    if name.starts_with("helvetica") || name.starts_with("arial") {
        let widths = if bold { HELVETICA_BOLD } else { HELVETICA };
        return Some(latin(Some(widths), 718.0, -207.0));
    }
    if name.starts_with("times") {
        let widths = match (bold, italic) {
            (true, true) => TIMES_BOLD_ITALIC,
            (true, false) => TIMES_BOLD,
            (false, true) => TIMES_ITALIC,
            (false, false) => TIMES_ROMAN,
        };
        return Some(latin(Some(widths), 683.0, -217.0));
    }
    if name == "symbol" {
        return Some(StandardFont {
            widths: Some(SYMBOL),
            ascent: 1010.0,
            descent: -293.0,
            builtin_encoding: &SYMBOL_ENCODING,
        });
    }
    if name == "zapfdingbats" {
        return Some(StandardFont {
            widths: Some(ZAPF_DINGBATS),
            ascent: 820.0,
            descent: -143.0,
            builtin_encoding: &ZAPF_DINGBATS_ENCODING,
        });
    }
    None
}

/// embedded subsets are named like ABCDEF+Helvetica.
pub fn strip_subset_prefix(name: &[u8]) -> &[u8] {
    if name.len() > 7 && name[6] == b'+' && name[..6].iter().all(u8::is_ascii_uppercase) {
        &name[7..]
    } else {
        name
    }
}

/// a named base encoding.
pub fn named_encoding(name: &[u8]) -> Option<&'static EncodingTable> {
    match name {
        b"StandardEncoding" => Some(&STANDARD_ENCODING),
        b"WinAnsiEncoding" => Some(&WIN_ANSI_ENCODING),
        b"MacRomanEncoding" => Some(&MAC_ROMAN_ENCODING),
        _ => None,
    }
}

/// the unicode text of a glyph name: from the glyph list, or spelled out
/// in the name itself (uni0041, u1F600, or a single letter like "A").
pub fn glyph_unicode(name: &str) -> Option<String> {
    let base = name.split('.').next().unwrap_or(name);
    if let Ok(index) = GLYPH_UNICODE.binary_search_by(|(glyph, _)| (*glyph).cmp(base)) {
        return char::from_u32(GLYPH_UNICODE[index].1).map(String::from);
    }
    if let Some(hex) = base.strip_prefix("uni") {
        return hex_code_points(hex, 4);
    }
    if let Some(hex) = base.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            return hex_code_points(hex, hex.len());
        }
    }
    let mut letters = base.chars();
    match (letters.next(), letters.next()) {
        (Some(letter), None) => Some(letter.to_string()),
        _ => None,
    }
}

fn hex_code_points(hex: &str, group: usize) -> Option<String> {
    if hex.is_empty()
        || !hex.len().is_multiple_of(group)
        || !hex.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    (0..hex.len())
        .step_by(group)
        .map(|start| {
            u32::from_str_radix(&hex[start..start + group], 16)
                .ok()
                .and_then(char::from_u32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_standard_fonts_and_their_widths() {
        let helvetica = standard_font(b"Helvetica").unwrap();
        assert_eq!(helvetica.width("space"), Some(278.0));
        assert_eq!(helvetica.width("A"), Some(667.0));
        let arial_bold = standard_font(b"ABCDEF+Arial,Bold").unwrap();
        assert_eq!(arial_bold.width("A"), Some(722.0));
        assert_eq!(
            standard_font(b"Courier-Oblique").unwrap().width("W"),
            Some(600.0)
        );
        assert!(standard_font(b"Calibri").is_none());
    }

    #[test]
    fn standard_encodings_name_the_right_glyphs() {
        assert_eq!(WIN_ANSI_ENCODING[65], "A");
        assert_eq!(WIN_ANSI_ENCODING[0x80], "Euro");
        assert_eq!(STANDARD_ENCODING[0x27], "quoteright");
    }

    #[test]
    fn glyph_names_become_text() {
        assert_eq!(glyph_unicode("eacute").as_deref(), Some("é"));
        assert_eq!(glyph_unicode("uni0041").as_deref(), Some("A"));
        assert_eq!(glyph_unicode("u1F600").as_deref(), Some("\u{1F600}"));
        assert_eq!(glyph_unicode("a.sc").as_deref(), Some("a"));
        assert_eq!(glyph_unicode("g123"), None);
    }
}
