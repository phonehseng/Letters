//! small helpers for reading lopdf objects without a wall of error handling.
//!
//! pdf files in the wild are often slightly broken, so readers here answer
//! "not found" instead of failing; the callers decide what is safe to do
//! when something is missing.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// the null object, for references that lead nowhere.
static NULL: Object = Object::Null;

/// follows references until a real object is reached. a missing or
/// looping reference reads as null.
pub fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    let mut current = object;
    for _ in 0..32 {
        match current {
            Object::Reference(id) => match doc.objects.get(id) {
                Some(next) => current = next,
                None => return &NULL,
            },
            _ => return current,
        }
    }
    &NULL
}

pub fn number(doc: &Document, object: &Object) -> Option<f64> {
    match resolve(doc, object) {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

pub fn dict_number(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<f64> {
    dict.get(key).ok().and_then(|value| number(doc, value))
}

pub fn dict_get<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    dict.get(key).ok().map(|value| resolve(doc, value))
}

pub fn dict_dict<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    match dict_get(doc, dict, key)? {
        Object::Dictionary(inner) => Some(inner),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub fn dict_name<'a>(doc: &'a Document, dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dict_get(doc, dict, key)?.as_name().ok()
}

pub fn dict_array<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    dict_get(doc, dict, key)?.as_array().ok()
}

/// the dictionary of a dictionary or stream object.
pub fn as_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

pub fn as_dict_mut(object: &mut Object) -> Option<&mut Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&mut stream.dict),
        _ => None,
    }
}

/// numbers from an array of numbers, skipping anything else.
pub fn numbers(doc: &Document, array: &[Object]) -> Vec<f64> {
    array
        .iter()
        .filter_map(|value| number(doc, value))
        .collect()
}

/// text from a pdf text string (pdfdoc encoding or utf-16 with a byte
/// order mark), for showing to people and for warnings.
pub fn text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    // pdfdoc encoding matches latin-1 for the characters people type
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

/// a stream's content with its filters undone, refusing anything larger
/// than `MAX_DECODED_BYTES` so a tiny file cannot fill the memory.
pub fn decoded_content(stream: &Stream) -> Result<Vec<u8>, String> {
    if stream.dict.get(b"Filter").is_err() {
        return Ok(stream.content.clone());
    }
    stream
        .decompressed_content_with_limit(MAX_DECODED_BYTES)
        .map_err(|error| error.to_string())
}

/// the biggest stream Letters will decode: 512 mb, far above any real
/// page or image, far below what would make the machine struggle.
pub const MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;

/// how many times each object is referenced, from anywhere in the file.
/// used to tell whether an object belongs to one page or is shared.
pub fn reference_counts(doc: &Document) -> HashMap<ObjectId, usize> {
    let mut counts = HashMap::new();
    let mut count =
        |object: &Object| visit_references(object, &mut |id| *counts.entry(id).or_insert(0) += 1);
    for object in doc.objects.values() {
        count(object);
    }
    for (_, value) in doc.trailer.iter() {
        count(value);
    }
    counts
}

/// calls `found` for every reference inside an object, however deep.
pub fn visit_references(object: &Object, found: &mut dyn FnMut(ObjectId)) {
    match object {
        Object::Reference(id) => found(*id),
        Object::Array(items) => items.iter().for_each(|item| visit_references(item, found)),
        Object::Dictionary(dict) => dict
            .iter()
            .for_each(|(_, value)| visit_references(value, found)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .for_each(|(_, value)| visit_references(value, found)),
        _ => {}
    }
}

/// a new compressed stream holding `content`.
pub fn flate_stream(mut dict: Dictionary, content: &[u8]) -> Stream {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(content)
        .expect("writing to memory cannot fail");
    let compressed = encoder.finish().expect("writing to memory cannot fail");
    dict.remove(b"DecodeParms");
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("Length", compressed.len() as i64);
    Stream::new(dict, compressed)
}

/// hands out object numbers for new objects without touching the
/// document, so a preview can plan edits and throw them away.
#[derive(Debug)]
pub struct NewObjects {
    next_number: u32,
    pub objects: Vec<(ObjectId, Object)>,
}

impl NewObjects {
    pub fn after(doc: &Document) -> Self {
        let highest = doc
            .objects
            .keys()
            .map(|id| id.0)
            .max()
            .unwrap_or(0)
            .max(doc.max_id);
        Self {
            next_number: highest + 1,
            objects: Vec::new(),
        }
    }

    pub fn add(&mut self, object: Object) -> ObjectId {
        let id = (self.next_number, 0);
        self.next_number += 1;
        self.objects.push((id, object));
        id
    }

    /// puts every planned object into the document.
    pub fn commit(self, doc: &mut Document) {
        for (id, object) in self.objects {
            doc.max_id = doc.max_id.max(id.0);
            doc.objects.insert(id, object);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strings_read_both_encodings() {
        assert_eq!(text_string(b"Report"), "Report");
        assert_eq!(text_string(&[0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69]), "Hi");
    }

    #[test]
    fn looping_references_read_as_null() {
        let mut doc = Document::new();
        doc.objects.insert((1, 0), Object::Reference((2, 0)));
        doc.objects.insert((2, 0), Object::Reference((1, 0)));
        assert_eq!(resolve(&doc, &Object::Reference((1, 0))), &Object::Null);
    }
}
