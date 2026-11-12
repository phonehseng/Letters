//! reading and writing content streams: the lists of drawing instructions
//! that make up a page.
//!
//! a content stream is a series of operations, each some operands followed
//! by an operator: `72 700 Td (Hello) Tj`. inline images are the one odd
//! case: `BI <entries> ID <raw bytes> EI`, with raw image bytes in the
//! middle of the text.
//!
//! the reader is strict on purpose. if any part of a page cannot be read,
//! the caller refuses to redact it rather than guess, because a part that
//! was skipped could be exactly the part that should have been removed.

pub mod lexer;

use lopdf::{Dictionary, Object, Writer};

use lexer::{is_delimiter, is_whitespace, Lexer, Token};

/// one drawing instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Operator {
        name: String,
        operands: Vec<Object>,
    },
    InlineImage {
        /// the image's entries, with their short names as written (W, H, CS...)
        dict: Dictionary,
        /// the image bytes exactly as stored, filters still applied
        data: Vec<u8>,
    },
}

impl Op {
    pub fn new(name: &str, operands: Vec<Object>) -> Self {
        Op::Operator {
            name: name.to_owned(),
            operands,
        }
    }

    /// the operator's name, or "BI" for an inline image.
    pub fn name(&self) -> &str {
        match self {
            Op::Operator { name, .. } => name,
            Op::InlineImage { .. } => "BI",
        }
    }

    pub fn operands(&self) -> &[Object] {
        match self {
            Op::Operator { operands, .. } => operands,
            Op::InlineImage { .. } => &[],
        }
    }
}

/// how deeply arrays and dictionaries may nest in a content stream.
const MAX_NESTING: usize = 32;

/// reads a whole content stream, or explains which part could not be read.
pub fn parse(bytes: &[u8]) -> Result<Vec<Op>, String> {
    let mut lexer = Lexer::new(bytes);
    let mut ops = Vec::new();
    let mut operands = Vec::new();
    while let Some(token) = lexer.next_token()? {
        match token {
            Token::Keyword(word) => match word.as_slice() {
                b"true" => operands.push(Object::Boolean(true)),
                b"false" => operands.push(Object::Boolean(false)),
                b"null" => operands.push(Object::Null),
                b"BI" => {
                    operands.clear();
                    ops.push(read_inline_image(&mut lexer)?);
                }
                b"" | b"{" | b"}" => {}
                _ => {
                    let name = String::from_utf8_lossy(&word).into_owned();
                    ops.push(Op::Operator {
                        name,
                        operands: std::mem::take(&mut operands),
                    });
                }
            },
            other => operands.push(token_to_object(other, &mut lexer, 0)?),
        }
    }
    // operands left at the end belong to no operator and are ignored by
    // every viewer, so dropping them changes nothing that is drawn
    Ok(ops)
}

/// turns a token (and, for arrays and dictionaries, the tokens after it)
/// into one object.
fn token_to_object(token: Token, lexer: &mut Lexer, depth: usize) -> Result<Object, String> {
    if depth > MAX_NESTING {
        return Err("arrays are nested too deeply".into());
    }
    Ok(match token {
        Token::Integer(value) => Object::Integer(value),
        Token::Real(value) => Object::Real(value as f32),
        Token::Name(name) => Object::Name(name),
        Token::String(text, format) => Object::String(text, format),
        Token::ArrayStart => Object::Array(read_array(lexer, depth)?),
        Token::DictStart => Object::Dictionary(read_dictionary(lexer, depth)?),
        Token::Keyword(word) => match word.as_slice() {
            b"true" => Object::Boolean(true),
            b"false" => Object::Boolean(false),
            b"null" => Object::Null,
            _ => return Err(format!("unexpected '{}'", String::from_utf8_lossy(&word))),
        },
        Token::ArrayEnd => return Err("unexpected ']'".into()),
        Token::DictEnd => return Err("unexpected '>>'".into()),
    })
}

fn read_array(lexer: &mut Lexer, depth: usize) -> Result<Vec<Object>, String> {
    let mut items = Vec::new();
    loop {
        match lexer.next_token()? {
            None => return Err("an array is not closed".into()),
            Some(Token::ArrayEnd) => return Ok(items),
            Some(token) => items.push(token_to_object(token, lexer, depth + 1)?),
        }
    }
}

