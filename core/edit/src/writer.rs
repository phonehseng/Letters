//! writing operators and their operands back as content stream text.

use lopdf::Object;

use crate::pdf_values::format_number;

/// one operator with its operands, e.g. `/F1 12 Tf`.
pub(crate) fn operation(operator: &str, operands: &[Object]) -> Vec<u8> {
    let mut out = Vec::new();
    for operand in operands {
        object(operand, &mut out);
        out.push(b' ');
    }
    out.extend_from_slice(operator.as_bytes());
    out
}

/// strings are always written in hex: any byte is safe and no escaping
/// rules can go wrong.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("<{digits}>")
}

pub(crate) fn name(value: &[u8]) -> String {
    let mut text = String::from("/");
    for &byte in value {
        let regular = byte.is_ascii_graphic() && !b"()<>[]{}/%#".contains(&byte);
        if regular {
            text.push(char::from(byte));
        } else {
            text.push_str(&format!("#{byte:02X}"));
        }
    }
    text
}

pub(crate) fn object(value: &Object, out: &mut Vec<u8>) {
    match value {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(flag) => out.extend_from_slice(if *flag { b"true" } else { b"false" }),
        Object::Integer(number) => out.extend_from_slice(number.to_string().as_bytes()),
        Object::Real(number) => out.extend_from_slice(format_number(*number).as_bytes()),
        Object::Name(bytes) => out.extend_from_slice(name(bytes).as_bytes()),
        Object::String(bytes, _) => out.extend_from_slice(hex(bytes).as_bytes()),
        Object::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b' ');
                }
                object(item, out);
            }
            out.push(b']');
        }
        Object::Dictionary(dictionary) => {
            out.extend_from_slice(b"<<");
            for (key, item) in dictionary.iter() {
                out.extend_from_slice(name(key).as_bytes());
                out.push(b' ');
                object(item, out);
                out.push(b' ');
            }
            out.extend_from_slice(b">>");
        }
        Object::Reference((number, generation)) => {
            out.extend_from_slice(format!("{number} {generation} R").as_bytes())
        }
        // streams can't appear inside content
        Object::Stream(_) => out.extend_from_slice(b"null"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::StringFormat;

    #[test]
    fn operations_read_back_the_same() {
        let operands = vec![Object::Array(vec![
            Object::String(b"a(b".to_vec(), StringFormat::Literal),
            Object::Real(-120.5),
        ])];
        let written = operation("TJ", &operands);
        assert_eq!(written, b"[<612862> -120.5] TJ");
        let read = crate::lexer::parse(&written);
        assert_eq!(
            read[0].operands[0],
            Object::Array(vec![
                Object::String(b"a(b".to_vec(), StringFormat::Hexadecimal),
                Object::Real(-120.5)
            ])
        );
        assert_eq!(name(b"A B#"), "/A#20B#23");
    }
}
