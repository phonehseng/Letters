//! places outside the redacted boxes that may still show what was removed.
//!
//! redaction removes what is under the boxes and nothing else. the same
//! name or number can also sit in a bookmark, the document title, a
//! comment, another page, or an image used twice. guessing which of those
//! the person wants gone would be wrong as often as right, so instead the
//! removed text is looked for in those places and each match is reported.

use lopdf::{Document, Object, ObjectId};

use crate::objects::{as_dict, dict_dict, dict_get, resolve, text_string};
use crate::page::page_ids;
use crate::redact::collect_glyphs;
use crate::text_layout::{lay_out, PlacedGlyph};

pub struct Leftover<'a> {
    /// the removed text, one line at a time
    pub removed_text: &'a [String],
    /// images that were changed on a redacted page but are used elsewhere too
    pub shared_images: &'a [ObjectId],
}

/// removed text shorter than this is not looked for elsewhere: a single
/// letter or two would match almost everywhere.
const MIN_SEARCH_LENGTH: usize = 4;

/// outline entries read at most, so a looping outline can't loop forever.
const MAX_OUTLINE_ENTRIES: usize = 10_000;

pub fn find_leftovers(doc: &Document, leftovers: &Leftover) -> Vec<String> {
    let mut warnings = Vec::new();
    let needles: Vec<(String, String)> = leftovers
        .removed_text
        .iter()
        .map(|text| (text.trim().to_owned(), normalise(text)))
        .filter(|(_, needle)| {
            needle.chars().filter(|c| !c.is_whitespace()).count() >= MIN_SEARCH_LENGTH
        })
        .collect();
    if !needles.is_empty() {
        let places = searchable_places(doc);
        for (shown, needle) in &needles {
            for (place, text) in &places {
                if text.contains(needle.as_str()) {
                    warnings.push(format!("“{shown}” still appears {place}."));
                }
            }
        }
    }
    warnings.extend(shared_image_warnings(doc, leftovers.shared_images));
    if has_attachments(doc) {
        warnings.push("This file has attached files, which redaction does not change. Use Remove hidden information to remove them.".to_owned());
    }
    if doc.encryption_state.is_some() {
        warnings.push("This file had permission settings. The redacted copy is saved without them unless you lock it again.".to_owned());
    }
    warnings
}

/// lower case with runs of white space made single spaces, so line breaks
/// and odd spacing do not hide a match.
fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// every place text can hide outside page content, with its text.
fn searchable_places(doc: &Document) -> Vec<(String, String)> {
    let mut places = Vec::new();
    if let Some(info) = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|info| as_dict(resolve(doc, info)))
    {
        for (key, value) in info.iter() {
            if let Object::String(bytes, _) = resolve(doc, value) {
                let key = String::from_utf8_lossy(key);
                places.push((
                    format!("in the document’s {key}"),
                    normalise(&text_string(bytes)),
                ));
            }
        }
    }
    let titles = outline_titles(doc);
    if !titles.is_empty() {
        places.push(("in a bookmark".to_owned(), normalise(&titles.join("\n"))));
    }
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let number = index + 1;
        if let Some(text) = page_text(doc, page_id) {
            places.push((format!("on page {number}"), text));
        }
        let notes = annotation_texts(doc, page_id);
        if !notes.is_empty() {
            places.push((
                format!("in a comment or form field on page {number}"),
                normalise(&notes.join("\n")),
            ));
        }
    }
    places
}

fn page_text(doc: &Document, page_id: ObjectId) -> Option<String> {
    let glyphs = collect_glyphs(doc, page_id).ok()?;
    let placed: Vec<PlacedGlyph> = glyphs
        .iter()
        .map(|g| PlacedGlyph {
            text: &g.text,
            quad: &g.quad,
        })
        .collect();
    Some(normalise(&lay_out(&placed).text))
}

fn annotation_texts(doc: &Document, page_id: ObjectId) -> Vec<String> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let Some(Object::Array(annots)) = dict_get(doc, page, b"Annots") else {
        return Vec::new();
    };
    let mut texts = Vec::new();
    for annot in annots.iter().filter_map(|a| as_dict(resolve(doc, a))) {
        for key in [b"Contents".as_slice(), b"T", b"V", b"RC"] {
            if let Some(Object::String(bytes, _)) = dict_get(doc, annot, key) {
                texts.push(text_string(bytes));
            }
        }
    }
    texts
}

fn outline_titles(doc: &Document) -> Vec<String> {
    let mut titles = Vec::new();
    let Some(outlines) = doc
        .catalog()
        .ok()
        .and_then(|c| dict_dict(doc, c, b"Outlines"))
    else {
        return titles;
    };
    let mut waiting: Vec<Object> = outlines.get(b"First").ok().cloned().into_iter().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(entry) = waiting.pop() {
        if titles.len() >= MAX_OUTLINE_ENTRIES {
            break;
        }
        if let Ok(id) = entry.as_reference() {
            if !seen.insert(id) {
                continue;
            }
        }
        let Some(item) = as_dict(resolve(doc, &entry)) else {
            continue;
        };
        if let Some(Object::String(bytes, _)) = dict_get(doc, item, b"Title") {
            titles.push(text_string(bytes));
        }
        for key in [b"Next".as_slice(), b"First"] {
            if let Ok(next) = item.get(key) {
                waiting.push(next.clone());
            }
        }
    }
    titles
}

fn shared_image_warnings(doc: &Document, images: &[ObjectId]) -> Vec<String> {
    let mut warnings = Vec::new();
    for image in images {
        for (index, page_id) in page_ids(doc).into_iter().enumerate() {
            if page_uses_object(doc, page_id, *image) {
                warnings.push(format!(
                    "An image you redacted is also used on page {}, where it was not changed.",
                    index + 1
                ));
            }
        }
    }
    warnings
}

fn page_uses_object(doc: &Document, page_id: ObjectId, target: ObjectId) -> bool {
    let resources = crate::page::page_resources(doc, page_id);
    let Some(xobjects) = dict_dict(doc, &resources, b"XObject") else {
        return false;
    };
    xobjects
        .iter()
        .any(|(_, value)| value.as_reference().ok() == Some(target))
}

fn has_attachments(doc: &Document) -> bool {
    doc.catalog()
        .ok()
        .and_then(|catalog| dict_dict(doc, catalog, b"Names"))
        .is_some_and(|names| names.has(b"EmbeddedFiles"))
}