fn read_dictionary(lexer: &mut Lexer, depth: usize) -> Result<Dictionary, String> {
    let mut dict = Dictionary::new();
    loop {
        match lexer.next_token()? {
            None => return Err("a dictionary is not closed".into()),
            Some(Token::DictEnd) => return Ok(dict),
            Some(Token::Name(key)) => {
                let value = match lexer.next_token()? {
                    Some(token) => token_to_object(token, lexer, depth + 1)?,
                    None => return Err("a dictionary is not closed".into()),
                };
                dict.set(key, value);
            }
            Some(_) => return Err("a dictionary key is not a name".into()),
        }
    }
}

/// reads `BI ... ID <data> EI`, with the lexer just after `BI`.
fn read_inline_image(lexer: &mut Lexer) -> Result<Op, String> {
    let dict = read_inline_entries(lexer)?;
    let bytes = lexer.bytes();
    let mut start = lexer.position();
    // exactly one white-space byte separates ID from the data
    if bytes.get(start).is_some_and(|&byte| is_whitespace(byte)) {
        start += 1;
    }
    let end = match known_data_length(&dict) {
        Some(length) if ends_with_ei(bytes, start + length) => start + length,
        _ => find_ei(bytes, start).ok_or("an inline image has no end")?,
    };
    let data = bytes[start..end.min(bytes.len())].to_vec();
    let after_ei = skip_to_after_ei(bytes, end);
    lexer.set_position(after_ei);
    Ok(Op::InlineImage { dict, data })
}

fn read_inline_entries(lexer: &mut Lexer) -> Result<Dictionary, String> {
    let mut dict = Dictionary::new();
    loop {
        match lexer.next_token()? {
            None => return Err("an inline image has no data".into()),
            Some(Token::Keyword(word)) if word == b"ID" => return Ok(dict),
            Some(Token::Name(key)) => {
                let value = match lexer.next_token()? {
                    Some(token) => token_to_object(token, lexer, 1)?,
                    None => return Err("an inline image has no data".into()),
                };
                dict.set(key, value);
            }
            Some(_) => return Err("an inline image entry is not a name".into()),
        }
    }
}

/// the number of data bytes of an unfiltered inline image, when its
/// colour space is one of the built in ones.
fn known_data_length(dict: &Dictionary) -> Option<usize> {
    if inline_entry(dict, b"F", b"Filter").is_some() {
        return None;
    }
    let size = |short: &[u8], long: &[u8]| -> Option<usize> {
        usize::try_from(inline_entry(dict, short, long)?.as_i64().ok()?).ok()
    };
    let width = size(b"W", b"Width")?;
    let height = size(b"H", b"Height")?;
    let is_mask = matches!(
        inline_entry(dict, b"IM", b"ImageMask"),
        Some(Object::Boolean(true))
    );
    let bits = if is_mask {
        1
    } else {
        size(b"BPC", b"BitsPerComponent")?
    };
    let components = if is_mask {
        1
    } else {
        inline_components(inline_entry(dict, b"CS", b"ColorSpace")?)?
    };
    let row_bytes = (width.checked_mul(components)?.checked_mul(bits)?).div_ceil(8);
    row_bytes.checked_mul(height)
}

