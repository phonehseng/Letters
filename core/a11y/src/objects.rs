//! small helpers for reading and writing lopdf objects.
//!
//! pdf values can be written directly or as a reference to another object,
//! and damaged files point at objects that don't exist. every reader here
//! follows references and answers `None` instead of failing, so the rules
//! can be written as plain lookups.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use serde::{Deserialize, Serialize};

use crate::error::A11yError;

/// names an object in the file, the way the ui refers to a structure
/// element, an annotation or a form field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    pub number: u32,
    pub generation: u16,
}

impl From<ObjectId> for ObjectRef {
    fn from((number, generation): ObjectId) -> Self {
        Self { number, generation }
    }
}

impl ObjectRef {
    pub fn id(self) -> ObjectId {
        (self.number, self.generation)
    }
}

/// follows references until a real value is reached.
pub fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match doc.dereference(object) {
        Ok((_, value)) => value,
        Err(_) => &Object::Null,
    }
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

pub fn get_array<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [Object]> {
    get(doc, dict, key)?.as_array().ok().map(Vec::as_slice)
}

pub fn get_name<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    get(doc, dict, key)?.as_name().ok()
}

pub fn get_number(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<f32> {
    get(doc, dict, key)?.as_float().ok()
}

pub fn get_integer(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<i64> {
    match get(doc, dict, key)? {
        Object::Integer(value) => Some(*value),
        Object::Real(value) => Some(*value as i64),
        _ => None,
    }
}

pub fn get_bool(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<bool> {
    get(doc, dict, key)?.as_bool().ok()
}

/// a text string such as a title or alt text, decoded to rust text.
pub fn get_text(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    decode_text(get(doc, dict, key)?)
}

pub fn decode_text(object: &Object) -> Option<String> {
    match object {
        Object::String(..) => Some(
            lopdf::decode_text_string(object)
                .unwrap_or_else(|_| String::from_utf8_lossy(object.as_str().unwrap()).into_owned()),
        ),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

/// a text string to store in the file: plain bytes when the text is plain
/// ascii, utf-16 otherwise, so every language survives.
pub fn text_object(text: &str) -> Object {
    if text.is_ascii() {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        lopdf::text_string(text)
    }
}

pub fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

pub fn name_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// the reference stored under `key`, when the value is a reference.
pub fn get_reference(dict: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    dict.get(key).ok()?.as_reference().ok()
}

/// a page setting that may be written on the page or on any page tree
/// node above it.
pub fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    let mut seen = HashSet::new();
    while seen.insert(node) {
        let dict = doc.get_dictionary(node).ok()?;
        if let Some(value) = get(doc, dict, key) {
            return Some(value);
        }
        node = get_reference(dict, b"Parent")?;
    }
    None
}

/// four numbers of a rectangle setting, normalised so left < right and
/// bottom < top.
pub fn number_box(object: &Object) -> Option<[f32; 4]> {
    let items = object.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut numbers = [0.0; 4];
    for (slot, item) in numbers.iter_mut().zip(items) {
        *slot = item.as_float().ok()?;
    }
    let [a, b, c, d] = numbers;
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

/// the catalog, or a clear error when the file has none.
pub fn catalog(doc: &Document) -> Result<&Dictionary, A11yError> {
    doc.catalog()
        .map_err(|error| A11yError::Damaged(error.to_string()))
}

pub fn catalog_id(doc: &Document) -> Result<ObjectId, A11yError> {
    doc.trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| A11yError::Damaged(error.to_string()))
}

/// a mutable dictionary stored as its own object.
pub fn dict_mut(doc: &mut Document, id: ObjectId) -> Result<&mut Dictionary, A11yError> {
    match doc.get_object_mut(id) {
        Ok(Object::Dictionary(dict)) => Ok(dict),
        Ok(Object::Stream(stream)) => Ok(&mut stream.dict),
        _ => Err(A11yError::Internal(format!(
            "object {} {} is not a dictionary",
            id.0, id.1
        ))),
    }
}

/// the page objects in display order.
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

/// refuses a file that is still encrypted: lopdf could not read its
/// strings and streams, so every answer would be wrong.
pub fn ensure_readable(doc: &Document) -> Result<(), A11yError> {
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(A11yError::Locked);
    }
    Ok(())
}

/// refuses to change a file that is or was encrypted. saving it again
/// would quietly drop its protection, which the person did not ask for.
pub fn ensure_editable(doc: &Document) -> Result<(), A11yError> {
    ensure_readable(doc)?;
    if doc.was_encrypted() {
        return Err(A11yError::Locked);
    }
    Ok(())
}

/// the value stored in `dict` under `key` as an array, wrapping a single
/// value, so it can be appended to.
pub fn as_list(value: Option<&Object>) -> Vec<Object> {
    match value {
        None | Some(Object::Null) => Vec::new(),
        Some(Object::Array(items)) => items.clone(),
        Some(other) => vec![other.clone()],
    }
}
