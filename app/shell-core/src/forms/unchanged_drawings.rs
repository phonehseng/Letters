//! filling a field without touching the fields it didn't change.
//!
//! core/forms' fill_field works every calculation out again and draws
//! every formatted field again, each time, as new appearance streams. for
//! an unsigned document that costs nothing. for a signed one every save is
//! an incremental update (see signed.rs), so those new drawings are saved
//! as changes to fields nobody touched, and a verifier reports a locked or
//! certified field as changed (measured in the app: filling Name after
//! certifying listed Travel, Meals and Total as changed, not allowed).
//!
//! so after filling, a widget whose new drawing is byte for byte the one it
//! had gets its old drawing back, and the new copy is dropped. a field
//! whose look really changed (the filled one, a total that moved) keeps it.

use std::collections::BTreeSet;

use letters_forms::{fill_field, FieldValue, FillReport, FormResult};
use lopdf::{Dictionary, Document, Object, ObjectId};

/// fills a field like core/forms' `fill_field`, then puts back every
/// drawing that came out exactly as it was.
pub fn fill_field_keeping_drawings(
    document: &mut Document,
    name: &str,
    value: &FieldValue,
) -> FormResult<FillReport> {
    let before = document.clone();
    let report = fill_field(document, name, value)?;
    keep_unchanged_drawings(&before, document);
    Ok(report)
}

/// puts back the old /AP of every widget whose only change is a drawing
/// equal to the old one, and removes the new drawings that leaves unused.
pub fn keep_unchanged_drawings(before: &Document, after: &mut Document) {
    let mut dropped: BTreeSet<ObjectId> = BTreeSet::new();
    for (id, old) in &before.objects {
        let Ok(old_widget) = old.as_dict() else {
            continue;
        };
        let Some(Ok(new_widget)) = after.objects.get(id).map(Object::as_dict) else {
            continue;
        };
        if old_widget == new_widget || !only_drawing_differs(old_widget, new_widget) {
            continue;
        }
        let (Ok(old_ap), Ok(new_ap)) = (old_widget.get(b"AP"), new_widget.get(b"AP")) else {
            continue;
        };
        if same_drawing(before, old_ap, after, new_ap) {
            references_in(new_ap, &mut dropped);
            after.objects.insert(*id, old.clone());
        }
    }
    for id in dropped {
        // only drawings this fill made: anything the file had before stays
        if !before.objects.contains_key(&id) {
            after.objects.remove(&id);
        }
    }
}

/// the two widget dictionaries are the same apart from /AP.
fn only_drawing_differs(old: &Dictionary, new: &Dictionary) -> bool {
    let mut old = old.clone();
    let mut new = new.clone();
    old.remove(b"AP");
    new.remove(b"AP");
    old == new
}

/// two appearance entries draw the same: the same keys, and every stream
/// they lead to has the same dictionary (apart from /Length) and bytes.
fn same_drawing(before: &Document, old: &Object, after: &Document, new: &Object) -> bool {
    match (resolve(before, old), resolve(after, new)) {
        (Some(Object::Stream(old)), Some(Object::Stream(new))) => {
            let mut old_dict = old.dict.clone();
            let mut new_dict = new.dict.clone();
            old_dict.remove(b"Length");
            new_dict.remove(b"Length");
            old_dict == new_dict && old.content == new.content
        }
        (Some(Object::Dictionary(old)), Some(Object::Dictionary(new))) => {
            old.len() == new.len()
                && old.iter().all(|(key, value)| {
                    new.get(key)
                        .is_ok_and(|other| same_drawing(before, value, after, other))
                })
        }
        (Some(old), Some(new)) => old == new,
        _ => false,
    }
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.objects.get(id),
        other => Some(other),
    }
}

/// every object an appearance entry points at directly.
fn references_in(object: &Object, found: &mut BTreeSet<ObjectId>) {
    match object {
        Object::Reference(id) => {
            found.insert(*id);
        }
        Object::Dictionary(dict) => dict
            .iter()
            .for_each(|(_, value)| references_in(value, found)),
        Object::Array(items) => items.iter().for_each(|item| references_in(item, found)),
        _ => {}
    }
}
