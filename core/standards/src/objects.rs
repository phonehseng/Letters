//! small helpers for reading pdf objects with lopdf: following references,
//! reading typed values, and naming objects for reports.

use lopdf::{Dictionary, Document, Object, ObjectId};

/// how many references in a row are followed before giving up, so a
/// broken file whose references point at each other can't loop forever.
const MAX_REFERENCE_HOPS: usize = 32;

pub fn resolve<'a>(doc: &'a Document, mut object: &'a Object) -> &'a Object {
    for _ in 0..MAX_REFERENCE_HOPS {
        match object {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(target) => object = target,
                Err(_) => return &Object::Null,
            },
            _ => return object,
        }
    }
    &Object::Null
}

pub fn get<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let value = resolve(doc, dict.get(key).ok()?);
    (!value.is_null()).then_some(value)
}

pub fn get_dict<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    match get(doc, dict, key)? {
        Object::Dictionary(inner) => Some(inner),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub fn get_array<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    get(doc, dict, key)?.as_array().ok()
}

pub fn get_name(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<Vec<u8>> {
    get(doc, dict, key)?.as_name().ok().map(<[u8]>::to_vec)
}

pub fn get_number(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<f64> {
    number_of(get(doc, dict, key)?)
}

pub fn number_of(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

/// the names in a value that is one name or an array of names (/Filter).
pub fn names_in(doc: &Document, value: Option<&Object>) -> Vec<Vec<u8>> {
    match value.map(|value| resolve(doc, value)) {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| resolve(doc, item).as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    }
}

/// the dictionary of a dictionary or stream object.
pub fn dict_of(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

/// "12 0 R", the way pdf tools name an object.
pub fn object_label(id: ObjectId) -> String {
    format!("{} {} R", id.0, id.1)
}

/// the ids of every page in order, so index 0 is the first page.
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

/// a page attribute written on the page or inherited from a parent.
pub fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = doc.get_dictionary(page).ok()?;
    for _ in 0..MAX_REFERENCE_HOPS {
        if let Some(value) = get(doc, current, key) {
            return Some(value);
        }
        current = get_dict(doc, current, b"Parent")?;
    }
    None
}

/// a four number rectangle.
pub fn rect_of(doc: &Document, object: &Object) -> Option<[f64; 4]> {
    let values = resolve(doc, object).as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = number_of(resolve(doc, value))?;
    }
    Some(out)
}

/// the decoded bytes of a stream, or its raw bytes when it can't be decoded.
pub fn stream_bytes(stream: &lopdf::Stream) -> Vec<u8> {
    stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone())
}
