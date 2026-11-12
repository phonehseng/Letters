//! copying pages from one document into another.
//!
//! a page is not one object but a web of them: its content streams, the
//! fonts and pictures they use, its annotations, and whatever those point
//! at. copying a page means copying everything it reaches, giving each
//! copied object a fresh id in the target so nothing clashes, and copying
//! each shared object only once.
//!
//! just as important is what is not copied. everything that leads back to
//! the rest of the source document is cut: the page's parent in the page
//! tree, other pages, and annotations that live on other pages (a form
//! field, for example, lists the boxes it has on every page). links whose
//! target page isn't copied are dropped, so nothing in the copy leads to
//! a page that isn't there.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::forms;
use crate::links::{Jump, NamedDestinations};
use crate::tree::{inherited_raw, INHERITABLE_KEYS};

/// object types that are part of the document's own structure, which a
/// page never needs and which would pull the whole document in.
const STRUCTURAL_TYPES: [&[u8]; 3] = [b"Pages", b"Catalog", b"Outlines"];

/// page keys that only make sense inside the source document: its place
/// in the page tree, its place in the tagged structure, and article beads.
const SOURCE_ONLY_PAGE_KEYS: [&[u8]; 3] = [b"Parent", b"StructParents", b"B"];

pub(crate) struct PageCopier<'s> {
    source: &'s Document,
    names: NamedDestinations,
    /// every page of the source
    source_pages: HashSet<ObjectId>,
    /// the copied pages: source id to target id
    page_map: HashMap<ObjectId, ObjectId>,
    /// the pages being copied, in order
    chosen: Vec<ObjectId>,
    /// annotations that sit on source pages that are not copied
    foreign_annotations: HashSet<ObjectId>,
    /// every other copied object: source id to target id
    copied: HashMap<ObjectId, ObjectId>,
    /// source objects to copy in an adjusted form instead of as they are
    overrides: HashMap<ObjectId, Object>,
    /// source objects given a target id but not written yet
    waiting: Vec<ObjectId>,
    /// copied form widgets (source ids)
    widgets: Vec<ObjectId>,
}

impl<'s> PageCopier<'s> {
    /// prepares to copy `chosen` (pages of `source`, in order) into `target`.
    /// `all_pages` is every page of the source.
    pub fn new(
        source: &'s Document,
        all_pages: &[ObjectId],
        chosen: &[ObjectId],
        target: &mut Document,
    ) -> Self {
        let page_map = chosen
            .iter()
            .map(|&page| (page, target.new_object_id()))
            .collect::<HashMap<_, _>>();
        let foreign_annotations = all_pages
            .iter()
            .filter(|page| !page_map.contains_key(page))
            .flat_map(|&page| annotation_ids(source, page))
            .collect();
        Self {
            source,
            names: NamedDestinations::read(source),
            source_pages: all_pages.iter().copied().collect(),
            page_map,
            chosen: chosen.to_vec(),
            foreign_annotations,
            copied: HashMap::new(),
            overrides: HashMap::new(),
            waiting: Vec::new(),
            widgets: Vec::new(),
        }
    }

    pub fn names(&self) -> &NamedDestinations {
        &self.names
    }

    /// the copy of a source page, if that page is being copied.
    pub fn target_page(&self, source_page: ObjectId) -> Option<ObjectId> {
        self.page_map.get(&source_page).copied()
    }

    /// copies the chosen pages and everything they use, and returns the
    /// new pages in order. the pages have no parent yet.
    pub fn copy_pages(&mut self, target: &mut Document) -> Vec<ObjectId> {
        let mut new_pages = Vec::with_capacity(self.chosen.len());
        for page in self.chosen.clone() {
            let prepared = self.prepared_page(page);
            let copy = self.copy_dictionary(target, &prepared);
            let new_page = self.page_map[&page];
            target.objects.insert(new_page, Object::Dictionary(copy));
            new_pages.push(new_page);
        }
        self.finish(target);
        new_pages
    }

    /// copies one object from the source (for example a bookmark's colour
    /// or action) under the same rules as the pages. none means it only
    /// led somewhere that isn't copied. call `finish` afterwards.
    pub fn copy_object(&mut self, target: &mut Document, object: &Object) -> Option<Object> {
        match object {
            Object::Reference(id) => self.copy_reference(target, *id).map(Object::Reference),
            Object::Array(items) => Some(Object::Array(
                items
                    .iter()
                    .filter_map(|item| self.copy_object(target, item))
                    .collect(),
            )),
            Object::Dictionary(dict) => {
                Some(Object::Dictionary(self.copy_dictionary(target, dict)))
            }
            Object::Stream(stream) => Some(Object::Stream(Stream {
                dict: self.copy_dictionary(target, &stream.dict),
                content: stream.content.clone(),
                allows_compression: stream.allows_compression,
                start_position: None,
            })),
            other => Some(other.clone()),
        }
    }

    fn copy_dictionary(&mut self, target: &mut Document, dict: &Dictionary) -> Dictionary {
        let mut copy = Dictionary::new();
        for (key, value) in dict.iter() {
            if let Some(value) = self.copy_object(target, value) {
                copy.set(key.clone(), value);
            }
        }
        copy
    }

    /// the target id for a referenced source object, giving it one (and
    /// queueing it to be written) the first time it is seen.
    fn copy_reference(&mut self, target: &mut Document, id: ObjectId) -> Option<ObjectId> {
        if let Some(&page) = self.page_map.get(&id) {
            return Some(page);
        }
        if self.source_pages.contains(&id) || self.foreign_annotations.contains(&id) {
            return None;
        }
        if let Some(&copy) = self.copied.get(&id) {
            return Some(copy);
        }
        if is_structural(self.source, id) {
            return None;
        }
        let copy = target.new_object_id();
        self.copied.insert(id, copy);
        self.waiting.push(id);
        Some(copy)
    }

