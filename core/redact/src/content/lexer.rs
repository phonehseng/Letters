//! splits pdf syntax into tokens: numbers, names, strings, brackets and
//! keywords. the same rules hold in content streams and in cmap files, so
//! both are read with this.

use lopdf::StringFormat;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Integer(i64),
    Real(f64),
    Name(Vec<u8>),
    String(Vec<u8>, StringFormat),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
    /// an operator, or true, false and null
    Keyword(Vec<u8>),
}

pub struct Lexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

pub fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' | b'\0')
}

pub fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_whitespace(byte) && !is_delimiter(byte)
}

impl<'a> Lexer<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn set_position(&mut self, position: usize) {
        self.position = position.min(self.bytes.len());
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// the next token, none at the end, or an error describing bad syntax.
    pub fn next_token(&mut self) -> Result<Option<Token>, String> {
        self.skip_whitespace_and_comments();
        let Some(&byte) = self.bytes.get(self.position) else {
            return Ok(None);
        };
        let token = match byte {
            b'[' => self.single(Token::ArrayStart),
            b']' => self.single(Token::ArrayEnd),
            b'{' | b'}' => self.single(Token::Keyword(vec![byte])),
            b'(' => self.literal_string()?,
            b'<' if self.bytes.get(self.position + 1) == Some(&b'<') => {
                self.position += 2;
                Token::DictStart
            }
            b'<' => self.hex_string()?,
            b'>' if self.bytes.get(self.position + 1) == Some(&b'>') => {
                self.position += 2;
                Token::DictEnd
            }
            b'/' => self.name(),
            b'>' | b')' => return Err(format!("unexpected '{}'", byte as char)),
            _ => self.number_or_keyword(),
        };
        Ok(Some(token))
    }

    fn single(&mut self, token: Token) -> Token {
        self.position += 1;
        token
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(&byte) = self.bytes.get(self.position) {
            if is_whitespace(byte) {
                self.position += 1;
            } else if byte == b'%' {
                while let Some(&byte) = self.bytes.get(self.position) {
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }
                    self.position += 1;
                }
            } else {
                break;
            }
        }
    }

    fn regular_run(&mut self) -> &'a [u8] {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(|&byte| is_regular(byte))
        {
            self.position += 1;
        }
        &self.bytes[start..self.position]
    }

    fn name(&mut self) -> Token {
        self.position += 1;
        let raw = self.regular_run();
        let mut name = Vec::with_capacity(raw.len());
        let mut index = 0;
        while index < raw.len() {
            if raw[index] == b'#' && index + 2 < raw.len() {
                if let Some(value) = hex_pair(raw[index + 1], raw[index + 2]) {
                    name.push(value);
                    index += 3;
                    continue;
                }
            }
            name.push(raw[index]);
            index += 1;
        }
        Token::Name(name)
    }

    fn number_or_keyword(&mut self) -> Token {
        let raw = self.regular_run();
        if raw.is_empty() {
            // a stray delimiter such as '{' is skipped by the caller's loop
            self.position += 1;
            return Token::Keyword(Vec::new());
        }
        parse_number(raw).unwrap_or_else(|| Token::Keyword(raw.to_vec()))
    }

    fn literal_string(&mut self) -> Result<Token, String> {
        self.position += 1;
        let mut text = Vec::new();
        let mut depth = 1;
        loop {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Err("a string is not closed".into());
            };
            self.position += 1;
            match byte {
                b'\\' => self.escape(&mut text),
                b'(' => {
                    depth += 1;
                    text.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    text.push(byte);
                }
                b'\r' => {
                    // an end of line inside a string always reads as \n
                    if self.bytes.get(self.position) == Some(&b'\n') {
                        self.position += 1;
                    }
                    text.push(b'\n');
                }
                _ => text.push(byte),
            }
        }
        Ok(Token::String(text, StringFormat::Literal))
    }

    fn escape(&mut self, text: &mut Vec<u8>) {
        let Some(&byte) = self.bytes.get(self.position) else {
            return;
        };
        self.position += 1;
        match byte {
            b'n' => text.push(b'\n'),
            b'r' => text.push(b'\r'),
            b't' => text.push(b'\t'),
            b'b' => text.push(0x08),
            b'f' => text.push(0x0C),
            b'0'..=b'7' => {
                let mut value = u32::from(byte - b'0');
                for _ in 0..2 {
                    match self.bytes.get(self.position) {
                        Some(&digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            self.position += 1;
                        }
                        _ => break,
                    }
                }
                text.push((value & 0xFF) as u8);
            }
            // a backslash at the end of a line joins the lines
            b'\r' => {
                if self.bytes.get(self.position) == Some(&b'\n') {
                    self.position += 1;
                }
            }
            b'\n' => {}
            _ => text.push(byte),
        }
    }

    fn hex_string(&mut self) -> Result<Token, String> {
        self.position += 1;
        let mut digits = Vec::new();
        loop {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Err("a hex string is not closed".into());
            };
            self.position += 1;
            match byte {
                b'>' => break,
                _ if is_whitespace(byte) => {}
                _ => match hex_value(byte) {
                    Some(value) => digits.push(value),
                    None => return Err("a hex string holds a letter that isn't hex".into()),
                },
            }
        }
        if digits.len() % 2 == 1 {
            digits.push(0);
        }
        let text = digits
            .chunks(2)
            .map(|pair| pair[0] * 16 + pair[1])
            .collect();
        Ok(Token::String(text, StringFormat::Hexadecimal))
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

