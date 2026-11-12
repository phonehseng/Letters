//! the windows "ansi" character set that pdf's built-in fonts understand.
//!
//! every pdf reader knows helvetica without the font being stored in the
//! file, but only for the 218 characters of this set. it covers english,
//! spanish, french, german and the other western european languages, so
//! most recognised text can use it; anything else goes through the
//! glyphless font (see `fonts.rs`).

/// the characters at 0x80 to 0x9f, where windows differs from latin-1.
/// none marks the five unused codes.
const HIGH_CONTROLS: [Option<char>; 32] = [
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
    Some('‘'),
    Some('’'),
    Some('“'),
    Some('”'),
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

/// the code of one character, or none when the set doesn't have it.
pub fn encode_char(letter: char) -> Option<u8> {
    let code = letter as u32;
    match code {
        0x20..=0x7E | 0xA0..=0xFF => Some(code as u8),
        _ => HIGH_CONTROLS
            .iter()
            .position(|&known| known == Some(letter))
            .map(|index| 0x80 + index as u8),
    }
}

/// the codes of a whole word, or none if any character is missing.
pub fn encode(text: &str) -> Option<Vec<u8>> {
    text.chars().map(encode_char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn western_european_letters_are_in_the_set() {
        assert_eq!(encode("Año"), Some(vec![b'A', 0xF1, b'o']));
        assert_eq!(
            encode("Straße"),
            Some(vec![b'S', b't', b'r', b'a', 0xDF, b'e'])
        );
        assert_eq!(
            encode("œuvre €5"),
            Some(vec![0x9C, b'u', b'v', b'r', b'e', b' ', 0x80, b'5'])
        );
        assert_eq!(
            encode("“quoted”"),
            Some([vec![0x93], b"quoted".to_vec(), vec![0x94]].concat())
        );
    }

    #[test]
    fn other_scripts_are_not() {
        assert_eq!(encode("łódź"), None);
        assert_eq!(encode("Ωμέγα"), None);
        assert_eq!(encode_char('\u{81}'), None);
    }
}