    /// writes every object that has been given an id but not written yet.
    /// a list is used instead of recursion so a long chain of references
    /// can't overflow the stack.
    pub fn finish(&mut self, target: &mut Document) {
        let source = self.source;
        while let Some(id) = self.waiting.pop() {
            let copy = match self.overrides.remove(&id) {
                Some(adjusted) => self.copy_object(target, &adjusted),
                None => match source.get_object(id) {
                    Ok(object) => self.copy_object(target, object),
                    Err(_) => None,
                },
            };
            target
                .objects
                .insert(self.copied[&id], copy.unwrap_or(Object::Null));
        }
    }

    /// a source page with its inherited settings written on it, without
    /// the keys that only make sense in the source, and with its
    /// annotations prepared for the copy.
    fn prepared_page(&mut self, page: ObjectId) -> Dictionary {
        let source = self.source;
        let mut dict = source.get_dictionary(page).cloned().unwrap_or_default();
        for key in INHERITABLE_KEYS {
            if !dict.has(key) {
                if let Some(value) = inherited_raw(source, page, key) {
                    dict.set(key, value.clone());
                }
            }
        }
        for key in SOURCE_ONLY_PAGE_KEYS {
            dict.remove(key);
        }
        if let Ok(annotations) = dict.get(b"Annots") {
            let kept = self.prepared_annotations(annotations);
            dict.set("Annots", Object::Array(kept));
        }
        dict
    }

    fn prepared_annotations(&mut self, annotations: &Object) -> Vec<Object> {
        let source = self.source;
        let Ok((_, Object::Array(annotations))) = source.dereference(annotations) else {
            return Vec::new();
        };
        let mut kept = Vec::with_capacity(annotations.len());
        for annotation in annotations {
            match annotation {
                Object::Reference(id) => {
                    let Ok(dict) = source.get_dictionary(*id) else {
                        continue;
                    };
                    let Some(prepared) = self.prepared_annotation(dict) else {
                        continue;
                    };
                    if is_widget(dict) {
                        self.widgets.push(*id);
                    }
                    if &prepared != dict {
                        self.overrides.insert(*id, Object::Dictionary(prepared));
                    }
                    kept.push(annotation.clone());
                }
                Object::Dictionary(dict) => {
                    if let Some(prepared) = self.prepared_annotation(dict) {
                        kept.push(Object::Dictionary(prepared));
                    }
                }
                _ => {}
            }
        }
        kept
    }

    /// an annotation as it should be copied, or none to leave it out.
    ///
    /// a jump within the document is written out in full (so a named
    /// destination doesn't have to be copied too). a link to a page that
    /// isn't copied, or to nowhere, is left out; any other annotation with
    /// such a jump (a form button, say) keeps everything but the jump.
    fn prepared_annotation(&self, annotation: &Dictionary) -> Option<Dictionary> {
        let mut prepared = annotation.clone();
        prepared.remove(b"StructParent");
        let jump = self.names.jump_of(self.source, annotation);
        let target_is_copied = match &jump {
            Jump::NotInternal => return Some(prepared),
            Jump::To(destination) => self.page_map.contains_key(&destination.page),
            Jump::Broken => false,
        };
        prepared.remove(b"A");
        prepared.remove(b"Dest");
        match jump {
            Jump::To(destination) if target_is_copied => {
                prepared.set("Dest", destination.to_object(destination.page));
                Some(prepared)
            }
            _ if is_link(annotation) => None,
            _ => Some(prepared),
        }
    }

    /// adds the form fields of copied widgets to the target's form.
    pub fn finish_forms(&mut self, target: &mut Document) {
        let mut top_fields = Vec::new();
        for &widget in &self.widgets {
            let top = forms::top_field(self.source, widget);
            let is_field = self
                .source
                .get_dictionary(top)
                .is_ok_and(|dict| dict.has(b"T") || dict.has(b"FT"));
            if let Some(&copy) = self.copied.get(&top) {
                if is_field && !top_fields.contains(&copy) {
                    top_fields.push(copy);
                }
            }
        }
        if top_fields.is_empty() {
            return;
        }
        let raw = forms::form_defaults(self.source);
        let copy = |copier: &mut Self, target: &mut Document, value: Option<Object>| {
            value.and_then(|value| copier.copy_object(target, &value))
        };
        let defaults = forms::FormDefaults {
            default_appearance: copy(self, target, raw.default_appearance),
            default_resources: copy(self, target, raw.default_resources),
            need_appearances: copy(self, target, raw.need_appearances),
        };
        self.finish(target);
        forms::add_fields(target, &top_fields, defaults);
    }
}

fn is_widget(annotation: &Dictionary) -> bool {
    annotation.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget")
}

fn is_link(annotation: &Dictionary) -> bool {
    annotation.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Link")
}

fn is_structural(doc: &Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .and_then(Dictionary::get_type)
        .is_ok_and(|kind| STRUCTURAL_TYPES.contains(&kind))
}

/// the annotations a page lists (only those stored as their own objects).
fn annotation_ids(doc: &Document, page: ObjectId) -> Vec<ObjectId> {
    doc.get_dictionary(page)
        .ok()
        .and_then(|dict| dict.get(b"Annots").ok())
        .and_then(|annots| doc.dereference(annots).ok())
        .and_then(|(_, annots)| annots.as_array().ok())
        .map(|annots| {
            annots
                .iter()
                .filter_map(|a| a.as_reference().ok())
                .collect()
        })
        .unwrap_or_default()
}
