//! keeping form fields consistent when pages come and go.
//!
//! a form field is a tree: a field can have child fields, and the leaves
//! are "widgets", the boxes drawn on pages. the document lists its top
//! level fields in /AcroForm /Fields. when a page goes, its widgets go,
//! and so must any field left without a single widget; when pages are
//! copied in, their fields must be added to the list or readers won't
//! treat them as a form.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// a field tree is a few levels deep; this stops a looping one.
const MAX_FIELD_DEPTH: usize = 32;

/// the form dictionary and where it lives.
enum FormHome {
    /// in its own object
    Object(ObjectId),
    /// written directly inside the catalog
    InCatalog(ObjectId),
}

fn catalog_id(doc: &Document) -> Option<ObjectId> {
    doc.trailer.get(b"Root").and_then(Object::as_reference).ok()
}

/// the form dictionary (a copy) and where to put it back.
fn read_form(doc: &Document) -> Option<(Dictionary, FormHome)> {
    let catalog_id = catalog_id(doc)?;
    let catalog = doc.get_dictionary(catalog_id).ok()?;
    match catalog.get(b"AcroForm").ok()? {
        Object::Reference(id) => {
            Some((doc.get_dictionary(*id).ok()?.clone(), FormHome::Object(*id)))
        }
        Object::Dictionary(dict) => Some((dict.clone(), FormHome::InCatalog(catalog_id))),
        _ => None,
    }
}

fn write_form(doc: &mut Document, form: Dictionary, home: FormHome) {
    match home {
        FormHome::Object(id) => {
            doc.objects.insert(id, Object::Dictionary(form));
        }
        FormHome::InCatalog(catalog) => {
            if let Ok(catalog) = doc.get_dictionary_mut(catalog) {
                catalog.set("AcroForm", Object::Dictionary(form));
            }
        }
    }
}

/// the references in an array stored under `key` (following a reference).
fn references_at(doc: &Document, dict: &Dictionary, key: &[u8]) -> Vec<ObjectId> {
    dict.get(key)
        .ok()
        .and_then(|value| doc.dereference(value).ok())
        .and_then(|(_, value)| value.as_array().ok())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_reference().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn reference_array(ids: &[ObjectId]) -> Object {
    Object::Array(ids.iter().map(|&id| Object::Reference(id)).collect())
}

/// takes widgets that were on deleted pages out of the field tree, and
/// drops every field that is left with no widget at all.
pub(crate) fn remove_widgets(doc: &mut Document, removed_widgets: &HashSet<ObjectId>) {
    if removed_widgets.is_empty() {
        return;
    }
    let Some((mut form, home)) = read_form(doc) else {
        return;
    };
    let mut pruner = FieldPruner {
        removed_widgets,
        kids_updates: Vec::new(),
        dropped: HashSet::new(),
    };
    let fields = references_at(doc, &form, b"Fields");
    let kept: Vec<ObjectId> = fields
        .iter()
        .copied()
        .filter(|&field| pruner.keep(doc, field, 0))
        .collect();
    if kept.len() == fields.len() && pruner.kids_updates.is_empty() {
        return;
    }
    let calculation_order: Vec<ObjectId> = references_at(doc, &form, b"CO")
        .into_iter()
        .filter(|field| !pruner.dropped.contains(field))
        .collect();
    let FieldPruner { kids_updates, .. } = pruner;
    for (field, kids) in kids_updates {
        if let Ok(dict) = doc.get_dictionary_mut(field) {
            dict.set("Kids", reference_array(&kids));
        }
    }
    form.set("Fields", reference_array(&kept));
    if form.has(b"CO") {
        form.set("CO", reference_array(&calculation_order));
    }
    write_form(doc, form, home);
}

struct FieldPruner<'a> {
    removed_widgets: &'a HashSet<ObjectId>,
    /// fields whose list of children shrank, with what is left
    kids_updates: Vec<(ObjectId, Vec<ObjectId>)>,
    dropped: HashSet<ObjectId>,
}

impl FieldPruner<'_> {
    /// whether a field (or widget) stays, after pruning its children.
    fn keep(&mut self, doc: &Document, field: ObjectId, depth: usize) -> bool {
        if self.removed_widgets.contains(&field) {
            self.dropped.insert(field);
            return false;
        }
        let Ok(dict) = doc.get_dictionary(field) else {
            return false;
        };
        if depth > MAX_FIELD_DEPTH || !dict.has(b"Kids") {
            return true;
        }
        let kids = references_at(doc, dict, b"Kids");
        let kept: Vec<ObjectId> = kids
            .iter()
            .copied()
            .filter(|&kid| self.keep(doc, kid, depth + 1))
            .collect();
        if kept.len() == kids.len() {
            return true;
        }
        if kept.is_empty() {
            self.dropped.insert(field);
            return false;
        }
        self.kids_updates.push((field, kept));
        true
    }
}

/// the top field a widget belongs to: follow /Parent as far as it goes.
pub(crate) fn top_field(doc: &Document, widget: ObjectId) -> ObjectId {
    let mut field = widget;
    for _ in 0..MAX_FIELD_DEPTH {
        match doc
            .get_dictionary(field)
            .and_then(|dict| dict.get(b"Parent"))
            .and_then(Object::as_reference)
        {
            Ok(parent) => field = parent,
            Err(_) => break,
        }
    }
    field
}

