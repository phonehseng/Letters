//! reading a font's /ToUnicode map, which says which character each code
//! in the font stands for. only the parts pdf writers use for this are
//! read: bfchar (one code) and bfrange (a run of codes) entries.

use std::collections::HashMap;

/// character codes and the text they stand for.
pub type CodeMap = HashMap<u32, String>;

/// longest run of codes one bfrange entry may cover, so a broken map
/// can't make the reader fill memory.
const MAX_RANGE: u32 = 65_536;

pub fn parse_to_unicode(bytes: &[u8]) -> CodeMap {
    let text = String::from_utf8_lossy(bytes);
    let tokens = tokenize(&text);
    let mut map = CodeMap::new();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "beginbfchar" => index = read_bfchar(&tokens, index + 1, &mut map),
            "beginbfrange" => index = read_bfrange(&tokens, index + 1, &mut map),
            _ => index += 1,
        }
    }
    map
}

/// splits the map into hex strings, brackets and words.
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '<' => {
                let mut hex = String::from("<");
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                    if next.is_ascii_hexdigit() {
                        hex.push(next);
                    }
                }
                tokens.push(hex);
            }
            '[' | ']' => tokens.push(character.to_string()),
            c if c.is_whitespace() => {}
            _ => {
                let mut word = character.to_string();
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || "<[]".contains(next) {
                        break;
                    }
                    word.push(next);
                    chars.next();
                }
                tokens.push(word);
            }
        }
    }
    tokens
}

fn hex_value(token: &str) -> Option<u32> {
    let digits = token.strip_prefix('<')?;
    if digits.is_empty() || digits.len() > 8 {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// a destination hex string is utf-16 text.
fn hex_text(token: &str) -> Option<String> {
    let digits = token.strip_prefix('<')?;
    let bytes: Vec<u8> = (0..digits.len() / 2)
        .filter_map(|index| u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16).ok())
        .collect();
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn read_bfchar(tokens: &[String], mut index: usize, map: &mut CodeMap) -> usize {
    while index + 1 < tokens.len() && tokens[index] != "endbfchar" {
        if let (Some(code), Some(text)) = (hex_value(&tokens[index]), hex_text(&tokens[index + 1]))
        {
            map.insert(code, text);
        }
        index += 2;
    }
    index + 1
}

fn read_bfrange(tokens: &[String], mut index: usize, map: &mut CodeMap) -> usize {
    while index + 2 < tokens.len() && tokens[index] != "endbfrange" {
        let (Some(low), Some(high)) = (hex_value(&tokens[index]), hex_value(&tokens[index + 1]))
        else {
            index += 1;
            continue;
        };
        let high = high.min(low.saturating_add(MAX_RANGE));
        if tokens[index + 2] == "[" {
            // one destination per code
            let mut cursor = index + 3;
            let mut code = low;
            while cursor < tokens.len() && tokens[cursor] != "]" {
                if let Some(text) = hex_text(&tokens[cursor]) {
                    if code <= high {
                        map.insert(code, text);
                    }
                    code += 1;
                }
                cursor += 1;
            }
            index = cursor + 1;
        } else {
            // the destination counts up from its first character
            if let Some(start) = hex_text(&tokens[index + 2]) {
                let mut units: Vec<u16> = start.encode_utf16().collect();
                for code in low..=high {
                    map.insert(code, String::from_utf16_lossy(&units));
                    if let Some(last) = units.last_mut() {
                        *last = last.wrapping_add(1);
                    }
                }
            }
            index += 3;
        }
    }
    index + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_single_codes_and_ranges() {
        let map = parse_to_unicode(
            b"2 beginbfchar <0003> <0020> <0024> <0041> endbfchar\n\
              1 beginbfrange <0044> <0046> <0061> endbfrange\n\
              1 beginbfrange <0050> <0051> [<00E9> <20AC>] endbfrange",
        );
        assert_eq!(map[&3], " ");
        assert_eq!(map[&0x24], "A");
        assert_eq!(map[&0x46], "c");
        assert_eq!(map[&0x51], "€");
    }
}
