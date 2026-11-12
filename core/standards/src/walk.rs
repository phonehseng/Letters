//! visiting everything a document draws: each page's content, the form
//! xobjects it paints (followed through `Do`, each once), and annotation
//! appearance streams, each with the resources it draws with.

use std::collections::HashSet;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::objects::{dict_of, get, get_dict, inherited, page_ids, resolve, stream_bytes};

/// one content stream, ready to look at.
pub struct Drawing<'a> {
    /// 0-based page it belongs to
    pub page: u32,
    /// the stream's object, when it is one (page contents may be several)
    pub object: Option<ObjectId>,
    pub resources: Dictionary,
    pub operations: Vec<Operation>,
    pub doc: &'a Document,
}

/// form xobjects may nest; deeper than this is a broken or hostile file.
const MAX_NESTING: usize = 24;

/// calls `visit` for every drawing in the document.
pub fn each_drawing(doc: &Document, visit: &mut dyn FnMut(&Drawing)) {
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let page = index as u32;
        let resources = inherited(doc, page_id, b"Resources")
            .and_then(|value| value.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let content = page_content(doc, page_id);
        draw(doc, page, None, resources, &content, &mut seen, visit, 0);
        for (appearance, own_resources) in appearance_streams(doc, page_id) {
            if !seen.insert(appearance) {
                continue;
            }
            let Ok(stream) = doc.get_object(appearance).and_then(Object::as_stream) else {
                continue;
            };
            let bytes = stream_bytes(stream);
            draw(
                doc,
                page,
                Some(appearance),
                own_resources,
                &bytes,
                &mut seen,
                visit,
                0,
            );
        }
    }
}

/// the page's content streams joined, as a reader would read them.
fn page_content(doc: &Document, page_id: ObjectId) -> Vec<u8> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let parts: Vec<&Object> = match get(doc, page, b"Contents") {
        Some(Object::Array(items)) => items.iter().collect(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    let mut bytes = Vec::new();
    for part in parts {
        if let Object::Stream(stream) = resolve(doc, part) {
            bytes.extend(stream_bytes(stream));
            bytes.push(b'\n');
        }
    }
    bytes
}

#[allow(clippy::too_many_arguments)]
fn draw(
    doc: &Document,
    page: u32,
    object: Option<ObjectId>,
    resources: Dictionary,
    bytes: &[u8],
    seen: &mut HashSet<ObjectId>,
    visit: &mut dyn FnMut(&Drawing),
    depth: usize,
) {
    let operations = Content::decode(bytes)
        .map(|content| content.operations)
        .unwrap_or_default();
    let forms = painted_forms(doc, &resources, &operations);
    visit(&Drawing {
        page,
        object,
        resources,
        operations,
        doc,
    });
    if depth >= MAX_NESTING {
        return;
    }
    for form in forms {
        if !seen.insert(form) {
            continue;
        }
        let Ok(stream) = doc.get_object(form).and_then(Object::as_stream) else {
            continue;
        };
        let own = get_dict(doc, &stream.dict, b"Resources")
            .cloned()
            .unwrap_or_default();
        let bytes = stream_bytes(stream);
        draw(doc, page, Some(form), own, &bytes, seen, visit, depth + 1);
    }
}

/// the form xobjects a drawing paints with `Do`.
fn painted_forms(
    doc: &Document,
    resources: &Dictionary,
    operations: &[Operation],
) -> Vec<ObjectId> {
    let Some(xobjects) = get_dict(doc, resources, b"XObject") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for operation in operations
        .iter()
        .filter(|operation| operation.operator == "Do")
    {
        let Some(name) = operation
            .operands
            .first()
            .and_then(|operand| operand.as_name().ok())
        else {
            continue;
        };
        let Ok(Object::Reference(id)) = xobjects.get(name) else {
            continue;
        };
        let is_form = doc
            .get_object(*id)
            .ok()
            .and_then(dict_of)
            .and_then(|dict| dict.get(b"Subtype").ok())
            .and_then(|subtype| subtype.as_name().ok())
            == Some(b"Form");
        if is_form {
            out.push(*id);
        }
    }
    out
}

/// every appearance stream of the page's annotations, with its resources.
fn appearance_streams(doc: &Document, page_id: ObjectId) -> Vec<(ObjectId, Dictionary)> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for annotation in crate::objects::get_array(doc, page, b"Annots")
        .into_iter()
        .flatten()
    {
        let Some(annotation) = dict_of(resolve(doc, annotation)) else {
            continue;
        };
        let Some(appearances) = get_dict(doc, annotation, b"AP") else {
            continue;
        };
        for (_, value) in appearances.iter() {
            collect_appearance(doc, value, &mut out);
        }
    }
    out
}

/// an appearance entry is a stream, or a dictionary of streams by state.
fn collect_appearance(doc: &Document, value: &Object, out: &mut Vec<(ObjectId, Dictionary)>) {
    match value {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Stream(stream)) => {
                let resources = get_dict(doc, &stream.dict, b"Resources")
                    .cloned()
                    .unwrap_or_default();
                out.push((*id, resources));
            }
            Ok(Object::Dictionary(states)) => {
                for (_, state) in states.iter() {
                    collect_appearance(doc, state, out);
                }
            }
            _ => {}
        },
        Object::Dictionary(states) => {
            for (_, state) in states.iter() {
                if let Object::Reference(_) = state {
                    collect_appearance(doc, state, out);
                }
            }
        }
        _ => {}
    }
}
