//! reading cmap files: the tables that say how the bytes of a string split
//! into character codes, which glyph (cid) each code draws, and which
//! unicode text each code stands for (a /ToUnicode cmap).
//!
//! only the parts that matter here are read: code space ranges, cid
//! ranges, unicode ranges and the writing mode.

use std::collections::HashMap;

use crate::content::lexer::{Lexer, Token};

/// the byte ranges that make up valid codes, e.g. <00> to <FF> for one
/// byte codes or <0000> to <FFFF> for two.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeSpace {
    ranges: Vec<(Vec<u8>, Vec<u8>)>,
}

impl CodeSpace {
    pub fn two_bytes() -> Self {
        Self {
            ranges: vec![(vec![0, 0], vec![0xFF, 0xFF])],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// splits a string into (code, byte length) pairs.
    pub fn split(&self, bytes: &[u8]) -> Vec<(u32, usize)> {
        let mut codes = Vec::new();
        let mut position = 0;
        while position < bytes.len() {
            let length = self.code_length(&bytes[position..]);
            let end = (position + length).min(bytes.len());
            let code = bytes[position..end]
                .iter()
                .fold(0u32, |code, &byte| (code << 8) | u32::from(byte));
            codes.push((code, end - position));
            position = end;
        }
        codes
    }

    /// how many bytes the code at the start of `bytes` takes. a code that
    /// fits no range takes as many bytes as the shortest range, which is
    /// what viewers do with such broken strings.
    fn code_length(&self, bytes: &[u8]) -> usize {
        for length in 1..=4 {
            if bytes.len() < length {
                break;
            }
            let candidate = &bytes[..length];
            let fits = self.ranges.iter().any(|(low, high)| {
                low.len() == length
                    && candidate
                        .iter()
                        .zip(low.iter().zip(high))
                        .all(|(byte, (low, high))| byte >= low && byte <= high)
            });
            if fits {
                return length;
            }
        }
        self.ranges
            .iter()
            .map(|(low, _)| low.len())
            .min()
            .unwrap_or(1)
            .max(1)
    }
}

/// everything read from one cmap.
#[derive(Clone, Debug, Default)]
pub struct CMap {
    pub code_space: CodeSpace,
    /// code to cid, from cidchar and cidrange
    pub cids: HashMap<u32, u32>,
    /// ranges of codes to cids that are too long to list one by one
    pub cid_ranges: Vec<(u32, u32, u32)>,
    /// code to unicode text, from bfchar and bfrange
    pub unicode: HashMap<u32, String>,
    /// true for vertical writing (/WMode 1)
    pub vertical: bool,
    /// the name of a cmap this one builds on (usecmap)
    pub uses: Option<Vec<u8>>,
}

/// unicode ranges longer than this are kept out of the map, so a broken
/// cmap cannot make Letters build a table of millions of entries.
const MAX_RANGE: u32 = 65_536;

impl CMap {
    pub fn cid(&self, code: u32) -> Option<u32> {
        if let Some(cid) = self.cids.get(&code) {
            return Some(*cid);
        }
        self.cid_ranges
            .iter()
            .find(|(low, high, _)| code >= *low && code <= *high)
            .map(|(low, _, first)| first + (code - low))
    }

    pub fn parse(bytes: &[u8]) -> CMap {
        let mut cmap = CMap::default();
        let tokens = read_tokens(bytes);
        let mut index = 0;
        while index < tokens.len() {
            index = match &tokens[index] {
                Token::Keyword(word) => cmap.section(word, &tokens, index),
                Token::Name(name) if name == b"WMode" => {
                    if let Some(Token::Integer(mode)) = tokens.get(index + 1) {
                        cmap.vertical = *mode == 1;
                    }
                    index + 1
                }
                _ => index + 1,
            };
        }
        cmap
    }

    /// reads the section starting at `index` and returns where to go on.
    fn section(&mut self, word: &[u8], tokens: &[Token], index: usize) -> usize {
        match word {
            b"begincodespacerange" => {
                self.read_groups(tokens, index + 1, 2, b"endcodespacerange", |cmap, group| {
                    if let [Token::String(low, _), Token::String(high, _)] = group {
                        cmap.code_space.ranges.push((low.clone(), high.clone()));
                    }
                })
            }
            b"begincidchar" => {
                self.read_groups(tokens, index + 1, 2, b"endcidchar", |cmap, group| {
                    if let [Token::String(code, _), Token::Integer(cid)] = group {
                        cmap.cids.insert(code_of(code), *cid as u32);
                    }
                })
            }
            b"begincidrange" => {
                self.read_groups(tokens, index + 1, 3, b"endcidrange", |cmap, group| {
                    if let [Token::String(low, _), Token::String(high, _), Token::Integer(cid)] =
                        group
                    {
                        cmap.cid_ranges
                            .push((code_of(low), code_of(high), *cid as u32));
                    }
                })
            }
            b"beginbfchar" => {
                self.read_groups(tokens, index + 1, 2, b"endbfchar", |cmap, group| {
                    if let [Token::String(code, _), Token::String(text, _)] = group {
                        cmap.unicode.insert(code_of(code), utf16_text(text));
                    }
                })
            }
            b"beginbfrange" => self.read_bf_ranges(tokens, index + 1),
            b"usecmap" => {
                if let Some(Token::Name(name)) = index.checked_sub(1).and_then(|i| tokens.get(i)) {
                    self.uses = Some(name.clone());
                }
                index + 1
            }
            _ => index + 1,
        }
    }

    /// reads fixed size groups of tokens until the end keyword.
    fn read_groups(
        &mut self,
        tokens: &[Token],
        mut index: usize,
        size: usize,
        end: &[u8],
        mut each: impl FnMut(&mut CMap, &[Token]),
    ) -> usize {
        while index + size <= tokens.len() {
            if matches!(&tokens[index], Token::Keyword(word) if word == end) {
                return index + 1;
            }
            each(self, &tokens[index..index + size]);
            index += size;
        }
        tokens.len()
    }

    /// bfrange entries are `<low> <high> <first text>` or
    /// `<low> <high> [<text> <text> ...]`.
    fn read_bf_ranges(&mut self, tokens: &[Token], mut index: usize) -> usize {
        while index + 2 < tokens.len() {
            if matches!(&tokens[index], Token::Keyword(word) if word == b"endbfrange") {
                return index + 1;
            }
            let (Token::String(low, _), Token::String(high, _)) =
                (&tokens[index], &tokens[index + 1])
            else {
                return index + 1;
            };
            let (low, high) = (code_of(low), code_of(high));
            index += 2;
            match &tokens[index] {
                Token::String(first, _) => {
                    self.add_unicode_range(low, high, first);
                    index += 1;
                }
                Token::ArrayStart => {
                    index += 1;
                    let mut code = low;
                    while let Some(Token::String(text, _)) = tokens.get(index) {
                        self.unicode.insert(code, utf16_text(text));
                        code += 1;
                        index += 1;
                    }
                    // the closing bracket
                    index += 1;
                }
                _ => index += 1,
            }
        }
        tokens.len()
    }

    fn add_unicode_range(&mut self, low: u32, high: u32, first: &[u8]) {
        if high < low || high - low > MAX_RANGE {
            return;
        }
        for offset in 0..=(high - low) {
            let mut text = first.to_vec();
            add_to_last_byte(&mut text, offset);
            self.unicode.insert(low + offset, utf16_text(&text));
        }
    }
}

/// a range's destination text counts up in its last byte (carrying into
/// the bytes before it).
fn add_to_last_byte(bytes: &mut [u8], amount: u32) {
    let mut carry = amount;
    for byte in bytes.iter_mut().rev() {
        if carry == 0 {
            break;
        }
        let sum = u32::from(*byte) + (carry & 0xFF);
        *byte = (sum & 0xFF) as u8;
        carry = (carry >> 8) + (sum >> 8);
    }
}

fn read_tokens(bytes: &[u8]) -> Vec<Token> {
    let mut lexer = Lexer::new(bytes);
    let mut tokens = Vec::new();
    loop {
        match lexer.next_token() {
            Ok(Some(token)) => tokens.push(token),
            Ok(None) => break,
            // a broken cmap is read as far as it goes
            Err(_) => {
                let next = lexer.position() + 1;
                if next >= bytes.len() {
                    break;
                }
                lexer.set_position(next);
            }
        }
    }
    tokens
}

pub fn code_of(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(4)
        .fold(0u32, |code, &byte| (code << 8) | u32::from(byte))
}

fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"/CIDInit /ProcSet findresource begin
        12 dict begin begincmap /WMode 0 def
        1 begincodespacerange <00> <7F> endcodespacerange
        1 begincodespacerange <8000> <FFFF> endcodespacerange
        1 begincidrange <8000> <80FF> 100 endcidrange
        2 beginbfchar <41> <0041> <42> <D83DDE00> endbfchar
        1 beginbfrange <8000> <8002> <0061> endbfrange
        1 beginbfrange <8010> <8011> [<0078> <0079>] endbfrange
        endcmap";

    #[test]
    fn splits_mixed_length_codes() {
        let cmap = CMap::parse(SAMPLE);
        assert_eq!(
            cmap.code_space.split(&[0x41, 0x80, 0x01, 0x42]),
            vec![(0x41, 1), (0x8001, 2), (0x42, 1)]
        );
    }

    #[test]
    fn maps_codes_to_cids_and_text() {
        let cmap = CMap::parse(SAMPLE);
        assert_eq!(cmap.cid(0x8005), Some(105));
        assert_eq!(cmap.unicode[&0x41], "A");
        assert_eq!(cmap.unicode[&0x42], "\u{1F600}");
        assert_eq!(cmap.unicode[&0x8002], "c");
        assert_eq!(cmap.unicode[&0x8011], "y");
    }
}
