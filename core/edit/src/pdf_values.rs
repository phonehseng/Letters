//! small helpers for reading and writing pdf values.
//!
//! values that should be numbers or arrays are sometimes stored behind a
//! reference. these helpers follow references and quietly answer "none"
//! for anything malformed, so one odd value never stops a page being read.

use lopdf::{Dictionary, Document, Object};

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

pub(crate) fn dict_array<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Vec<Object>> {
    resolve(doc, dict.get(key).ok()?).as_array().ok()
}

/// the decoded bytes of a stream, whether given directly or by reference.
pub(crate) fn stream_bytes(doc: &Document, object: &Object) -> Option<Vec<u8>> {
    let stream = resolve(doc, object).as_stream().ok()?;
    if stream.dict.has(b"Filter") {
        stream.decompressed_content().ok()
    } else {
        Some(stream.content.clone())
    }
}

pub(crate) fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

/// a number for writing: whole numbers as integers, others rounded to a
/// thousandth, which keeps files small and is finer than any screen.
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

/// a number written the short way for a content stream. positions get
/// four decimals so re-set text lands within a ten-thousandth of a point.
pub(crate) fn format_number(value: f32) -> String {
    let rounded = (f64::from(value) * 10_000.0).round() / 10_000.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let text = format!("{rounded:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_short() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-0.00001), "0");
        assert_eq!(format_number(1.234_56), "1.2346");
        assert_eq!(real(3.0), Object::Integer(3));
    }
}
