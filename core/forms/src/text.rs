//! turning text into the single bytes the standard pdf fonts understand.
//!
//! appearance streams in this crate use winansiencoding, the windows
//! "western" code page that every pdf reader knows. it covers english and
//! most western european languages. text with other letters is refused
//! for now with a clear message instead of being drawn wrongly.

/// the characters winansiencoding puts in 0x80..=0x9f, where it differs
/// from latin-1. `None` marks the five unused codes.
const WIN_ANSI_HIGH: [Option<char>; 32] = [
    Some('€'),
    None,
    Some('‚'),
    Some('ƒ'),
    Some('„'),
    Some('…'),
    Some('†'),
    Some('‡'),
    Some('ˆ'),
    Some('‰'),
    Some('Š'),
    Some('‹'),
    Some('Œ'),
    None,
    Some('Ž'),
    None,
    None,
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('•'),
    Some('–'),
    Some('—'),
    Some('˜'),
    Some('™'),
    Some('š'),
    Some('›'),
    Some('œ'),
    None,
    Some('ž'),
    Some('Ÿ'),
];

/// pdfdocencoding's own characters in 0x80..=0xa0 (it is latin-1 elsewhere,
/// apart from a few accents in 0x18..=0x1f).
const PDF_DOC_HIGH: [Option<char>; 33] = [
    Some('•'),
    Some('†'),
    Some('‡'),
    Some('…'),
    Some('—'),
    Some('–'),
    Some('ƒ'),
    Some('⁄'),
    Some('‹'),
    Some('›'),
    Some('−'),
    Some('‰'),
    Some('„'),
    Some('\u{201C}'),
    Some('\u{201D}'),
    Some('\u{2018}'),
    Some('\u{2019}'),
    Some('‚'),
    Some('™'),
    Some('ﬁ'),
    Some('ﬂ'),
    Some('Ł'),
    Some('Œ'),
    Some('Š'),
    Some('Ÿ'),
    Some('Ž'),
    Some('ı'),
    Some('ł'),
    Some('œ'),
    Some('š'),
    Some('ž'),
    None,
    Some('€'),
];

const PDF_DOC_ACCENTS: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];

pub fn is_printable_ascii(character: char) -> bool {
    (' '..='~').contains(&character)
}

/// the winansi byte for one character, if it has one.
pub fn win_ansi_byte(character: char) -> Option<u8> {
    let code = character as u32;
    if (0x20..0x7F).contains(&code) || (0xA0..=0xFF).contains(&code) {
        return Some(code as u8);
    }
    WIN_ANSI_HIGH
        .iter()
        .position(|&entry| entry == Some(character))
        .map(|index| 0x80 + index as u8)
}

/// encodes text as winansi bytes, or returns the characters that don't fit.
pub fn encode_win_ansi(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut missing = String::new();
    for character in text.chars() {
        match win_ansi_byte(character) {
            Some(byte) => bytes.push(byte),
            None if !missing.contains(character) => missing.push(character),
            None => {}
        }
    }
    if missing.is_empty() {
        Ok(bytes)
    } else {
        Err(missing)
    }
}

pub fn decode_win_ansi_byte(byte: u8) -> char {
    match byte {
        0x80..=0x9F => WIN_ANSI_HIGH[(byte - 0x80) as usize].unwrap_or('\u{FFFD}'),
        _ => byte as char,
    }
}

pub fn decode_pdf_doc_encoding(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| match byte {
            0x18..=0x1F => PDF_DOC_ACCENTS[(byte - 0x18) as usize],
            0x80..=0xA0 => PDF_DOC_HIGH[(byte - 0x80) as usize].unwrap_or('\u{FFFD}'),
            _ => byte as char,
        })
        .collect()
}

/// writes bytes as a pdf literal string, escaping what must be escaped.
pub fn literal_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('(');
    for &byte in bytes {
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7E => out.push(byte as char),
            // everything else as an octal escape keeps the content stream ascii
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out.push(')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn western_letters_encode() {
        assert_eq!(encode_win_ansi("café €5").unwrap(), b"caf\xE9 \x805");
    }

    #[test]
    fn other_scripts_are_reported() {
        assert_eq!(encode_win_ansi("hi Ωλ Ω").unwrap_err(), "Ωλ");
    }

    #[test]
    fn literal_strings_escape_brackets_and_high_bytes() {
        assert_eq!(literal_string(b"a(b)\\\xE9"), "(a\\(b\\)\\\\\\351)");
    }

    #[test]
    fn decoding_round_trips() {
        for character in ['A', 'é', '€', '\u{201C}', 'Ÿ'] {
            let byte = win_ansi_byte(character).unwrap();
            assert_eq!(decode_win_ansi_byte(byte), character);
        }
    }
}
