//! small helpers over lopdf's objects: follow references, read numbers and
//! names, and decode streams.

use lopdf::{Dictionary, Document, Object, Stream};

/// follows references until a direct object.
pub fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    document
        .dereference(object)
        .map(|(_, object)| object)
        .unwrap_or(object)
}

/// a dictionary entry, with references followed.
pub fn get<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Object> {
    dictionary
        .get(key)
        .ok()
        .map(|object| resolve(document, object))
}

pub fn get_dict<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    match get(document, dictionary, key)? {
        Object::Dictionary(inner) => Some(inner),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub fn get_stream<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Stream> {
    match get(document, dictionary, key)? {
        Object::Stream(stream) => Some(stream),
        _ => None,
    }
}

pub fn get_array<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    match get(document, dictionary, key)? {
        Object::Array(items) => Some(items),
        _ => None,
    }
}

pub fn get_name<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a [u8]> {
    match get(document, dictionary, key)? {
        Object::Name(name) => Some(name),
        _ => None,
    }
}

pub fn get_number(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<f64> {
    number(get(document, dictionary, key)?)
}

pub fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        // lopdf stores reals as f32; reading it back through its decimal
        // text gives the double the file meant (0.001, not 0.0010000000475)
        Object::Real(value) => format!("{value}").parse().ok(),
        _ => None,
    }
}

/// every number of an array, with references followed.
pub fn numbers(document: &Document, items: &[Object]) -> Vec<f64> {
    items
        .iter()
        .filter_map(|item| number(resolve(document, item)))
        .collect()
}

/// a stream's bytes with every filter undone, or why that failed.
pub fn stream_bytes(stream: &Stream) -> Result<Vec<u8>, String> {
    if stream.dict.get(b"Filter").is_err() {
        return Ok(stream.content.clone());
    }
    stream
        .decompressed_content()
        .map_err(|error| error.to_string())
}

/// the names of a stream's filters, in decoding order.
pub fn filter_names(document: &Document, stream: &Stream) -> Vec<Vec<u8>> {
    match get(document, &stream.dict, b"Filter") {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| match resolve(document, item) {
                Object::Name(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// a pdf text string (pdfdoc encoding or utf-16 with a byte order mark).
pub fn text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|&b| char::from(b)).collect()
}
