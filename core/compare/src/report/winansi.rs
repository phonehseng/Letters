//! turning text into the bytes the standard pdf fonts understand.
//!
//! the report uses Helvetica, which every pdf reader has built in, with
//! the "WinAnsi" encoding: one byte per letter, covering western european
//! languages and the curly quotes, dashes and bullet the report uses. a
//! letter outside it is written as "?" rather than lost silently.

/// the bytes of `text` in the WinAnsi encoding.
pub(crate) fn encode(text: &str) -> Vec<u8> {
    text.chars().map(byte_for).collect()
}

fn byte_for(ch: char) -> u8 {
    let code = ch as u32;
    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        return code as u8;
    }
    match ch {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        // a space of any other kind is still a space
        _ if ch.is_whitespace() => b' ',
        _ => b'?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_latin_and_curly_quotes_are_kept() {
        assert_eq!(encode("Az"), b"Az");
        assert_eq!(encode("é×"), [0xe9, 0xd7]);
        assert_eq!(encode("“a”–•"), [0x93, b'a', 0x94, 0x96, 0x95]);
    }

    #[test]
    fn letters_outside_the_encoding_become_question_marks() {
        assert_eq!(encode("日"), b"?");
    }
}
