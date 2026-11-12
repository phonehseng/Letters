//! whether a form calculates answers by itself.
//!
//! forms made in acrobat can carry javascript that adds up totals or
//! copies one answer into another field. Letters never runs scripts (a
//! pdf's scripts are a security risk, and the engine has no interpreter),
//! so the person must be told when a form expects them: the totals will
//! not fill themselves in.
//!
//! a form calculates when its /AcroForm has a calculation order (/CO) or
//! any field has a calculate action (/AA /C). date formatting scripts
//! don't count: the engine understands those patterns itself.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// fields nested deeper than this are not looked at. real forms are a
/// few levels deep; the limit only guards against a damaged tree.
const MAX_DEPTH: usize = 32;

/// true when the form has automatic calculations that Letters doesn't run.
pub fn has_calculations(document: &Document) -> bool {
    let Some(form) = acroform(document) else {
        return false;
    };
    if has_calculation_order(document, form) {
        return true;
    }
    let Some(fields) = form.get(b"Fields").ok().and_then(|f| as_array(document, f)) else {
        return false;
    };
    let mut seen = HashSet::new();
    fields
        .iter()
        .any(|field| calculates(document, field, 0, &mut seen))
}

fn acroform(document: &Document) -> Option<&Dictionary> {
    let catalog = document.catalog().ok()?;
    as_dictionary(document, catalog.get(b"AcroForm").ok()?)
}

fn has_calculation_order(document: &Document, form: &Dictionary) -> bool {
    form.get(b"CO")
        .ok()
        .and_then(|order| as_array(document, order))
        .is_some_and(|order| !order.is_empty())
}

/// whether a field, or any field below it, has a calculate action.
fn calculates(
    document: &Document,
    field: &Object,
    depth: usize,
    seen: &mut HashSet<ObjectId>,
) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if let Object::Reference(id) = field {
        // a damaged file can point a field back at its parent
        if !seen.insert(*id) {
            return false;
        }
    }
    let Some(dict) = as_dictionary(document, field) else {
        return false;
    };
    let has_calculate = dict
        .get(b"AA")
        .ok()
        .and_then(|actions| as_dictionary(document, actions))
        .is_some_and(|actions| actions.has(b"C"));
    if has_calculate {
        return true;
    }
    dict.get(b"Kids")
        .ok()
        .and_then(|kids| as_array(document, kids))
        .is_some_and(|kids| {
            kids.iter()
                .any(|kid| calculates(document, kid, depth + 1, seen))
        })
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}

fn as_dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(document, object)?.as_dict().ok()
}

fn as_array<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Vec<Object>> {
    resolve(document, object)?.as_array().ok()
}
