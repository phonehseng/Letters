//! reading and writing the outline (the bookmarks panel).
//!
//! the outline is a tree of entries stored as linked objects: each entry
//! points at its parent, its neighbours and its first and last children.
//! editing those links in place is fiddly and easy to get wrong, so the
//! outline is read into a plain tree, changed there, and written out again.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::links::{Destination, Jump, NamedDestinations};

/// limits that keep a damaged outline from looping or exploding.
const MAX_OUTLINE_DEPTH: usize = 64;
const MAX_OUTLINE_ENTRIES: usize = 20_000;

/// the keys of an entry, besides its place in the tree and its target,
/// that are kept when it is rewritten: colour, bold or italic, and an
/// action that isn't a jump within the document (such as a web link).
const STYLE_KEYS: [&[u8]; 3] = [b"C", b"F", b"A"];

/// one outline entry and the entries under it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OutlineEntry {
    /// the title as the file wrote it (a pdf text string)
    pub title: Object,
    pub target: Option<Destination>,
    pub style: Vec<(Vec<u8>, Object)>,
    /// whether its children are shown when the file opens
    pub open: bool,
    pub children: Vec<OutlineEntry>,
}

/// the document's outline as a tree. empty when it has none.
pub(crate) fn read(doc: &Document, names: &NamedDestinations) -> Vec<OutlineEntry> {
    let Some(first) = outline_root(doc).and_then(first_child) else {
        return Vec::new();
    };
    let mut reader = Reader {
        doc,
        names,
        seen: HashSet::new(),
    };
    reader.siblings(first, 0)
}

fn outline_root(doc: &Document) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    crate::links::dictionary_at(doc, catalog, b"Outlines")
}

fn first_child(entry: &Dictionary) -> Option<ObjectId> {
    entry.get(b"First").and_then(Object::as_reference).ok()
}

struct Reader<'a> {
    doc: &'a Document,
    names: &'a NamedDestinations,
    seen: HashSet<ObjectId>,
}

impl Reader<'_> {
    fn siblings(&mut self, first: ObjectId, depth: usize) -> Vec<OutlineEntry> {
        let mut entries = Vec::new();
        let mut next = Some(first);
        while let Some(id) = next {
            let too_much = depth > MAX_OUTLINE_DEPTH || self.seen.len() >= MAX_OUTLINE_ENTRIES;
            if too_much || !self.seen.insert(id) {
                break;
            }
            let Ok(dict) = self.doc.get_dictionary(id) else {
                break;
            };
            entries.push(self.entry(dict, depth));
            next = dict.get(b"Next").and_then(Object::as_reference).ok();
        }
        entries
    }

    fn entry(&mut self, dict: &Dictionary, depth: usize) -> OutlineEntry {
        let jump = self.names.jump_of(self.doc, dict);
        let target = match &jump {
            Jump::To(destination) => Some(destination.clone()),
            _ => None,
        };
        let style = STYLE_KEYS
            .iter()
            // a "go to" action is replaced by the written-out target
            .filter(|&&key| !(key == b"A" && jump != Jump::NotInternal))
            .filter_map(|&key| Some((key.to_vec(), dict.get(key).ok()?.clone())))
            .collect();
        let title = dict
            .get(b"Title")
            .ok()
            .and_then(|title| self.doc.dereference(title).ok())
            .map(|(_, title)| title.clone())
            .unwrap_or_else(|| Object::string_literal("Untitled"));
        let children = match first_child(dict) {
            Some(first) => self.siblings(first, depth + 1),
            None => Vec::new(),
        };
        let open = dict.get(b"Count").and_then(Object::as_i64).unwrap_or(0) > 0;
        OutlineEntry {
            title,
            target,
            style,
            open,
            children,
        }
    }
}

/// points entries at new pages. `new_page` gives the new page for an old
/// one, or none when that page is gone; an entry leading to a page that is
/// gone is dropped, and its children take its place so nothing under it
/// is lost.
pub(crate) fn retarget(
    entries: Vec<OutlineEntry>,
    new_page: &impl Fn(ObjectId) -> Option<ObjectId>,
) -> Vec<OutlineEntry> {
    let mut kept = Vec::new();
    for mut entry in entries {
        let children = retarget(std::mem::take(&mut entry.children), new_page);
        match entry.target.take() {
            Some(destination) => match new_page(destination.page) {
                Some(page) => {
                    entry.target = Some(Destination {
                        page,
                        view: destination.view,
                    });
                    entry.children = children;
                    kept.push(entry);
                }
                None => kept.extend(children),
            },
            None => {
                entry.children = children;
                kept.push(entry);
            }
        }
    }
    kept
}

