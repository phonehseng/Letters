//! reading a content stream into operations, remembering where each one is.
//!
//! adapted from `core/edit`'s lexer, with two changes this crate needs:
//! numbers are read as `f64` (lopdf keeps reals as `f32`, about 3e-5 pt off
//! at 500 pt, which matters when positions are compared to 0.001 pt), and
//! inline image data is kept, because pictures are converted too.

use std::ops::Range;

/// one operand of a content stream operator.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Number(f64),
    Name(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<Operand>),
    Dictionary(Vec<(Vec<u8>, Operand)>),
    Boolean(bool),
    Null,
}

impl Operand {
    pub fn number(&self) -> Option<f64> {
        match self {
            Operand::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&[u8]> {
        match self {
            Operand::Name(name) => Some(name),
            _ => None,
        }
    }

    pub fn string(&self) -> Option<&[u8]> {
        match self {
            Operand::String(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// a dictionary entry, for inline image dictionaries.
    pub fn get(&self, key: &[u8]) -> Option<&Operand> {
        match self {
            Operand::Dictionary(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// one operator with its operands, and where it sits in the stream.
#[derive(Clone, Debug, PartialEq)]
pub struct Operation {
    pub operator: String,
    pub operands: Vec<Operand>,
    /// from the first operand to the end of the operator (for an inline
    /// image, to the end of its "EI")
    pub span: Range<usize>,
    /// the data of an inline image ("BI"), between "ID" and "EI"
    pub inline_data: Option<Vec<u8>>,
}

/// reads every operation in a content stream. bytes that can't be read are
/// skipped, the way pdf readers tolerate small mistakes.
pub fn parse(content: &[u8]) -> Vec<Operation> {
    let mut lexer = Lexer {
        bytes: content,
        position: 0,
    };
    let mut operations = Vec::new();
    let mut operands = Vec::new();
    let mut start = None;
    loop {
        lexer.skip_space();
        let token_start = lexer.position;
        let Some(token) = lexer.next_token() else {
            break;
        };
        match token {
            Token::Object(object) => {
                start.get_or_insert(token_start);
                operands.push(object);
            }
            Token::Keyword(keyword) => {
                let begin = start.take().unwrap_or(token_start);
                let operator = String::from_utf8_lossy(&keyword).into_owned();
                if operator == "BI" {
                    let (dictionary, data, end) = lexer.inline_image();
                    operations.push(Operation {
                        operator,
                        operands: vec![dictionary],
                        span: begin..end,
                        inline_data: Some(data),
                    });
                    operands.clear();
                    continue;
                }
                operations.push(Operation {
                    operator,
                    operands: std::mem::take(&mut operands),
                    span: begin..lexer.position,
                    inline_data: None,
                });
            }
            Token::Skip => {}
        }
    }
    operations
}

enum Token {
    Object(Operand),
    Keyword(Vec<u8>),
    /// a stray byte or an unmatched closing bracket
    Skip,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_space(byte) && !is_delimiter(byte)
}

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_space(&mut self) {
        while let Some(byte) = self.peek() {
            if is_space(byte) {
                self.position += 1;
            } else if byte == b'%' {
                while let Some(byte) = self.peek() {
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

    fn next_token(&mut self) -> Option<Token> {
        let byte = self.peek()?;
        Some(match byte {
            b'/' => Token::Object(self.name()),
            b'(' => Token::Object(self.literal_string()),
            b'<' if self.bytes.get(self.position + 1) == Some(&b'<') => {
                self.position += 2;
                Token::Object(self.dictionary_body())
            }
            b'<' => Token::Object(self.hex_string()),
            b'[' => {
                self.position += 1;
                Token::Object(Operand::Array(self.array_body()))
            }
            b'0'..=b'9' | b'+' | b'-' | b'.' => match self.number() {
                Some(number) => Token::Object(number),
                None => Token::Skip,
            },
            _ if is_regular(byte) => {
                let word = self.word();
                match word.as_slice() {
                    b"true" => Token::Object(Operand::Boolean(true)),
                    b"false" => Token::Object(Operand::Boolean(false)),
                    b"null" => Token::Object(Operand::Null),
                    _ => Token::Keyword(word),
                }
            }
            _ => {
                self.position += 1;
                Token::Skip
            }
        })
    }

    fn word(&mut self) -> Vec<u8> {
        let start = self.position;
        while self.peek().is_some_and(is_regular) {
            self.position += 1;
        }
        self.bytes[start..self.position].to_vec()
    }

    fn number(&mut self) -> Option<Operand> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
        {
            self.position += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position]).ok()?;
        // some writers put "-.5" or even "--5"; read what can be read
        let cleaned = text.trim_start_matches('+');
        let cleaned = if cleaned.starts_with("--") {
            &cleaned[1..]
        } else {
            cleaned
        };
        let cleaned = if cleaned.ends_with('.') && cleaned.len() > 1 {
            &cleaned[..cleaned.len() - 1]
        } else {
            cleaned
        };
        cleaned.parse::<f64>().ok().map(Operand::Number)
    }

    fn name(&mut self) -> Operand {
        self.position += 1;
        let raw = self.word();
        let mut decoded = Vec::with_capacity(raw.len());
        let mut index = 0;
        while index < raw.len() {
            if raw[index] == b'#' {
                if let Some(byte) = raw
                    .get(index + 1..index + 3)
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                {
                    decoded.push(byte);
                    index += 3;
                    continue;
                }
            }
            decoded.push(raw[index]);
            index += 1;
        }
        Operand::Name(decoded)
    }

    fn literal_string(&mut self) -> Operand {
        self.position += 1;
        let mut depth = 1;
        let mut text = Vec::new();
        while let Some(byte) = self.peek() {
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
                _ => text.push(byte),
            }
        }
        Operand::String(text)
    }

    fn escape(&mut self, text: &mut Vec<u8>) {
        let Some(byte) = self.peek() else {
            return;
        };
        self.position += 1;
        match byte {
            b'n' => text.push(b'\n'),
            b'r' => text.push(b'\r'),
            b't' => text.push(b'\t'),
            b'b' => text.push(8),
            b'f' => text.push(12),
            b'\r' => {
                // a backslash at the end of a line joins the lines
                if self.peek() == Some(b'\n') {
                    self.position += 1;
                }
            }
            b'\n' => {}
            b'0'..=b'7' => {
                let mut value = u32::from(byte - b'0');
                for _ in 0..2 {
                    match self.peek() {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            self.position += 1;
                        }
                        _ => break,
                    }
                }
                text.push(value as u8);
            }
            other => text.push(other),
        }
    }

    fn hex_string(&mut self) -> Operand {
        self.position += 1;
        let mut digits = Vec::new();
        while let Some(byte) = self.peek() {
            self.position += 1;
            if byte == b'>' {
                break;
            }
            if byte.is_ascii_hexdigit() {
                digits.push(byte);
            }
        }
        if digits.len() % 2 == 1 {
            // an odd last digit counts as if followed by 0
            digits.push(b'0');
        }
        let bytes = digits
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0)
            })
            .collect();
        Operand::String(bytes)
    }

    fn array_body(&mut self) -> Vec<Operand> {
        let mut items = Vec::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => break,
                Some(b']') => {
                    self.position += 1;
                    break;
                }
                _ => match self.next_token() {
                    Some(Token::Object(object)) => items.push(object),
                    // an operator inside an array is a mistake; skip it
                    Some(Token::Keyword(_)) | Some(Token::Skip) => {}
                    None => break,
                },
            }
        }
        items
    }

    fn dictionary_body(&mut self) -> Operand {
        let mut entries = Vec::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => break,
                Some(b'>') => {
                    self.position += 1;
                    if self.peek() == Some(b'>') {
                        self.position += 1;
                    }
                    break;
                }
                _ => {
                    let key = match self.next_token() {
                        Some(Token::Object(Operand::Name(key))) => key,
                        Some(_) => continue,
                        None => break,
                    };
                    self.skip_space();
                    if let Some(Token::Object(value)) = self.next_token() {
                        entries.push((key, value));
                    }
                }
            }
        }
        Operand::Dictionary(entries)
    }

    /// reads an inline image after "BI": its short dictionary, "ID", one
    /// space, the data, and "EI".
    fn inline_image(&mut self) -> (Operand, Vec<u8>, usize) {
        let mut entries = Vec::new();
        loop {
            self.skip_space();
            match self.next_token() {
                Some(Token::Keyword(word)) if word == b"ID" => break,
                Some(Token::Object(Operand::Name(key))) => {
                    self.skip_space();
                    if let Some(Token::Object(value)) = self.next_token() {
                        entries.push((key, value));
                    }
                }
                None => return (Operand::Dictionary(entries), Vec::new(), self.position),
                _ => {}
            }
        }
        // exactly one white-space byte separates ID from the data
        self.position += 1;
        let data_start = self.position.min(self.bytes.len());
        let end = find_inline_image_end(self.bytes, data_start);
        let data_end = end.saturating_sub(2).max(data_start);
        // the white space before "EI" is not part of the data
        let data_end = if data_end > data_start && is_space(self.bytes[data_end - 1]) {
            data_end - 1
        } else {
            data_end
        };
        self.position = end;
        (
            Operand::Dictionary(entries),
            self.bytes[data_start..data_end].to_vec(),
            end,
        )
    }
}

/// finds the "EI" that ends inline image data: preceded by white space and
/// followed by white space or the end. image data can contain the letters
/// "EI" by chance, so both neighbours are checked.
fn find_inline_image_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index + 1 < bytes.len() {
        let before_ok = index == from || is_space(bytes[index - 1]);
        let after_ok = bytes.get(index + 2).is_none_or(|byte| is_space(*byte));
        if bytes[index] == b'E' && bytes[index + 1] == b'I' && before_ok && after_ok {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_where_each_operation_is() {
        let content = b"q 1 0 0 1 10 20 cm BT /F1 12 Tf (Hi) Tj ET Q";
        let operations = parse(content);
        let operators: Vec<&str> = operations.iter().map(|op| op.operator.as_str()).collect();
        assert_eq!(operators, ["q", "cm", "BT", "Tf", "Tj", "ET", "Q"]);
        assert_eq!(&content[operations[4].span.clone()], b"(Hi) Tj");
    }

    #[test]
    fn numbers_keep_double_precision() {
        let operations = parse(b"1 0 0 1 523.45678 -.5 Tm");
        assert_eq!(operations[0].operands[4], Operand::Number(523.45678));
        assert_eq!(operations[0].operands[5], Operand::Number(-0.5));
    }

    #[test]
    fn reads_strings_arrays_and_names() {
        let operations = parse(b"[(a\\)b) -250 <4142>] TJ /Na#20me 0 Tf (x\\101\\\ny) Tj");
        let Operand::Array(items) = &operations[0].operands[0] else {
            panic!("expected an array");
        };
        assert_eq!(items[0], Operand::String(b"a)b".to_vec()));
        assert_eq!(items[1], Operand::Number(-250.0));
        assert_eq!(items[2], Operand::String(b"AB".to_vec()));
        assert_eq!(operations[1].operands[0], Operand::Name(b"Na me".to_vec()));
        assert_eq!(operations[2].operands[0], Operand::String(b"xAy".to_vec()));
    }

    #[test]
    fn keeps_inline_image_data() {
        // the data holds the letters "EI" not followed by a space, which
        // must not be mistaken for the end
        let content = b"q BI /W 4 /H 1 /BPC 8 /CS /G ID \x45\x49\x41\x42 EI Q % done";
        let operations = parse(content);
        let operators: Vec<&str> = operations.iter().map(|op| op.operator.as_str()).collect();
        assert_eq!(operators, ["q", "BI", "Q"]);
        assert_eq!(operations[1].inline_data.as_deref(), Some(&b"EIAB"[..]));
        assert_eq!(
            operations[1].operands[0].get(b"W"),
            Some(&Operand::Number(4.0))
        );
    }
}