/// reads `12`, `-3`, `+.5`, `4.` and similar. anything else is a keyword.
fn parse_number(raw: &[u8]) -> Option<Token> {
    let text = std::str::from_utf8(raw).ok()?;
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    if body.is_empty()
        || !body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    if body.bytes().filter(|&byte| byte == b'.').count() > 1 || body == "." {
        return None;
    }
    if body.contains('.') {
        let normalised = if text.ends_with('.') {
            format!("{text}0")
        } else {
            text.to_owned()
        };
        normalised.parse::<f64>().ok().map(Token::Real)
    } else {
        match text.parse::<i64>() {
            Ok(value) => Some(Token::Integer(value)),
            // a whole number too big for 64 bits is still a number
            Err(_) => text.parse::<f64>().ok().map(Token::Real),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &[u8]) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut found = Vec::new();
        while let Some(token) = lexer.next_token().unwrap() {
            found.push(token);
        }
        found
    }

    #[test]
    fn reads_numbers_names_and_keywords() {
        assert_eq!(
            tokens(b"12 -3.5 +.5 4. /F#201 Tf % a comment\nq"),
            vec![
                Token::Integer(12),
                Token::Real(-3.5),
                Token::Real(0.5),
                Token::Real(4.0),
                Token::Name(b"F 1".to_vec()),
                Token::Keyword(b"Tf".to_vec()),
                Token::Keyword(b"q".to_vec()),
            ]
        );
    }

    #[test]
    fn reads_escapes_in_literal_strings() {
        assert_eq!(
            tokens(b"(a\\(b\\)c\\101\\\nd (nested))"),
            vec![Token::String(
                b"a(b)cAd (nested)".to_vec(),
                StringFormat::Literal
            )]
        );
    }

    #[test]
    fn reads_hex_strings_with_odd_digits() {
        assert_eq!(
            tokens(b"<48 69 7>"),
            vec![Token::String(
                vec![0x48, 0x69, 0x70],
                StringFormat::Hexadecimal
            )]
        );
    }

    #[test]
    fn reads_dictionaries_and_arrays() {
        assert_eq!(
            tokens(b"<</A [1]>>"),
            vec![
                Token::DictStart,
                Token::Name(b"A".to_vec()),
                Token::ArrayStart,
                Token::Integer(1),
                Token::ArrayEnd,
                Token::DictEnd,
            ]
        );
    }
}