/// the form settings that come with fields copied from another document:
/// default appearance (/DA), default resources such as fonts (/DR), and
/// whether readers should redraw fields themselves.
#[derive(Default)]
pub(crate) struct FormDefaults {
    pub default_appearance: Option<Object>,
    pub default_resources: Option<Object>,
    pub need_appearances: Option<Object>,
}

/// reads the form settings of a document (raw, still referring to its objects).
pub(crate) fn form_defaults(doc: &Document) -> FormDefaults {
    let Some((form, _)) = read_form(doc) else {
        return FormDefaults::default();
    };
    FormDefaults {
        default_appearance: form.get(b"DA").ok().cloned(),
        default_resources: form.get(b"DR").ok().cloned(),
        need_appearances: form.get(b"NeedAppearances").ok().cloned(),
    }
}

/// adds copied top level fields to the document's form, creating the form
/// when the document has none. a field whose name is already taken gets a
/// number added ("name_2"), because two fields with one name are treated
/// as one field and would always show the same value.
pub(crate) fn add_fields(doc: &mut Document, fields: &[ObjectId], defaults: FormDefaults) {
    if fields.is_empty() {
        return;
    }
    let Some(catalog) = catalog_id(doc) else {
        return;
    };
    let (mut form, home) = match read_form(doc) {
        Some((form, home)) => (form, Some(home)),
        None => (Dictionary::new(), None),
    };
    let mut listed = references_at(doc, &form, b"Fields");
    let mut taken: HashSet<Vec<u8>> = listed
        .iter()
        .filter_map(|&id| field_name(doc, id))
        .collect();
    for &field in fields {
        if listed.contains(&field) {
            continue;
        }
        if let Some(name) = field_name(doc, field) {
            let unique = unique_name(&name, &taken);
            if unique != name {
                if let Ok(dict) = doc.get_dictionary_mut(field) {
                    dict.set("T", Object::string_literal(unique.clone()));
                }
            }
            taken.insert(unique);
        }
        listed.push(field);
    }
    form.set("Fields", reference_array(&listed));
    merge_defaults(doc, &mut form, defaults);
    match home {
        Some(home) => write_form(doc, form, home),
        None => {
            let form_id = doc.add_object(Object::Dictionary(form));
            if let Ok(catalog) = doc.get_dictionary_mut(catalog) {
                catalog.set("AcroForm", Object::Reference(form_id));
            }
        }
    }
}

/// fills in form settings the document lacks from the copied document's.
/// fonts named in both keep the document's own.
fn merge_defaults(doc: &Document, form: &mut Dictionary, defaults: FormDefaults) {
    if !form.has(b"DA") {
        if let Some(appearance) = defaults.default_appearance {
            form.set("DA", appearance);
        }
    }
    if !form.has(b"NeedAppearances") {
        if let Some(need) = defaults.need_appearances {
            form.set("NeedAppearances", need);
        }
    }
    let Some(theirs) = defaults.default_resources else {
        return;
    };
    let Some(ours) = form
        .get(b"DR")
        .ok()
        .and_then(|value| doc.dereference(value).ok())
        .and_then(|(_, value)| value.as_dict().ok())
        .cloned()
    else {
        form.set("DR", theirs);
        return;
    };
    let theirs_fonts = doc
        .dereference(&theirs)
        .ok()
        .and_then(|(_, theirs)| theirs.as_dict().ok())
        .and_then(|theirs| crate::links::dictionary_at(doc, theirs, b"Font"))
        .cloned()
        .unwrap_or_default();
    let mut merged = ours;
    let mut fonts = crate::links::dictionary_at(doc, &merged, b"Font")
        .cloned()
        .unwrap_or_default();
    for (name, font) in theirs_fonts.iter() {
        if !fonts.has(name) {
            fonts.set(name.clone(), font.clone());
        }
    }
    merged.set("Font", fonts);
    form.set("DR", merged);
}

/// a field's own name (/T), as raw bytes.
fn field_name(doc: &Document, field: ObjectId) -> Option<Vec<u8>> {
    let dict = doc.get_dictionary(field).ok()?;
    let (_, name) = doc.dereference(dict.get(b"T").ok()?).ok()?;
    name.as_str().ok().map(<[u8]>::to_vec)
}

/// `name`, or `name_2`, `name_3`... whichever isn't taken. names stored as
/// utf-16 get the suffix in utf-16 too.
fn unique_name(name: &[u8], taken: &HashSet<Vec<u8>>) -> Vec<u8> {
    if !taken.contains(name) {
        return name.to_vec();
    }
    let utf16 = name.starts_with(&[0xFE, 0xFF]);
    (2..)
        .map(|number| {
            let suffix = format!("_{number}");
            let mut candidate = name.to_vec();
            if utf16 {
                for unit in suffix.encode_utf16() {
                    candidate.extend_from_slice(&unit.to_be_bytes());
                }
            } else {
                candidate.extend_from_slice(suffix.as_bytes());
            }
            candidate
        })
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_names_get_a_number() {
        let taken: HashSet<Vec<u8>> = [b"name".to_vec(), b"name_2".to_vec()].into();
        assert_eq!(unique_name(b"name", &taken), b"name_3");
        assert_eq!(unique_name(b"other", &taken), b"other");
    }
}
