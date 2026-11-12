//! small helpers for reading and writing pdf values.
//!
//! pdf files are written by many programs, and values that should be
//! numbers or arrays are sometimes stored behind a reference. these
//! helpers follow references and quietly answer "none" for anything
//! malformed, so one odd value never stops a page from being read.

use lopdf::{Dictionary, Document, Object};

/// follows a reference (or a few in a row) to the value behind it.
pub(crate) fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    doc.dereference(object)
        .map(|(_, object)| object)
        .unwrap_or(object)
}

pub(crate) fn number(doc: &Document, object: &Object) -> Option<f32> {
    resolve(doc, object)
        .as_float()
        .ok()
        .filter(|value| value.is_finite())
}

/// a list of numbers, or none if any entry isn't a number.
pub(crate) fn numbers(doc: &Document, object: &Object) -> Option<Vec<f32>> {
    resolve(doc, object)
        .as_array()
        .ok()?
        .iter()
        .map(|item| number(doc, item))
        .collect()
}

/// a pdf rectangle, normalised to `[left, bottom, right, top]`.
pub(crate) fn rect_from_object(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let values = numbers(doc, object)?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].min(values[2]),
        values[1].min(values[3]),
        values[0].max(values[2]),
        values[1].max(values[3]),
    ])
}

pub(crate) fn dict_number(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<f32> {
    number(doc, dict.get(key).ok()?)
}

pub(crate) fn dict_numbers(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<Vec<f32>> {
    numbers(doc, dict.get(key).ok()?)
}

pub(crate) fn dict_rect(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<[f32; 4]> {
    rect_from_object(doc, dict.get(key).ok()?)
}

pub(crate) fn dict_name<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a [u8]> {
    resolve(doc, dict.get(key).ok()?).as_name().ok()
}

pub(crate) fn dict_dict<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    resolve(doc, dict.get(key).ok()?).as_dict().ok()
}

pub(crate) fn dict_bytes<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a [u8]> {
    resolve(doc, dict.get(key).ok()?).as_str().ok()
}

/// a number for writing into a file. whole numbers are written as
/// integers and others are rounded to a thousandth of a point, which
/// keeps files small and is far finer than any screen or printer.
pub(crate) fn real(value: f32) -> Object {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded.fract() == 0.0 && rounded.abs() < 1e9 {
        Object::Integer(rounded as i64)
    } else {
        Object::Real(rounded)
    }
}

pub(crate) fn number_array(values: &[f32]) -> Object {
    Object::Array(values.iter().map(|value| real(*value)).collect())
}

pub(crate) fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

/// the same number written the short way for a content stream.
pub(crate) fn format_number(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    // "-0" looks odd and some old readers dislike it
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let text = format!("{rounded:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_short() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-0.0001), "0");
        assert_eq!(format_number(1.23456), "1.235");
        assert_eq!(real(3.0), Object::Integer(3));
        assert_eq!(real(0.25), Object::Real(0.25));
    }

    #[test]
    fn rectangles_are_normalised() {
        let doc = Document::new();
        let flipped = number_array(&[100.0, 50.0, 0.0, 10.0]);
        assert_eq!(
            rect_from_object(&doc, &flipped),
            Some([0.0, 10.0, 100.0, 50.0])
        );
        assert_eq!(rect_from_object(&doc, &number_array(&[1.0, 2.0])), None);
    }
}
