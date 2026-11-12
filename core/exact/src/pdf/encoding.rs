//! simple font encodings and glyph names (pdf 1.7, 9.6.6 and annex d).

use std::collections::HashMap;

use super::tables::{GLYPH_LIST, MAC_ROMAN_ENCODING, STANDARD_ENCODING, WIN_ANSI_ENCODING};

/// code to glyph name for a named base encoding. anything that is not
/// winansi or macroman reads as the standard encoding.
pub fn base_encoding(name: Option<&[u8]>) -> HashMap<u32, String> {
    let table = match name {
        Some(b"WinAnsiEncoding") => &WIN_ANSI_ENCODING,
        Some(b"MacRomanEncoding") => &MAC_ROMAN_ENCODING,
        _ => &STANDARD_ENCODING,
    };
    table
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(code, name)| (code as u32, name.to_string()))
        .collect()
}

/// the unicode text of a glyph name, the way the adobe glyph list
/// specification says (and fonttools' `toUnicode` does): drop everything
/// after the first period, split ligatures at underscores, then look each
/// part up in the glyph list or read it as `uniXXXX` or `uXXXX[XX]`.
pub fn glyph_name_to_text(name: &str) -> String {
    let base = name.split('.').next().unwrap_or("");
    base.split('_').map(component_text).collect()
}

fn component_text(component: &str) -> String {
    if let Ok(index) = GLYPH_LIST.binary_search_by(|(name, _)| (*name).cmp(component)) {
        return GLYPH_LIST[index].1.to_string();
    }
    if let Some(text) = uni_text(component) {
        return text;
    }
    u_text(component).unwrap_or_default()
}

fn is_upper_hex(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}

/// `uni0041` or `uni00410042`: groups of four upper case hex digits, no surrogates.
fn uni_text(component: &str) -> Option<String> {
    let digits = component.strip_prefix("uni")?;
    if digits.len() % 4 != 0 || !is_upper_hex(digits) {
        return None;
    }
    digits
        .as_bytes()
        .chunks(4)
        .map(|group| {
            let value = u32::from_str_radix(std::str::from_utf8(group).ok()?, 16).ok()?;
            if (0xD800..=0xDFFF).contains(&value) {
                None
            } else {
                char::from_u32(value)
            }
        })
        .collect()
}

/// `u1F600`: four to six upper case hex digits.
fn u_text(component: &str) -> Option<String> {
    let digits = component.strip_prefix('u')?;
    if !(4..=6).contains(&digits.len()) || !is_upper_hex(digits) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let valid = value <= 0xD7FF || (0xE000..=0x10FFFF).contains(&value);
    if valid {
        char::from_u32(value).map(String::from)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names_become_text() {
        assert_eq!(glyph_name_to_text("A"), "A");
        assert_eq!(glyph_name_to_text("f_i"), "fi");
        assert_eq!(glyph_name_to_text("uni00410042"), "AB");
        assert_eq!(glyph_name_to_text("a.sc"), "a");
        assert_eq!(glyph_name_to_text("u1F600"), "\u{1F600}");
        assert_eq!(glyph_name_to_text("uni004a"), ""); // lower case hex is not a uni name
        assert_eq!(glyph_name_to_text("quoteright"), "\u{2019}");
        assert_eq!(glyph_name_to_text("g123"), "");
    }

    #[test]
    fn base_encodings() {
        let win = base_encoding(Some(b"WinAnsiEncoding"));
        assert_eq!(win[&0x41], "A");
        assert_eq!(win[&0x92], "quoteright");
        assert!(!win.contains_key(&0x81));
        let standard = base_encoding(None);
        assert_eq!(standard[&0x27], "quoteright");
        assert_eq!(base_encoding(Some(b"MacRomanEncoding"))[&0x80], "Adieresis");
    }
}