/// every object that makes up the current outline, root included, so the
/// old outline can be removed before a new one is written.
pub(crate) fn object_ids(doc: &Document) -> Vec<ObjectId> {
    let Ok(catalog) = doc.catalog() else {
        return Vec::new();
    };
    let Ok(root) = catalog.get(b"Outlines").and_then(Object::as_reference) else {
        return Vec::new();
    };
    let mut ids = vec![root];
    let mut seen = HashSet::from([root]);
    let mut to_visit: Vec<ObjectId> = doc
        .get_dictionary(root)
        .ok()
        .and_then(first_child)
        .into_iter()
        .collect();
    while let Some(id) = to_visit.pop() {
        if seen.len() >= MAX_OUTLINE_ENTRIES || !seen.insert(id) {
            continue;
        }
        ids.push(id);
        if let Ok(dict) = doc.get_dictionary(id) {
            for key in [b"First".as_slice(), b"Next"] {
                if let Ok(next) = dict.get(key).and_then(Object::as_reference) {
                    to_visit.push(next);
                }
            }
        }
    }
    ids
}

/// writes `entries` as a new outline and returns its root, or none when
/// there is nothing to write.
pub(crate) fn write(doc: &mut Document, entries: &[OutlineEntry]) -> Option<ObjectId> {
    if entries.is_empty() {
        return None;
    }
    let root = doc.new_object_id();
    let level = write_level(doc, entries, root);
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Outlines".to_vec()));
    dict.set("First", Object::Reference(level.first));
    dict.set("Last", Object::Reference(level.last));
    dict.set("Count", Object::Integer(level.visible as i64));
    doc.objects.insert(root, Object::Dictionary(dict));
    Some(root)
}

/// what writing one level of siblings produced.
struct WrittenLevel {
    first: ObjectId,
    last: ObjectId,
    /// how many entries are shown, counting open children
    visible: usize,
}

fn write_level(doc: &mut Document, entries: &[OutlineEntry], parent: ObjectId) -> WrittenLevel {
    let ids: Vec<ObjectId> = entries.iter().map(|_| doc.new_object_id()).collect();
    let mut visible = 0;
    for (index, entry) in entries.iter().enumerate() {
        let mut dict = Dictionary::new();
        dict.set("Title", entry.title.clone());
        dict.set("Parent", Object::Reference(parent));
        if index > 0 {
            dict.set("Prev", Object::Reference(ids[index - 1]));
        }
        if let Some(&next) = ids.get(index + 1) {
            dict.set("Next", Object::Reference(next));
        }
        visible += 1;
        if !entry.children.is_empty() {
            let children = write_level(doc, &entry.children, ids[index]);
            dict.set("First", Object::Reference(children.first));
            dict.set("Last", Object::Reference(children.last));
            // a positive count means open, a negative one closed
            let count = if entry.open {
                visible += children.visible;
                children.visible as i64
            } else {
                -(entry.children.len() as i64)
            };
            dict.set("Count", Object::Integer(count));
        }
        if let Some(destination) = &entry.target {
            dict.set("Dest", destination.to_object(destination.page));
        }
        for (key, value) in &entry.style {
            dict.set(key.clone(), value.clone());
        }
        doc.objects.insert(ids[index], Object::Dictionary(dict));
    }
    WrittenLevel {
        first: ids[0],
        last: ids[ids.len() - 1],
        visible,
    }
}

/// replaces the document's outline with `entries` (or removes it when
/// there are none).
pub(crate) fn replace(doc: &mut Document, entries: &[OutlineEntry]) {
    for id in object_ids(doc) {
        doc.objects.remove(&id);
    }
    let root = write(doc, entries);
    let Ok(catalog) = doc.catalog_mut() else {
        return;
    };
    match root {
        Some(root) => catalog.set("Outlines", Object::Reference(root)),
        None => {
            catalog.remove(b"Outlines");
        }
    }
}
