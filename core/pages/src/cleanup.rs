//! tidying up after pages are deleted, so nothing points at a page that
//! is gone.
//!
//! a deleted page can still be named by bookmarks, by links on other
//! pages, by named destinations, by the "open at" setting and by form
//! fields. each of those is updated so the file stays valid and every
//! remaining link still leads to the right page.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::links::{name_tree_entries, Jump, NamedDestinations};
use crate::{forms, outline};

/// updates everything that referred to `removed`, which have already been
/// taken out of the page tree, then drops the pages and anything only
/// they used.
pub(crate) fn forget_pages(doc: &mut Document, removed: &HashSet<ObjectId>, kept: &[ObjectId]) {
    let names = NamedDestinations::read(doc);
    let is_gone = |page: ObjectId| removed.contains(&page);
    let removed_annotations = annotations_on(doc, removed.iter().copied());

    update_outline(doc, &names, &is_gone);
    update_links(doc, &names, kept, &is_gone);
    drop_named_destinations(doc, &names, &is_gone);
    drop_open_action(doc, &names, &is_gone);
    forms::remove_widgets(doc, &removed_annotations);

    let gone: HashSet<ObjectId> = removed.union(&removed_annotations).copied().collect();
    for id in &gone {
        doc.objects.remove(id);
    }
    for object in doc.objects.values_mut() {
        strip_references(object, &gone);
    }
    // content streams, fonts and pictures only the deleted pages used
    doc.prune_objects();
}

/// the annotations listed on the given pages.
fn annotations_on(doc: &Document, pages: impl Iterator<Item = ObjectId>) -> HashSet<ObjectId> {
    pages
        .flat_map(|page| annotation_list(doc, page))
        .filter_map(|annotation| annotation.as_reference().ok())
        .collect()
}

/// a page's /Annots entries, following a reference to the array.
fn annotation_list(doc: &Document, page: ObjectId) -> Vec<Object> {
    doc.get_dictionary(page)
        .ok()
        .and_then(|dict| dict.get(b"Annots").ok())
        .and_then(|annots| doc.dereference(annots).ok())
        .and_then(|(_, annots)| annots.as_array().ok())
        .cloned()
        .unwrap_or_default()
}

fn update_outline(
    doc: &mut Document,
    names: &NamedDestinations,
    is_gone: &impl Fn(ObjectId) -> bool,
) {
    let before = outline::read(doc, names);
    let after = outline::retarget(before.clone(), &|page| (!is_gone(page)).then_some(page));
    if after != before {
        outline::replace(doc, &after);
    }
}

/// what to do with one annotation on a page that stays.
enum LinkFix {
    Keep,
    /// a link to a deleted page: take it off the page
    Drop,
    /// another kind of annotation (a form button, say) that jumps to a
    /// deleted page: keep it, without the jump
    RemoveJump(ObjectId),
}

fn link_fix(
    doc: &Document,
    names: &NamedDestinations,
    annotation: &Object,
    is_gone: &impl Fn(ObjectId) -> bool,
) -> LinkFix {
    let Ok((id, Object::Dictionary(dict))) = doc.dereference(annotation) else {
        return LinkFix::Keep;
    };
    let Jump::To(destination) = names.jump_of(doc, dict) else {
        return LinkFix::Keep;
    };
    if !is_gone(destination.page) {
        return LinkFix::Keep;
    }
    let is_link = dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Link");
    match id {
        Some(id) if !is_link => LinkFix::RemoveJump(id),
        _ => LinkFix::Drop,
    }
}

fn update_links(
    doc: &mut Document,
    names: &NamedDestinations,
    kept: &[ObjectId],
    is_gone: &impl Fn(ObjectId) -> bool,
) {
    let mut new_lists = Vec::new();
    let mut jumps_to_remove = Vec::new();
    for &page in kept {
        let annotations = annotation_list(doc, page);
        let mut remaining = Vec::with_capacity(annotations.len());
        for annotation in &annotations {
            match link_fix(doc, names, annotation, is_gone) {
                LinkFix::Keep => remaining.push(annotation.clone()),
                LinkFix::Drop => {}
                LinkFix::RemoveJump(id) => {
                    jumps_to_remove.push(id);
                    remaining.push(annotation.clone());
                }
            }
        }
        if remaining.len() != annotations.len() {
            new_lists.push((page, remaining));
        }
    }
    for (page, remaining) in new_lists {
        if let Ok(dict) = doc.get_dictionary_mut(page) {
            // the page gets its own list, as the old one may be shared
            dict.set("Annots", Object::Array(remaining));
        }
    }
    for id in jumps_to_remove {
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            dict.remove(b"Dest");
            dict.remove(b"A");
        }
    }
}

