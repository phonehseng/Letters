//! reading a font's /ToUnicode map, which says which letters its codes stand for.
//!
//! a pdf string is a list of codes, not letters. for many fonts (all of the
//! two-byte ones) the only way back to letters is this little program, a
//! "CMap", that lists codes and the unicode text each one means. only the
//! parts that matter for that are read: code space ranges (how many bytes a
//! code takes), single codes (bfchar) and ranges of codes (bfrange).

use std::collections::HashMap;

/// a parsed /ToUnicode map.
#[derive(Debug, Default, Clone)]
pub(crate) struct ToUnicode {
    /// (byte length, first code, last code) of each code space range
    code_spaces: Vec<(u8, u32, u32)>,
    singles: HashMap<(u8, u32), String>,
    ranges: Vec<CodeRange>,
}

#[derive(Debug, Clone)]
struct CodeRange {
    length: u8,
    first: u32,
    last: u32,
    target: RangeTarget,
}

#[derive(Debug, Clone)]
enum RangeTarget {
    /// the first code means this text; each next code adds one to its last unit
    Counting(Vec<u16>),
    /// each code has its own text, in order
    Listed(Vec<String>),
}

/// the most entries one map may have. real maps list at most a few
/// thousand; the limit keeps a damaged file from using unbounded memory.
const MAX_ENTRIES: usize = 100_000;

impl ToUnicode {
    pub fn parse(program: &[u8]) -> ToUnicode {
        let tokens = tokenize(program);
        let mut map = ToUnicode::default();
        let mut position = 0;
        while position < tokens.len() {
            position = match &tokens[position] {
                Token::Word(word) if word == "begincodespacerange" => {
                    map.read_code_spaces(&tokens, position + 1)
                }
                Token::Word(word) if word == "beginbfchar" => {
                    map.read_singles(&tokens, position + 1)
                }
                Token::Word(word) if word == "beginbfrange" => {
                    map.read_ranges(&tokens, position + 1)
                }
                _ => position + 1,
            };
        }
        map
    }

    fn read_code_spaces(&mut self, tokens: &[Token], mut position: usize) -> usize {
        while let (Some(Token::Hex(low)), Some(Token::Hex(high))) =
            (tokens.get(position), tokens.get(position + 1))
        {
            self.code_spaces
                .push((low.len() as u8, code_value(low), code_value(high)));
            position += 2;
        }
        position
    }

    fn read_singles(&mut self, tokens: &[Token], mut position: usize) -> usize {
        while let (Some(Token::Hex(code)), Some(target)) =
            (tokens.get(position), tokens.get(position + 1))
        {
            if let Some(text) = target_text(target) {
                if self.singles.len() < MAX_ENTRIES {
                    self.singles
                        .insert((code.len() as u8, code_value(code)), text);
                }
            }
            position += 2;
        }
        position
    }

    fn read_ranges(&mut self, tokens: &[Token], mut position: usize) -> usize {
        loop {
            let (Some(Token::Hex(first)), Some(Token::Hex(last))) =
                (tokens.get(position), tokens.get(position + 1))
            else {
                return position;
            };
            let length = first.len() as u8;
            let (first, last) = (code_value(first), code_value(last));
            position += 2;
            let target = match tokens.get(position) {
                Some(Token::Hex(units)) => {
                    position += 1;
                    RangeTarget::Counting(utf16_units(units))
                }
                Some(Token::OpenArray) => {
                    let (texts, after) = read_array(tokens, position + 1);
                    position = after;
                    RangeTarget::Listed(texts)
                }
                _ => return position,
            };
            if last >= first && self.ranges.len() < MAX_ENTRIES {
                self.ranges.push(CodeRange {
                    length,
                    first,
                    last,
                    target,
                });
            }
        }
    }

    /// splits a string's bytes into codes, using the code space ranges to
    /// tell how many bytes each code takes. `default_length` is used when
    /// the map gives no code spaces (many maps leave them out).
    pub fn split_codes(&self, bytes: &[u8], default_length: u8) -> Vec<(u32, u8)> {
        let mut codes = Vec::new();
        let mut position = 0;
        while position < bytes.len() {
            let length = self
                .code_length_at(&bytes[position..])
                .unwrap_or(default_length)
                .clamp(1, 4) as usize;
            let end = (position + length).min(bytes.len());
            codes.push((code_value(&bytes[position..end]), (end - position) as u8));
            position = end;
        }
        codes
    }

    fn code_length_at(&self, bytes: &[u8]) -> Option<u8> {
        self.code_spaces
            .iter()
            .find(|(length, low, high)| {
                let length = *length as usize;
                bytes.len() >= length && (*low..=*high).contains(&code_value(&bytes[..length]))
            })
            .map(|(length, _, _)| *length)
    }

    pub fn has_code_spaces(&self) -> bool {
        !self.code_spaces.is_empty()
    }

    /// the text a code stands for, if the map knows it.
    pub fn text_for(&self, code: u32, length: u8) -> Option<String> {
        if let Some(text) = self.singles.get(&(length, code)) {
            return Some(text.clone());
        }
        let range = self
            .ranges
            .iter()
            .find(|range| range.length == length && (range.first..=range.last).contains(&code))?;
        let step = (code - range.first) as usize;
        match &range.target {
            RangeTarget::Counting(units) => {
                let mut units = units.clone();
                let last = units.last_mut()?;
                *last = last.wrapping_add(step as u16);
                Some(String::from_utf16_lossy(&units))
            }
            RangeTarget::Listed(texts) => texts.get(step).cloned(),
        }
    }
}

