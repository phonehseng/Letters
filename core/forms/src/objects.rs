//! small helpers for reading and writing pdf objects with lopdf.
//!
//! pdf lets almost any value be stored either directly or as a reference
//! to another object, so most reads go through `resolve` first.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use crate::error::{FormError, FormResult};
use crate::geometry::{PageFrame, PdfRect};
use crate::text::{decode_pdf_doc_encoding, is_printable_ascii};

/// how many references in a row are followed before giving up. a broken
/// file can make references point at each other forever.
const MAX_REFERENCE_HOPS: usize = 32;

/// follows references until a real value is reached.
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

/// a value from a dictionary, with references followed.
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

pub fn get_integer(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<i64> {
    match get(doc, dict, key)? {
        Object::Integer(value) => Some(*value),
        Object::Real(value) => Some(*value as i64),
        _ => None,
    }
}

/// a text string (like a field name or value) decoded into rust text.
pub fn get_text(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    match get(doc, dict, key)? {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

pub fn number_of(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

/// reads a four number array such as /Rect or /BBox.
pub fn rect_of(doc: &Document, object: &Object) -> Option<PdfRect> {
    let values = resolve(doc, object).as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let numbers: Vec<f32> = values
        .iter()
        .map(|value| number_of(resolve(doc, value)))
        .collect::<Option<_>>()?;
    Some(PdfRect::from_corners(
        numbers[0], numbers[1], numbers[2], numbers[3],
    ))
}

pub fn get_rect(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<PdfRect> {
    rect_of(doc, dict.get(key).ok()?)
}

pub fn rect_object(rect: &PdfRect) -> Object {
    Object::Array(vec![
        real(rect.left),
        real(rect.bottom),
        real(rect.right),
        real(rect.top),
    ])
}

/// a number object, kept whole when it is whole so files stay tidy.
pub fn real(value: f32) -> Object {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.fract() == 0.0 && rounded.abs() < 1.0e9 {
        Object::Integer(rounded as i64)
    } else {
        Object::Real(rounded)
    }
}

/// decodes a pdf text string: utf-16 when it starts with a byte order
/// mark, utf-8 when marked so (pdf 2.0), otherwise pdfdocencoding.
pub fn decode_text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        decode_pdf_doc_encoding(bytes)
    }
}

/// encodes text as a pdf text string. plain ascii (with line breaks)
/// stays readable in the file; anything else is written as utf-16 so no
/// letter is lost.
pub fn text_string(text: &str) -> Object {
    if text
        .chars()
        .all(|character| character == '\n' || is_printable_ascii(character))
    {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

pub fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

/// fails with a clear message when the document can't be worked on at all.
pub fn check_document(doc: &Document) -> FormResult<()> {
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(FormError::Encrypted);
    }
    if let Some(form) = acroform(doc) {
        if form.has(b"XFA") {
            return Err(FormError::XfaUnsupported);
        }
    }
    Ok(())
}

/// the document's form dictionary, if it has one.
pub fn acroform(doc: &Document) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    get_dict(doc, catalog, b"AcroForm")
}

/// the object id of the form dictionary. a form stored directly inside
/// the catalog is moved into its own object first, so callers always get
/// something they can change through `get_dictionary_mut`.
pub fn acroform_id(doc: &mut Document) -> FormResult<Option<ObjectId>> {
    let catalog_id = catalog_id(doc)?;
    let entry = doc
        .get_dictionary(catalog_id)?
        .get(b"AcroForm")
        .ok()
        .cloned();
    match entry {
        Some(Object::Reference(id)) => Ok(Some(id)),
        Some(Object::Dictionary(dict)) => {
            let id = doc.add_object(Object::Dictionary(dict));
            doc.get_dictionary_mut(catalog_id)?
                .set("AcroForm", Object::Reference(id));
            Ok(Some(id))
        }
        _ => Ok(None),
    }
}

pub fn catalog_id(doc: &Document) -> FormResult<ObjectId> {
    doc.trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| FormError::BadPdf("the file has no catalog".into()))
}

/// the ids of every page in order, so index 0 is the first page.
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

pub fn page_id(doc: &Document, page: u32) -> FormResult<ObjectId> {
    page_ids(doc)
        .get(page as usize)
        .copied()
        .ok_or(FormError::UnknownPage(page + 1))
}

/// which page each annotation sits on, read from the pages' /Annots lists.
pub fn annotation_pages(doc: &Document) -> HashMap<ObjectId, u32> {
    let mut pages = HashMap::new();
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let Ok(page) = doc.get_dictionary(page_id) else {
            continue;
        };
        for annotation in get_array(doc, page, b"Annots").into_iter().flatten() {
            if let Object::Reference(id) = annotation {
                pages.entry(*id).or_insert(index as u32);
            }
        }
    }
    pages
}

/// a page attribute that may be written on the page or any of its parents.
pub fn inherited_page_value<'a>(
    doc: &'a Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<&'a Object> {
    let mut current = doc.get_dictionary(page_id).ok()?;
    for _ in 0..MAX_REFERENCE_HOPS {
        if let Some(value) = get(doc, current, key) {
            return Some(value);
        }
        current = get_dict(doc, current, b"Parent")?;
    }
    None
}

/// how the page is shown: its visible box and its turn.
pub fn page_frame(doc: &Document, page_id: ObjectId) -> PageFrame {
    let letter = PdfRect::from_corners(0.0, 0.0, 612.0, 792.0);
    let media = inherited_page_value(doc, page_id, b"MediaBox")
        .and_then(|value| rect_of(doc, value))
        .unwrap_or(letter);
    let crop = inherited_page_value(doc, page_id, b"CropBox")
        .and_then(|value| rect_of(doc, value))
        .and_then(|crop| crop.intersect(&media))
        .unwrap_or(media);
    let rotate = inherited_page_value(doc, page_id, b"Rotate")
        .and_then(number_of)
        .unwrap_or(0.0);
    PageFrame::new(crop, rotate as i64)
}

/// adds a reference to an array entry of a dictionary, creating the array
/// when missing and following it when it is stored as its own object.
pub fn push_to_array(
    doc: &mut Document,
    owner: ObjectId,
    key: &[u8],
    value: Object,
) -> FormResult<()> {
    insert_into_array(doc, owner, key, None, value)
}

/// like `push_to_array` but puts the value at `position` (or the end).
pub fn insert_into_array(
    doc: &mut Document,
    owner: ObjectId,
    key: &[u8],
    position: Option<usize>,
    value: Object,
) -> FormResult<()> {
    let entry = doc.get_dictionary(owner)?.get(key).ok().cloned();
    let insert = |array: &mut Vec<Object>| {
        let at = position.unwrap_or(array.len()).min(array.len());
        array.insert(at, value.clone());
    };
    match entry {
        Some(Object::Reference(array_id)) => match doc.get_object_mut(array_id)? {
            Object::Array(array) => insert(array),
            _ => {
                doc.get_dictionary_mut(owner)?
                    .set(key.to_vec(), Object::Array(vec![value.clone()]));
            }
        },
        Some(Object::Array(mut array)) => {
            insert(&mut array);
            doc.get_dictionary_mut(owner)?
                .set(key.to_vec(), Object::Array(array));
        }
        _ => {
            doc.get_dictionary_mut(owner)?
                .set(key.to_vec(), Object::Array(vec![value]));
        }
    }
    Ok(())
}

/// removes every reference to `target` from an array entry.
pub fn remove_from_array(
    doc: &mut Document,
    owner: ObjectId,
    key: &[u8],
    target: ObjectId,
) -> FormResult<()> {
    let entry = doc.get_dictionary(owner)?.get(key).ok().cloned();
    let keep = |object: &Object| object.as_reference().ok() != Some(target);
    match entry {
        Some(Object::Reference(array_id)) => {
            if let Object::Array(array) = doc.get_object_mut(array_id)? {
                array.retain(keep);
            }
        }
        Some(Object::Array(mut array)) => {
            array.retain(keep);
            doc.get_dictionary_mut(owner)?
                .set(key.to_vec(), Object::Array(array));
        }
        _ => {}
    }
    Ok(())
}

/// the references in an array entry, in order.
pub fn references_in(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<ObjectId> {
    get_array(doc, dict, key)
        .into_iter()
        .flatten()
        .filter_map(|object| object.as_reference().ok())
        .collect()
}

/// a copy of a page's resources (its own or inherited from a parent).
/// changes are written back onto the page itself, because resources are
/// often shared and changing a shared one would change other pages too.
fn page_resources_copy(doc: &Document, page_id: ObjectId) -> Dictionary {
    inherited_page_value(doc, page_id, b"Resources")
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// adds an xobject to a page's resources under a fresh name and returns
/// that name.
pub fn add_page_xobject(
    doc: &mut Document,
    page_id: ObjectId,
    prefix: &str,
    xobject: ObjectId,
) -> FormResult<String> {
    let mut resources = page_resources_copy(doc, page_id);
    let mut xobjects = match resources
        .get(b"XObject")
        .ok()
        .map(|value| resolve(doc, value))
    {
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    let mut counter = 1;
    let chosen = loop {
        let candidate = format!("{prefix}{counter}");
        if !xobjects.has(candidate.as_bytes()) {
            break candidate;
        }
        counter += 1;
    };
    xobjects.set(chosen.as_bytes().to_vec(), Object::Reference(xobject));
    resources.set("XObject", Object::Dictionary(xobjects));
    doc.get_dictionary_mut(page_id)?
        .set("Resources", Object::Dictionary(resources));
    Ok(chosen)
}

/// draws extra content on top of a page.
///
/// the existing content is wrapped in q ... Q first, so whatever state it
/// leaves behind (a changed matrix, a clip) can't move the new drawing.
pub fn append_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content: &[u8],
) -> FormResult<()> {
    let existing: Vec<Object> = match doc.get_dictionary(page_id)?.get(b"Contents").ok() {
        Some(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Some(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let before = doc.add_object(lopdf::Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut after_bytes = b"\nQ\n".to_vec();
    after_bytes.extend_from_slice(content);
    let mut after_stream = lopdf::Stream::new(Dictionary::new(), after_bytes);
    // small streams are left as they are; compression only pays off for big ones
    if content.len() > 512 {
        let _ = after_stream.compress();
    }
    let after = doc.add_object(after_stream);
    let mut contents = vec![Object::Reference(before)];
    contents.extend(existing);
    contents.push(Object::Reference(after));
    doc.get_dictionary_mut(page_id)?
        .set("Contents", Object::Array(contents));
    Ok(())
}