/// runs `edit` on a dictionary stored in the catalog under `key`, whether
/// it is written inside the catalog or kept in its own object.
fn edit_catalog_entry(doc: &mut Document, key: &[u8], edit: impl FnOnce(&mut Dictionary)) {
    let Ok(catalog) = doc.catalog() else {
        return;
    };
    match catalog.get(key) {
        Ok(Object::Reference(id)) => {
            let id = *id;
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                edit(dict);
            }
        }
        Ok(Object::Dictionary(_)) => {
            if let Ok(Object::Dictionary(dict)) =
                doc.catalog_mut().and_then(|catalog| catalog.get_mut(key))
            {
                edit(dict);
            }
        }
        _ => {}
    }
}

/// removes named destinations that lead to deleted pages.
fn drop_named_destinations(
    doc: &mut Document,
    names: &NamedDestinations,
    is_gone: &impl Fn(ObjectId) -> bool,
) {
    if names.is_empty() {
        return;
    }
    let doomed: HashSet<Vec<u8>> = names
        .entries()
        .filter(|(_, dest)| matches!(names.resolve(doc, dest), Some(d) if is_gone(d.page)))
        .map(|(name, _)| name.clone())
        .collect();
    if doomed.is_empty() {
        return;
    }
    edit_catalog_entry(doc, b"Dests", |dests| {
        for name in &doomed {
            dests.remove(name);
        }
    });
    let tree_entries: Option<Vec<(Vec<u8>, Object)>> = doc
        .catalog()
        .ok()
        .and_then(|catalog| crate::links::dictionary_at(doc, catalog, b"Names"))
        .and_then(|names| names.get(b"Dests").ok())
        .map(|tree| name_tree_entries(doc, tree));
    if let Some(mut entries) = tree_entries {
        entries.retain(|(name, _)| !doomed.contains(name));
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        // the remaining names are written as one flat leaf, which is a
        // valid name tree of any size
        let flat: Vec<Object> = entries
            .into_iter()
            .flat_map(|(name, dest)| [Object::string_literal(name), dest])
            .collect();
        let mut leaf = Dictionary::new();
        leaf.set("Names", Object::Array(flat));
        edit_catalog_entry(doc, b"Names", |names| {
            names.set("Dests", Object::Dictionary(leaf))
        });
    }
}

/// removes the "open at this page" setting when that page is gone.
fn drop_open_action(
    doc: &mut Document,
    names: &NamedDestinations,
    is_gone: &impl Fn(ObjectId) -> bool,
) {
    let Ok(catalog) = doc.catalog() else {
        return;
    };
    let Ok(action) = catalog.get(b"OpenAction") else {
        return;
    };
    let leads_to_gone_page = match doc.dereference(action) {
        Ok((_, Object::Array(_))) => names.resolve(doc, action),
        Ok((_, Object::Dictionary(dict))) => {
            let mut holder = Dictionary::new();
            holder.set("A", Object::Dictionary(dict.clone()));
            match names.jump_of(doc, &holder) {
                Jump::To(destination) => Some(destination),
                _ => None,
            }
        }
        _ => None,
    }
    .is_some_and(|destination| is_gone(destination.page));
    if leads_to_gone_page {
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"OpenAction");
        }
    }
}

/// removes every reference to a `gone` object from an object: from an
/// array the element goes, from a dictionary the key does. used last, for
/// anything the steps above don't know about (such as the tagged
/// structure of the page), so no reference dangles.
fn strip_references(object: &mut Object, gone: &HashSet<ObjectId>) {
    let is_gone = |item: &Object| matches!(item, Object::Reference(id) if gone.contains(id));
    match object {
        Object::Array(items) => {
            items.retain(|item| !is_gone(item));
            for item in items {
                strip_references(item, gone);
            }
        }
        Object::Dictionary(dict) => strip_dictionary(dict, gone),
        Object::Stream(stream) => strip_dictionary(&mut stream.dict, gone),
        _ => {}
    }
}

fn strip_dictionary(dict: &mut Dictionary, gone: &HashSet<ObjectId>) {
    let doomed: Vec<Vec<u8>> = dict
        .iter()
        .filter(|(_, value)| matches!(value, Object::Reference(id) if gone.contains(id)))
        .map(|(key, _)| key.clone())
        .collect();
    for key in doomed {
        dict.remove(&key);
    }
    for (_, value) in dict.iter_mut() {
        strip_references(value, gone);
    }
}
