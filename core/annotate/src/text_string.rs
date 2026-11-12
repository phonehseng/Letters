//! pdf "text strings": the encoding used for comments and author names.
//!
//! a text string is either PDFDocEncoding (one byte per character, close
//! to latin-1) or UTF-16BE starting with the bytes FE FF. newer files may
//! also use UTF-8 starting with EF BB BF.

use lopdf::{Object, StringFormat};

/// encodes text the way the widest range of readers understands:
/// plain ascii stays readable in the file, anything else becomes UTF-16BE.
pub(crate) fn encode(text: &str) -> Object {
    if text
        .bytes()
        .all(|byte| (0x20..0x7f).contains(&byte) || byte == b'\n')
    {
        return Object::String(text.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// decodes a text string in any of its three encodings.
pub(crate) fn decode(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let (pairs, _) = utf16.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    bytes.iter().map(|&byte| pdf_doc_char(byte)).collect()
}

/// one PDFDocEncoding byte as a character. it matches latin-1 except
/// for a few accents below 0x20 and typographic marks in 0x80..0xa0.
fn pdf_doc_char(byte: u8) -> char {
    const LOW: [char; 8] = [
        '\u{02d8}', '\u{02c7}', '\u{02c6}', '\u{02d9}', '\u{02dd}', '\u{02db}', '\u{02da}',
        '\u{02dc}',
    ];
    const HIGH: [char; 33] = [
        '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}',
        '\u{2044}', '\u{2039}', '\u{203a}', '\u{2212}', '\u{2030}', '\u{201e}', '\u{201c}',
        '\u{201d}', '\u{2018}', '\u{2019}', '\u{201a}', '\u{2122}', '\u{fb01}', '\u{fb02}',
        '\u{0141}', '\u{0152}', '\u{0160}', '\u{0178}', '\u{017d}', '\u{0131}', '\u{0142}',
        '\u{0153}', '\u{0161}', '\u{017e}', '\u{fffd}', '\u{20ac}',
    ];
    match byte {
        0x18..=0x1f => LOW[(byte - 0x18) as usize],
        0x80..=0xa0 => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) -> String {
        match encode(text) {
            Object::String(bytes, _) => decode(&bytes),
            _ => unreachable!(),
        }
    }

    #[test]
    fn ascii_and_other_scripts_survive_a_round_trip() {
        assert_eq!(round_trip("Check this (again)"), "Check this (again)");
        assert_eq!(round_trip("café – 東京 🙂"), "café – 東京 🙂");
    }

    #[test]
    fn pdf_doc_encoding_is_read() {
        assert_eq!(decode(b"caf\xe9 \x80"), "café •");
        assert_eq!(decode(b"\xef\xbb\xbfna\xc3\xafve"), "naïve");
    }
}
