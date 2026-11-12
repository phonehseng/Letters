//! WinAnsiEncoding: the one-byte character set the standard fonts use.
//!
//! it is latin-1 plus typographic marks (curly quotes, dashes, the euro
//! sign and a few letters) in the codes 128 to 159. anything outside it
//! can't be drawn with the built-in fonts without embedding a font file.

/// the characters in codes 128 to 159, in order. a zero marks a code
/// the encoding leaves empty.
const CODES_128_TO_159: [u32; 32] = [
    0x20ac, 0, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017d, 0, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc,
    0x2122, 0x0161, 0x203a, 0x0153, 0, 0x017e, 0x0178,
];

/// the byte for a character, or none if the encoding has no place for it.
pub(crate) fn encode_char(character: char) -> Option<u8> {
    let code = character as u32;
    match code {
        0x20..=0x7e | 0xa0..=0xff => Some(code as u8),
        0 => None,
        _ => CODES_128_TO_159
            .iter()
            .position(|&candidate| candidate == code)
            .map(|index| 128 + index as u8),
    }
}

/// encodes text, or returns every character that can't be encoded
/// (each listed once, in the order they first appear).
pub(crate) fn encode(text: &str) -> Result<Vec<u8>, Vec<char>> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut missing = Vec::new();
    for character in text.chars() {
        match encode_char(character) {
            Some(byte) => bytes.push(byte),
            None if !missing.contains(&character) => missing.push(character),
            None => {}
        }
    }
    if missing.is_empty() {
        Ok(bytes)
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_text_and_typographic_marks_encode() {
        assert_eq!(
            encode("Café – “ok” €"),
            Ok(b"Caf\xe9 \x96 \x93ok\x94 \x80".to_vec())
        );
    }

    #[test]
    fn other_scripts_are_reported_once_each() {
        assert_eq!(encode("a東京東b"), Err(vec!['東', '京']));
        assert_eq!(encode_char('\u{7f}'), None);
        assert_eq!(encode_char('\t'), None);
    }
}