/// colour components per pixel of an inline image's colour space, when
/// it can be told without the page's resources.
pub fn inline_components(space: &Object) -> Option<usize> {
    match space {
        Object::Name(name) => match name.as_slice() {
            b"G" | b"DeviceGray" | b"CalGray" => Some(1),
            b"RGB" | b"DeviceRGB" | b"CalRGB" => Some(3),
            b"CMYK" | b"DeviceCMYK" => Some(4),
            _ => None,
        },
        Object::Array(items) => match items.first() {
            Some(Object::Name(name)) if name == b"I" || name == b"Indexed" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

/// an inline image entry under its short or long name.
pub fn inline_entry<'a>(dict: &'a Dictionary, short: &[u8], long: &[u8]) -> Option<&'a Object> {
    dict.get(short).or_else(|_| dict.get(long)).ok()
}

fn ends_with_ei(bytes: &[u8], at: usize) -> bool {
    let mut position = at;
    while bytes.get(position).is_some_and(|&byte| is_whitespace(byte)) {
        position += 1;
    }
    bytes.get(position..position + 2) == Some(b"EI") && ends_token(bytes, position + 2)
}

fn ends_token(bytes: &[u8], at: usize) -> bool {
    match bytes.get(at) {
        None => true,
        Some(&byte) => is_whitespace(byte) || is_delimiter(byte),
    }
}

/// finds where the data of a filtered inline image ends: at an `EI`
/// standing on its own and followed by what looks like more content
/// rather than more binary data.
fn find_ei(bytes: &[u8], start: usize) -> Option<usize> {
    let mut position = start;
    while position + 2 <= bytes.len() {
        let before_ok =
            position == start || is_whitespace(bytes[position - 1]) || bytes[position - 1] == b'>';
        if before_ok && &bytes[position..position + 2] == b"EI" && ends_token(bytes, position + 2) {
            let following = &bytes[position + 2..(position + 34).min(bytes.len())];
            if following.iter().all(|&byte| byte < 0x80) {
                // the white space before EI is not part of the data
                let mut end = position;
                if end > start && is_whitespace(bytes[end - 1]) {
                    end -= 1;
                }
                return Some(end);
            }
        }
        position += 1;
    }
    None
}

fn skip_to_after_ei(bytes: &[u8], from: usize) -> usize {
    let mut position = from;
    while position + 2 <= bytes.len() {
        if &bytes[position..position + 2] == b"EI" && ends_token(bytes, position + 2) {
            return position + 2;
        }
        position += 1;
    }
    bytes.len()
}

/// writes operations back as a content stream.
pub fn serialize(ops: &[Op]) -> Vec<u8> {
    let mut out = Vec::new();
    for op in ops {
        write_op(&mut out, op);
        out.push(b'\n');
    }
    out
}

fn write_op(out: &mut Vec<u8>, op: &Op) {
    match op {
        Op::Operator { name, operands } => {
            for operand in operands {
                write_object(out, operand);
                out.push(b' ');
            }
            out.extend_from_slice(name.as_bytes());
        }
        Op::InlineImage { dict, data } => {
            out.extend_from_slice(b"BI");
            for (key, value) in dict.iter() {
                out.push(b' ');
                write_object(out, &Object::Name(key.clone()));
                out.push(b' ');
                write_object(out, value);
            }
            out.extend_from_slice(b" ID ");
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nEI");
        }
    }
}

pub fn write_object(out: &mut Vec<u8>, object: &Object) {
    Writer::write_object(out, object).expect("writing to memory cannot fail");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_operators_with_their_operands() {
        let ops = parse(b"BT /F1 12 Tf 72 700 Td [(He) -20 (llo)] TJ ET").unwrap();
        let names: Vec<&str> = ops.iter().map(Op::name).collect();
        assert_eq!(names, ["BT", "Tf", "Td", "TJ", "ET"]);
        assert_eq!(ops[1].operands()[1], Object::Integer(12));
    }

    #[test]
    fn reads_an_unfiltered_inline_image_whose_data_holds_ei() {
        // the data bytes spell "EI" on purpose; the known length wins
        let ops = parse(b"q BI /W 2 /H 1 /BPC 8 /CS /G ID EI EI Q").unwrap();
        assert_eq!(ops.len(), 3);
        match &ops[1] {
            Op::InlineImage { data, .. } => assert_eq!(data, b"EI"),
            other => panic!("expected an inline image, found {other:?}"),
        }
        assert_eq!(ops[2].name(), "Q");
    }

    #[test]
    fn reads_a_filtered_inline_image_by_finding_ei() {
        let ops = parse(b"BI /W 1 /H 1 /BPC 8 /CS /G /F /AHx ID 7F> EI 1 0 0 RG").unwrap();
        match &ops[0] {
            Op::InlineImage { data, .. } => assert_eq!(data, b"7F>"),
            other => panic!("expected an inline image, found {other:?}"),
        }
        assert_eq!(ops[1].name(), "RG");
    }

    #[test]
    fn refuses_broken_syntax() {
        assert!(parse(b"BT (not closed Tj ET").is_err());
        assert!(parse(b"[1 2 TJ").is_err());
    }

    #[test]
    fn writes_what_it_read() {
        let source = b"q 1 0 0 1 72.5 700 cm BI /W 1 /H 1 /BPC 8 /CS /G ID \x80\nEI Q";
        let ops = parse(source).unwrap();
        assert_eq!(parse(&serialize(&ops)).unwrap(), ops);
    }
}