/// the pieces a CMap program is made of, as far as reading it needs.
#[derive(Debug, PartialEq)]
enum Token {
    Hex(Vec<u8>),
    Name(String),
    Word(String),
    OpenArray,
    CloseArray,
}

fn tokenize(program: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut position = 0;
    while position < program.len() {
        let byte = program[position];
        match byte {
            b'%' => {
                // a comment runs to the end of the line
                while position < program.len() && !matches!(program[position], b'\n' | b'\r') {
                    position += 1;
                }
            }
            b'<' if program.get(position + 1) == Some(&b'<') => position += 2,
            b'>' if program.get(position + 1) == Some(&b'>') => position += 2,
            b'<' => {
                let end = find(program, position, b'>');
                tokens.push(Token::Hex(hex_bytes(&program[position + 1..end])));
                position = end + 1;
            }
            b'[' => {
                tokens.push(Token::OpenArray);
                position += 1;
            }
            b']' => {
                tokens.push(Token::CloseArray);
                position += 1;
            }
            b'(' => {
                // literal strings only appear in the map's header; skip them
                position = find(program, position, b')') + 1;
            }
            b'/' => {
                let end = word_end(program, position + 1);
                let name = String::from_utf8_lossy(&program[position + 1..end]).into_owned();
                tokens.push(Token::Name(name));
                position = end;
            }
            _ if byte.is_ascii_whitespace() => position += 1,
            _ => {
                let end = word_end(program, position).max(position + 1);
                let word = String::from_utf8_lossy(&program[position..end]).into_owned();
                tokens.push(Token::Word(word));
                position = end;
            }
        }
    }
    tokens
}

fn find(program: &[u8], from: usize, wanted: u8) -> usize {
    program[from..]
        .iter()
        .position(|&byte| byte == wanted)
        .map(|offset| from + offset)
        .unwrap_or(program.len())
}

fn word_end(program: &[u8], from: usize) -> usize {
    program[from..]
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || b"<>[]()/%{}".contains(byte))
        .map(|offset| from + offset)
        .unwrap_or(program.len())
}

fn hex_bytes(digits: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = digits
        .iter()
        .filter_map(|&digit| (digit as char).to_digit(16).map(|value| value as u8))
        .collect();
    digits
        .chunks(2)
        // an odd last digit is followed by an implied zero
        .map(|pair| pair[0] * 16 + pair.get(1).copied().unwrap_or(0))
        .collect()
}

fn code_value(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(4)
        .fold(0, |value, &byte| (value << 8) | byte as u32)
}

fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks(2)
        .map(|pair| (pair[0] as u16) << 8 | pair.get(1).copied().unwrap_or(0) as u16)
        .collect()
}

fn target_text(token: &Token) -> Option<String> {
    match token {
        Token::Hex(units) => Some(String::from_utf16_lossy(&utf16_units(units))),
        // a glyph name target such as /space; rare, and only simple names are understood
        Token::Name(name) if name == "space" => Some(" ".into()),
        Token::Name(name) if name.chars().count() == 1 => Some(name.clone()),
        _ => None,
    }
}

fn read_array(tokens: &[Token], mut position: usize) -> (Vec<String>, usize) {
    let mut texts = Vec::new();
    while let Some(token) = tokens.get(position) {
        position += 1;
        match token {
            Token::CloseArray => break,
            other => texts.push(target_text(other).unwrap_or_default()),
        }
    }
    (texts, position)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &[u8] = b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapName /Adobe-Identity-UCS def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
2 beginbfchar
<0003> <0020>
<0011> <0048>
endbfchar
2 beginbfrange
<0020> <0022> <0061>
<0030> <0031> [<0066006C> <0058>]
endbfrange
endcmap";

    #[test]
    fn single_codes_are_read() {
        let map = ToUnicode::parse(PROGRAM);
        assert_eq!(map.text_for(0x11, 2).as_deref(), Some("H"));
        assert_eq!(map.text_for(0x03, 2).as_deref(), Some(" "));
    }

    #[test]
    fn counting_ranges_count_up() {
        let map = ToUnicode::parse(PROGRAM);
        assert_eq!(map.text_for(0x20, 2).as_deref(), Some("a"));
        assert_eq!(map.text_for(0x22, 2).as_deref(), Some("c"));
        assert_eq!(map.text_for(0x23, 2), None);
    }

    #[test]
    fn listed_ranges_can_hold_ligatures() {
        let map = ToUnicode::parse(PROGRAM);
        assert_eq!(map.text_for(0x30, 2).as_deref(), Some("fl"));
        assert_eq!(map.text_for(0x31, 2).as_deref(), Some("X"));
    }

    #[test]
    fn code_spaces_decide_how_bytes_split() {
        let map = ToUnicode::parse(PROGRAM);
        assert_eq!(
            map.split_codes(&[0x00, 0x11, 0x00, 0x20], 1),
            vec![(0x11, 2), (0x20, 2)]
        );
        let without_spaces = ToUnicode::default();
        assert_eq!(
            without_spaces.split_codes(&[1, 2, 3], 2),
            vec![(0x0102, 2), (3, 1)]
        );
    }
}
