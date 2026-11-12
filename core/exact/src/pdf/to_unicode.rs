//! `ToUnicode` cmaps: the text behind each char code (pdf 1.7, 9.10.3).
//!
//! only the `bfchar` and `bfrange` sections matter here; everything else in
//! a cmap (its name, code space ranges) is skipped.

use std::collections::HashMap;

/// reads the bfchar and bfrange sections of a tounicode cmap.
pub fn parse_to_unicode(data: &[u8]) -> HashMap<u32, String> {
    let mut mapping = HashMap::new();
    for block in sections(data, b"beginbfchar", b"endbfchar") {
        let tokens = tokens(block);
        for pair in tokens.chunks(2) {
            if let [Token::Hex(source), Token::Hex(target)] = pair {
                mapping.insert(code_of(source), text_of(target));
            }
        }
    }
    for block in sections(data, b"beginbfrange", b"endbfrange") {
        let tokens = tokens(block);
        let mut index = 0;
        while index + 2 < tokens.len() {
            let (Token::Hex(low), Token::Hex(high)) = (&tokens[index], &tokens[index + 1]) else {
                index += 1;
                continue;
            };
            let (start, end) = (code_of(low), code_of(high));
            match &tokens[index + 2] {
                Token::Array(items) => {
                    for (offset, item) in items.iter().enumerate() {
                        mapping.insert(start + offset as u32, text_of(item));
                    }
                }
                Token::Hex(target) => {
                    let first: Vec<char> = text_of(target).chars().collect();
                    for offset in 0..=end.saturating_sub(start).min(0xFFFF) {
                        mapping.insert(start + offset, incremented(&first, offset));
                    }
                }
            }
            index += 3;
        }
    }
    mapping
}

/// only the last char increments across a range.
fn incremented(first: &[char], offset: u32) -> String {
    let Some((last, rest)) = first.split_last() else {
        return String::new();
    };
    let mut text: String = rest.iter().collect();
    text.push(char::from_u32(*last as u32 + offset).unwrap_or('\u{FFFD}'));
    text
}

fn sections<'a>(data: &'a [u8], begin: &[u8], end: &[u8]) -> Vec<&'a [u8]> {
    let mut found = Vec::new();
    let mut rest = data;
    while let Some(start) = find(rest, begin) {
        let after = &rest[start + begin.len()..];
        let Some(stop) = find(after, end) else {
            break;
        };
        found.push(&after[..stop]);
        rest = &after[stop + end.len()..];
    }
    found
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

enum Token {
    Hex(Vec<u8>),
    Array(Vec<Vec<u8>>),
}

/// the hex strings of a section, with arrays of hex strings kept together.
fn tokens(block: &[u8]) -> Vec<Token> {
    let mut found = Vec::new();
    let mut array: Option<Vec<Vec<u8>>> = None;
    let mut index = 0;
    while index < block.len() {
        match block[index] {
            b'<' => {
                let end = block[index..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map(|p| index + p);
                let Some(end) = end else { break };
                let hex = hex_bytes(&block[index + 1..end]);
                match array.as_mut() {
                    Some(items) => items.push(hex),
                    None => found.push(Token::Hex(hex)),
                }
                index = end + 1;
            }
            b'[' => {
                array = Some(Vec::new());
                index += 1;
            }
            b']' => {
                if let Some(items) = array.take() {
                    found.push(Token::Array(items));
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    found
}

fn hex_bytes(digits: &[u8]) -> Vec<u8> {
    let mut clean: Vec<u8> = digits
        .iter()
        .copied()
        .filter(u8::is_ascii_hexdigit)
        .collect();
    if clean.len() % 2 == 1 {
        clean.push(b'0');
    }
    clean
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0))
        .collect()
}

fn code_of(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |code, &byte| code.wrapping_shl(8) | u32::from(byte))
}

/// utf-16 big endian text; a broken pair or odd byte becomes U+FFFD.
fn text_of(bytes: &[u8]) -> String {
    let mut units: Vec<u16> = bytes
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    let mut text = String::from_utf16_lossy(&units);
    if bytes.len() % 2 == 1 {
        text.push('\u{FFFD}');
    }
    units.clear();
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_ranges_and_ligatures() {
        // the prototype's case (test_prototype.py)
        let cmap = b"beginbfchar <0003> <0020> <0011> <00660069> endbfchar
                   beginbfrange <0024> <0026> <0041> <0030> <0031> [<0078> <0079>] endbfrange";
        let mapping = parse_to_unicode(cmap);
        assert_eq!(mapping[&0x03], " ");
        assert_eq!(mapping[&0x11], "fi"); // a ligature glyph maps to two chars
        assert_eq!(
            [0x24, 0x25, 0x26].map(|c| mapping[&c].clone()),
            ["A", "B", "C"]
        );
        assert_eq!(
            (mapping[&0x30].as_str(), mapping[&0x31].as_str()),
            ("x", "y")
        );
    }

    #[test]
    fn surrogate_pairs_are_one_char() {
        let mapping = parse_to_unicode(b"beginbfchar <01> <D83DDE00> endbfchar");
        assert_eq!(mapping[&1], "\u{1F600}");
    }
}
