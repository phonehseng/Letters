//! reading a content stream into operations, remembering where each one is.
//!
//! lopdf can decode content streams too, but it forgets where in the bytes
//! each operation came from. editing needs exactly that: to rewrite one
//! text operator and leave every other byte as it was, so this small
//! reader keeps the byte range of every operation.

use std::ops::Range;

use lopdf::{Dictionary, Object, StringFormat};

/// one operator with its operands, and where it sits in the stream.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Operation {
    pub operator: String,
    pub operands: Vec<Object>,
    /// from the first operand to the end of the operator (for an inline
    /// image, to the end of its "EI")
    pub span: Range<usize>,
}

/// reads every operation in a content stream. bytes that can't be read
/// are skipped, the way pdf readers tolerate small mistakes.
pub(crate) fn parse(content: &[u8]) -> Vec<Operation> {
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
                    let (dictionary, end) = lexer.inline_image();
                    operations.push(Operation {
                        operator,
                        operands: vec![Object::Dictionary(dictionary)],
                        span: begin..end,
                    });
                    operands.clear();
                    continue;
                }
                operations.push(Operation {
                    operator,
                    operands: std::mem::take(&mut operands),
                    span: begin..lexer.position,
                });
            }
            Token::Skip => {}
        }
    }
    operations
}

enum Token {
    Object(Object),
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
                Token::Object(Object::Dictionary(self.dictionary_body()))
            }
            b'<' => Token::Object(self.hex_string()),
            b'[' => {
                self.position += 1;
                Token::Object(Object::Array(self.array_body()))
            }
            b'0'..=b'9' | b'+' | b'-' | b'.' => match self.number() {
                Some(number) => Token::Object(number),
                None => Token::Skip,
            },
            _ if is_regular(byte) => {
                let word = self.word();
                match word.as_slice() {
                    b"true" => Token::Object(Object::Boolean(true)),
                    b"false" => Token::Object(Object::Boolean(false)),
                    b"null" => Token::Object(Object::Null),
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

    fn number(&mut self) -> Option<Object> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
        {
            self.position += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position]).ok()?;
        if text.contains('.') {
            // some writers put "-.5" or even "--5"; read what can be read
            let cleaned = text.trim_start_matches('+');
            cleaned.parse::<f32>().ok().map(Object::Real)
        } else {
            text.trim_start_matches('+')
                .parse::<i64>()
                .ok()
                .map(Object::Integer)
        }
    }

    fn name(&mut self) -> Object {
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
        Object::Name(decoded)
    }

    fn literal_string(&mut self) -> Object {
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
        Object::String(text, StringFormat::Literal)
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

    fn hex_string(&mut self) -> Object {
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
        Object::String(bytes, StringFormat::Hexadecimal)
    }

    fn array_body(&mut self) -> Vec<Object> {
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

    fn dictionary_body(&mut self) -> Dictionary {
        let mut dictionary = Dictionary::new();
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
                        Some(Token::Object(Object::Name(key))) => key,
                        Some(_) => continue,
                        None => break,
                    };
                    self.skip_space();
                    if let Some(Token::Object(value)) = self.next_token() {
                        dictionary.set(key, value);
                    }
                }
            }
        }
        dictionary
    }

    /// reads an inline image after "BI": its short dictionary, "ID", one
    /// space, the data, and "EI". returns the dictionary and where the
    /// image ends.
    fn inline_image(&mut self) -> (Dictionary, usize) {
        let mut dictionary = Dictionary::new();
        loop {
            self.skip_space();
            match self.next_token() {
                Some(Token::Keyword(word)) if word == b"ID" => break,
                Some(Token::Object(Object::Name(key))) => {
                    self.skip_space();
                    if let Some(Token::Object(value)) = self.next_token() {
                        dictionary.set(key, value);
                    }
                }
                None => return (dictionary, self.position),
                _ => {}
            }
        }
        // exactly one white-space byte separates ID from the data
        self.position += 1;
        let data_start = self.position.min(self.bytes.len());
        let end = find_inline_image_end(self.bytes, data_start);
        self.position = end;
        (dictionary, end)
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
        let show = &operations[4];
        assert_eq!(&content[show.span.clone()], b"(Hi) Tj");
        assert_eq!(
            show.operands[0],
            Object::String(b"Hi".to_vec(), StringFormat::Literal)
        );
    }

    #[test]
    fn reads_strings_arrays_and_names() {
        let operations = parse(b"[(a\\)b) -250 <4142>] TJ /Na#20me 0 Tf (x\\101\\\ny) Tj");
        let Object::Array(items) = &operations[0].operands[0] else {
            panic!("expected an array");
        };
        assert_eq!(
            items[0],
            Object::String(b"a)b".to_vec(), StringFormat::Literal)
        );
        assert_eq!(items[1], Object::Integer(-250));
        assert_eq!(
            items[2],
            Object::String(b"AB".to_vec(), StringFormat::Hexadecimal)
        );
        assert_eq!(operations[1].operands[0], Object::Name(b"Na me".to_vec()));
        assert_eq!(
            operations[2].operands[0],
            Object::String(b"xAy".to_vec(), StringFormat::Literal)
        );
    }

    #[test]
    fn skips_inline_image_data() {
        // the data holds the letters "EI" not followed by a space, which
        // must not be mistaken for the end
        let content = b"q BI /W 4 /H 1 /BPC 8 /CS /G ID \x45\x49\x41\x42 EI Q % done";
        let operations = parse(content);
        let operators: Vec<&str> = operations.iter().map(|op| op.operator.as_str()).collect();
        assert_eq!(operators, ["q", "BI", "Q"]);
        assert!(content[operations[1].span.clone()].ends_with(b"EI"));
    }
}
