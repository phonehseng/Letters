//! removing hidden information: things a pdf carries that the page does
//! not show, such as who wrote it, attached files, scripts, hidden layers
//! and private data of the program that made it.
//!
//! each kind of information is a separate item that can be chosen on its
//! own, and `scrub_preview` says what each item would remove before
//! anything is changed. a scrubbed document must be saved with
//! `save_clean`, which also drops every object nothing uses any more.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use serde::{Deserialize, Serialize};

use crate::content::{self, Op};
use crate::error::RedactError;
use crate::objects::{
    as_dict, as_dict_mut, decoded_content, dict_dict, flate_stream, reference_counts, resolve,
    text_string,
};
use crate::page::{page_ids, page_resources};
use crate::redact::ensure_unlocked;

/// one kind of hidden information.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrubItem {
    /// title, author, subject, keywords, the program used, dates
    DocumentInfo,
    /// xmp metadata, for the document and for pages and images
    XmpMetadata,
    /// files attached to the document or to comments
    Attachments,
    /// scripts and actions that open programs or send data
    JavaScript,
    /// layers that are switched off, with everything drawn on them
    HiddenLayers,
    /// comments, highlights, stamps and drawings (not links or form fields)
    Comments,
    /// what was typed into form fields
    FormValues,
    /// the small page pictures some programs store
    Thumbnails,
    /// /PieceInfo: data the program that made the file kept for itself
    PieceInfo,
    /// other private data of drawing programs (Illustrator and similar)
    PrivateData,
    /// objects nothing in the document uses any more
    UnusedObjects,
    /// links that open web pages, other files or programs
    ExternalLinks,
    /// the file's identifier and the ids in its metadata, which can link
    /// copies of the file to each other
    Fingerprints,
}

impl ScrubItem {
    pub const ALL: [ScrubItem; 13] = [
        ScrubItem::DocumentInfo,
        ScrubItem::XmpMetadata,
        ScrubItem::Attachments,
        ScrubItem::JavaScript,
        ScrubItem::HiddenLayers,
        ScrubItem::Comments,
        ScrubItem::FormValues,
        ScrubItem::Thumbnails,
        ScrubItem::PieceInfo,
        ScrubItem::PrivateData,
        ScrubItem::UnusedObjects,
        ScrubItem::ExternalLinks,
        ScrubItem::Fingerprints,
    ];
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScrubOptions {
    pub items: Vec<ScrubItem>,
}

/// what one item found (in a preview) or removed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubFinding {
    pub item: ScrubItem,
    pub count: usize,
    /// what was found, for people: "Author: Jane Doe", "notes.docx"
    pub details: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubReport {
    pub findings: Vec<ScrubFinding>,
}

/// what every item would remove. it runs the same steps as `scrub` on a
/// copy of the document, so the preview cannot disagree with the result.
pub fn scrub_preview(doc: &Document) -> Result<ScrubReport, RedactError> {
    let mut copy = doc.clone();
    scrub(
        &mut copy,
        &ScrubOptions {
            items: ScrubItem::ALL.to_vec(),
        },
    )
}

/// removes the chosen items. hidden layers are worked out first, because
/// they are the only item that can fail (a page that can't be read).
pub fn scrub(doc: &mut Document, options: &ScrubOptions) -> Result<ScrubReport, RedactError> {
    ensure_unlocked(doc)?;
    let chosen: HashSet<ScrubItem> = options.items.iter().copied().collect();
    let mut report = ScrubReport::default();
    if chosen.contains(&ScrubItem::HiddenLayers) {
        report.findings.push(hidden_layers(doc)?);
    }
    for item in ScrubItem::ALL {
        if !chosen.contains(&item) || item == ScrubItem::HiddenLayers {
            continue;
        }
        let finding = match item {
            ScrubItem::DocumentInfo => document_info(doc),
            ScrubItem::XmpMetadata => xmp_metadata(doc),
            ScrubItem::Attachments => attachments(doc),
            ScrubItem::JavaScript => scripts(doc),
            ScrubItem::Comments => comments(doc),
            ScrubItem::FormValues => form_values(doc),
            ScrubItem::Thumbnails => keys_everywhere(doc, item, &[b"Thumb"]),
            ScrubItem::PieceInfo => keys_everywhere(doc, item, &[b"PieceInfo"]),
            ScrubItem::PrivateData => private_data(doc),
            ScrubItem::ExternalLinks => external_links(doc),
            ScrubItem::Fingerprints => fingerprints(doc),
            ScrubItem::UnusedObjects | ScrubItem::HiddenLayers => continue,
        };
        report.findings.push(finding);
    }
    // unused objects go last, so everything the other items freed goes too
    if chosen.contains(&ScrubItem::UnusedObjects) {
        report.findings.push(unused_objects(doc));
    }
    Ok(report)
}

fn finding(item: ScrubItem, details: Vec<String>) -> ScrubFinding {
    ScrubFinding {
        item,
        count: details.len(),
        details,
    }
}

fn describe_value(doc: &Document, value: &Object) -> String {
    match resolve(doc, value) {
        Object::String(bytes, _) => text_string(bytes),
        Object::Name(name) => String::from_utf8_lossy(name).into_owned(),
        Object::Integer(number) => number.to_string(),
        Object::Real(number) => number.to_string(),
        Object::Boolean(flag) => flag.to_string(),
        _ => "…".to_owned(),
    }
}

fn document_info(doc: &mut Document) -> ScrubFinding {
    let details = match doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|info| as_dict(resolve(doc, info)))
    {
        Some(info) => info
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}: {}",
                    String::from_utf8_lossy(key),
                    describe_value(doc, value)
                )
            })
            .collect(),
        None => Vec::new(),
    };
    if let Some(id) = doc
        .trailer
        .remove(b"Info")
        .and_then(|info| info.as_reference().ok())
    {
        doc.objects.insert(id, Object::Null);
    }
    finding(ScrubItem::DocumentInfo, details)
}

/// removes a key from every dictionary in the file (also from
/// dictionaries written inside others) and lists where it was.
fn remove_keys(doc: &mut Document, keys: &[&[u8]]) -> usize {
    let mut removed = 0;
    for object in doc.objects.values_mut() {
        removed += remove_keys_in(object, keys, 0);
    }
    removed
}

fn remove_keys_in(object: &mut Object, keys: &[&[u8]], depth: usize) -> usize {
    if depth > 64 {
        return 0;
    }
    let mut removed = 0;
    if let Some(dict) = as_dict_mut(object) {
        for key in keys {
            if dict.remove(key).is_some() {
                removed += 1;
            }
        }
        for (_, value) in dict.iter_mut() {
            removed += remove_keys_in(value, keys, depth + 1);
        }
    } else if let Object::Array(items) = object {
        for item in items {
            removed += remove_keys_in(item, keys, depth + 1);
        }
    }
    removed
}

fn keys_everywhere(doc: &mut Document, item: ScrubItem, keys: &[&[u8]]) -> ScrubFinding {
    let count = remove_keys(doc, keys);
    let label = match item {
        ScrubItem::Thumbnails => "page thumbnail",
        ScrubItem::PieceInfo => "block of private program data",
        _ => "item",
    };
    finding(item, (0..count).map(|_| label.to_owned()).collect())
}

fn xmp_metadata(doc: &mut Document) -> ScrubFinding {
    let had_document_metadata = doc.catalog().is_ok_and(|catalog| catalog.has(b"Metadata"));
    let count = remove_keys(doc, &[b"Metadata"]);
    let mut details = Vec::new();
    if had_document_metadata {
        details.push("the document’s metadata".to_owned());
    }
    let others = count - usize::from(had_document_metadata);
    details.extend((0..others).map(|_| "metadata of a page, image or font".to_owned()));
    finding(ScrubItem::XmpMetadata, details)
}

fn attachments(doc: &mut Document) -> ScrubFinding {
    let mut details = embedded_file_names(doc);
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Collection");
        if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
            names.remove(b"EmbeddedFiles");
        }
    }
    if let Some(names_id) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Names").ok())
        .and_then(|n| n.as_reference().ok())
    {
        if let Ok(names) = doc.get_dictionary_mut(names_id) {
            names.remove(b"EmbeddedFiles");
        }
    }
    // associated files (PDF 2.0) can hang off any object
    let associated = remove_keys(doc, &[b"AF"]);
    details.extend((0..associated).map(|_| "an associated file".to_owned()));
    let removed = remove_annotations_where(doc, |doc, annot| {
        subtype(doc, annot) == Some(b"FileAttachment")
    });
    details.extend(
        removed
            .into_iter()
            .map(|_| "a file attached to a comment".to_owned()),
    );
    finding(ScrubItem::Attachments, details)
}

fn embedded_file_names(doc: &Document) -> Vec<String> {
    let tree = doc
        .catalog()
        .ok()
        .and_then(|catalog| dict_dict(doc, catalog, b"Names"))
        .and_then(|names| dict_dict(doc, names, b"EmbeddedFiles"));
    let mut names = Vec::new();
    if let Some(tree) = tree {
        collect_name_tree_keys(doc, tree, &mut names, 0);
    }
    names
}

fn collect_name_tree_keys(
    doc: &Document,
    node: &Dictionary,
    names: &mut Vec<String>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    if let Some(Object::Array(pairs)) = node.get(b"Names").ok().map(|o| resolve(doc, o)) {
        for key in pairs.iter().step_by(2) {
            if let Object::String(bytes, _) = resolve(doc, key) {
                names.push(text_string(bytes));
            }
        }
    }
    if let Some(Object::Array(kids)) = node.get(b"Kids").ok().map(|o| resolve(doc, o)) {
        for kid in kids {
            if let Some(kid) = as_dict(resolve(doc, kid)) {
                collect_name_tree_keys(doc, kid, names, depth + 1);
            }
        }
    }
}

fn subtype<'a>(doc: &'a Document, annot: &'a Dictionary) -> Option<&'a [u8]> {
    annot
        .get(b"Subtype")
        .ok()
        .and_then(|o| resolve(doc, o).as_name().ok())
}

/// removes the annotations a test picks, on every page, with their
/// popups, and empties the removed objects. returns their descriptions.
fn remove_annotations_where(
    doc: &mut Document,
    picks: impl Fn(&Document, &Dictionary) -> bool,
) -> Vec<String> {
    let mut removed = Vec::new();
    for page_id in page_ids(doc) {
        let annots: Vec<Object> = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|annots| resolve(doc, annots).as_array().ok().cloned())
            .unwrap_or_default();
        let mut doomed: HashSet<ObjectId> = HashSet::new();
        let mut keep = Vec::new();
        for entry in &annots {
            let picked = as_dict(resolve(doc, entry)).is_some_and(|annot| picks(doc, annot));
            if picked {
                let annot = as_dict(resolve(doc, entry)).cloned().unwrap_or_default();
                removed.push(describe_annotation(doc, &annot));
                if let Ok(id) = entry.as_reference() {
                    doomed.insert(id);
                }
                if let Ok(popup) = annot.get(b"Popup").and_then(Object::as_reference) {
                    doomed.insert(popup);
                }
            } else {
                keep.push(entry.clone());
            }
        }
        if removed.is_empty() && doomed.is_empty() {
            continue;
        }
        keep.retain(|entry| {
            entry
                .as_reference()
                .map_or(true, |id| !doomed.contains(&id))
        });
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            page.set("Annots", Object::Array(keep));
        }
        for id in doomed {
            doc.objects.insert(id, Object::Null);
        }
    }
    removed
}

fn describe_annotation(doc: &Document, annot: &Dictionary) -> String {
    let kind = String::from_utf8_lossy(subtype(doc, annot).unwrap_or(b"Annotation")).into_owned();
    match annot.get(b"Contents").ok().map(|o| resolve(doc, o)) {
        Some(Object::String(bytes, _)) if !bytes.is_empty() => {
            format!("{kind}: {}", text_string(bytes))
        }
        _ => kind,
    }
}

/// action types that run code, open programs or send data out.
const RISKY_ACTIONS: [&[u8]; 6] = [
    b"JavaScript",
    b"Launch",
    b"SubmitForm",
    b"ImportData",
    b"Rendition",
    b"RichMediaExecute",
];

fn scripts(doc: &mut Document) -> ScrubFinding {
    let mut details = Vec::new();
    let named_scripts = doc
        .catalog()
        .ok()
        .and_then(|catalog| dict_dict(doc, catalog, b"Names"))
        .is_some_and(|names| names.has(b"JavaScript"));
    if named_scripts {
        details.push("document scripts".to_owned());
    }
    if let Ok(catalog) = doc.catalog_mut() {
        if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
            names.remove(b"JavaScript");
        }
    }
    if let Some(names_id) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Names").ok())
        .and_then(|n| n.as_reference().ok())
    {
        if let Ok(names) = doc.get_dictionary_mut(names_id) {
            names.remove(b"JavaScript");
        }
    }
    let risky_ids: HashSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| as_dict(object).is_some_and(|dict| is_risky_action(doc, dict, 0)))
        .map(|(id, _)| *id)
        .collect();
    let triggers = remove_keys(doc, &[b"AA"]);
    details.extend(
        (0..triggers)
            .map(|_| "actions that run when something is opened, clicked or changed".to_owned()),
    );
    let mut removed = 0;
    for object in doc.objects.values_mut() {
        removed += strip_risky_actions(object, &risky_ids, 0);
    }
    details.extend((0..removed).map(|_| "a script or an action that opens a program".to_owned()));
    if remove_xfa(doc) {
        details.push("the XFA form, which can hold scripts".to_owned());
    }
    finding(ScrubItem::JavaScript, details)
}

fn is_risky_action(doc: &Document, action: &Dictionary, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let kind = action
        .get(b"S")
        .ok()
        .and_then(|s| resolve(doc, s).as_name().ok());
    if kind.is_some_and(|kind| RISKY_ACTIONS.contains(&kind)) {
        return true;
    }
    // an action can be followed by more actions (/Next)
    match action.get(b"Next").ok().map(|n| resolve(doc, n)) {
        Some(Object::Dictionary(next)) => is_risky_action(doc, next, depth + 1),
        Some(Object::Array(nexts)) => nexts
            .iter()
            .filter_map(|next| as_dict(resolve(doc, next)))
            .any(|next| is_risky_action(doc, next, depth + 1)),
        _ => false,
    }
}

/// removes /A and /OpenAction entries that hold or point at a risky action.
fn strip_risky_actions(object: &mut Object, risky_ids: &HashSet<ObjectId>, depth: usize) -> usize {
    if depth > 64 {
        return 0;
    }
    let mut removed = 0;
    if let Some(dict) = as_dict_mut(object) {
        for key in [b"A".as_slice(), b"OpenAction"] {
            let risky = match dict.get(key) {
                Ok(Object::Reference(id)) => risky_ids.contains(id),
                Ok(Object::Dictionary(action)) => is_direct_risky(action, risky_ids, 0),
                _ => false,
            };
            if risky {
                dict.remove(key);
                removed += 1;
            }
        }
        for (_, value) in dict.iter_mut() {
            removed += strip_risky_actions(value, risky_ids, depth + 1);
        }
    } else if let Object::Array(items) = object {
        for item in items {
            removed += strip_risky_actions(item, risky_ids, depth + 1);
        }
    }
    removed
}

/// the same test as `is_risky_action` for an action written in place,
/// where the document can't be borrowed: its /Next may point at a risky
/// action object found earlier.
fn is_direct_risky(action: &Dictionary, risky_ids: &HashSet<ObjectId>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let kind = action.get(b"S").ok().and_then(|s| s.as_name().ok());
    if kind.is_some_and(|kind| RISKY_ACTIONS.contains(&kind)) {
        return true;
    }
    match action.get(b"Next") {
        Ok(Object::Reference(id)) => risky_ids.contains(id),
        Ok(Object::Dictionary(next)) => is_direct_risky(next, risky_ids, depth + 1),
        Ok(Object::Array(nexts)) => nexts.iter().any(|next| match next {
            Object::Reference(id) => risky_ids.contains(id),
            Object::Dictionary(next) => is_direct_risky(next, risky_ids, depth + 1),
            _ => false,
        }),
        _ => false,
    }
}

fn remove_xfa(doc: &mut Document) -> bool {
    let mut removed = false;
    let form_id = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|f| f.as_reference().ok());
    if let Ok(catalog) = doc.catalog_mut() {
        if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
            removed |= form.remove(b"XFA").is_some();
        }
    }
    if let Some(form) = form_id
        .and_then(|id| doc.objects.get_mut(&id))
        .and_then(as_dict_mut)
    {
        removed |= form.remove(b"XFA").is_some();
    }
    removed
}

/// annotation types that are comments or markup rather than part of how
/// the document works.
const COMMENT_TYPES: [&[u8]; 18] = [
    b"Text",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Caret",
    b"Stamp",
    b"Ink",
    b"Sound",
    b"FileAttachment",
    b"Redact",
    b"Popup",
];

fn comments(doc: &mut Document) -> ScrubFinding {
    let removed = remove_annotations_where(doc, |doc, annot| {
        subtype(doc, annot).is_some_and(|kind| COMMENT_TYPES.contains(&kind))
    });
    finding(
        ScrubItem::Comments,
        removed
            .into_iter()
            .filter(|text| !text.starts_with("Popup"))
            .collect(),
    )
}

fn form_values(doc: &mut Document) -> ScrubFinding {
    let mut details = Vec::new();
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut needs_appearances = false;
    for id in ids {
        let Some(dict) = doc.objects.get(&id).and_then(as_dict) else {
            continue;
        };
        let is_field = dict.has(b"FT")
            || dict.has(b"T")
            || dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Widget");
        if !is_field {
            continue;
        }
        let name = dict
            .get(b"T")
            .ok()
            .map(|t| describe_value(doc, t))
            .unwrap_or_else(|| "a field".to_owned());
        let value = dict.get(b"V").ok().map(|v| describe_value(doc, v));
        let field_type = field_type(doc, dict);
        let Some(dict) = doc.objects.get_mut(&id).and_then(as_dict_mut) else {
            continue;
        };
        let mut removed_value = false;
        for key in [b"V".as_slice(), b"DV", b"RV"] {
            removed_value |= dict.remove(key).is_some();
        }
        // the appearance of a text or choice field draws the value too
        if matches!(field_type.as_deref(), Some(b"Tx") | Some(b"Ch"))
            && dict.remove(b"AP").is_some()
        {
            needs_appearances = true;
        }
        if field_type.as_deref() == Some(b"Btn") && dict.has(b"AS") {
            dict.set("AS", Object::Name(b"Off".to_vec()));
        }
        if removed_value {
            details.push(format!("{name}: {}", value.unwrap_or_default()));
        }
    }
    if needs_appearances {
        set_need_appearances(doc);
    }
    if remove_xfa(doc) {
        details.push("the XFA copy of the form".to_owned());
    }
    finding(ScrubItem::FormValues, details)
}

/// a field's type (/FT), looked up through its parents.
fn field_type(doc: &Document, dict: &Dictionary) -> Option<Vec<u8>> {
    let mut node = dict;
    for _ in 0..32 {
        if let Ok(kind) = node.get(b"FT").and_then(Object::as_name) {
            return Some(kind.to_vec());
        }
        node = as_dict(resolve(doc, node.get(b"Parent").ok()?))?;
    }
    None
}

/// asks viewers to draw empty fields again, now their appearances are gone.
fn set_need_appearances(doc: &mut Document) {
    let form_id = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|f| f.as_reference().ok());
    if let Some(form) = form_id
        .and_then(|id| doc.objects.get_mut(&id))
        .and_then(as_dict_mut)
    {
        form.set("NeedAppearances", true);
    } else if let Ok(catalog) = doc.catalog_mut() {
        if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
            form.set("NeedAppearances", true);
        }
    }
}

/// keys drawing programs use for their own copies of the artwork.
fn private_data(doc: &mut Document) -> ScrubFinding {
    let mut removed = 0;
    for object in doc.objects.values_mut() {
        removed += remove_private_keys(object, 0);
    }
    finding(
        ScrubItem::PrivateData,
        (0..removed)
            .map(|_| "private program data".to_owned())
            .collect(),
    )
}

fn is_private_key(key: &[u8]) -> bool {
    key.starts_with(b"AIPrivateData")
        || key.starts_with(b"AIPDFPrivateData")
        || key == b"AIMetaData"
        || key == b"LastModified"
        || key == b"SpiderInfo"
}

fn remove_private_keys(object: &mut Object, depth: usize) -> usize {
    if depth > 64 {
        return 0;
    }
    let mut removed = 0;
    if let Some(dict) = as_dict_mut(object) {
        let keys: Vec<Vec<u8>> = dict
            .iter()
            .map(|(key, _)| key.clone())
            .filter(|key| is_private_key(key))
            .collect();
        for key in keys {
            dict.remove(&key);
            removed += 1;
        }
        for (_, value) in dict.iter_mut() {
            removed += remove_private_keys(value, depth + 1);
        }
    } else if let Object::Array(items) = object {
        for item in items {
            removed += remove_private_keys(item, depth + 1);
        }
    }
    removed
}

fn unused_objects(doc: &mut Document) -> ScrubFinding {
    let removed = doc.prune_objects();
    finding(
        ScrubItem::UnusedObjects,
        removed
            .iter()
            .map(|id| format!("object {}", id.0))
            .collect(),
    )
}

/// action types that leave the document.
const EXTERNAL_ACTIONS: [&[u8]; 5] = [b"URI", b"GoToR", b"GoToE", b"Launch", b"SubmitForm"];

fn external_links(doc: &mut Document) -> ScrubFinding {
    let mut details = Vec::new();
    let removed = remove_annotations_where(doc, |doc, annot| {
        subtype(doc, annot) == Some(b"Link")
            && annot
                .get(b"A")
                .ok()
                .and_then(|a| as_dict(resolve(doc, a)))
                .is_some_and(|action| is_external(doc, action))
    });
    details.extend(removed.into_iter().map(|_| "a link".to_owned()));
    // outline entries and other actions that open an address lose the action
    let external_ids: HashSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| as_dict(object).is_some_and(|dict| is_external(doc, dict)))
        .map(|(id, _)| *id)
        .collect();
    let mut removed_actions = 0;
    for object in doc.objects.values_mut() {
        if let Some(dict) = as_dict_mut(object) {
            let external = match dict.get(b"A") {
                Ok(Object::Reference(id)) => external_ids.contains(id),
                Ok(Object::Dictionary(action)) => is_external_kind(action),
                _ => false,
            };
            if external {
                dict.remove(b"A");
                removed_actions += 1;
            }
        }
    }
    details.extend(
        (0..removed_actions).map(|_| "a bookmark or button that opens an address".to_owned()),
    );
    finding(ScrubItem::ExternalLinks, details)
}

fn is_external(doc: &Document, action: &Dictionary) -> bool {
    action
        .get(b"S")
        .ok()
        .and_then(|s| resolve(doc, s).as_name().ok())
        .is_some_and(|kind| EXTERNAL_ACTIONS.contains(&kind))
}

fn is_external_kind(action: &Dictionary) -> bool {
    action
        .get(b"S")
        .ok()
        .and_then(|s| s.as_name().ok())
        .is_some_and(|kind| EXTERNAL_ACTIONS.contains(&kind))
}

/// gives the file a new random identifier and takes the ids out of any
/// xmp metadata that is kept.
fn fingerprints(doc: &mut Document) -> ScrubFinding {
    let mut details = Vec::new();
    if doc.trailer.has(b"ID") {
        details.push("the file identifier".to_owned());
    }
    doc.trailer
        .set("ID", Object::Array(vec![random_id(), random_id()]));
    let ids = regex::bytes::Regex::new(r#"(?s)<(xmpMM:(?:DocumentID|InstanceID|OriginalDocumentID))>.*?</xmpMM:(?:DocumentID|InstanceID|OriginalDocumentID)>|xmpMM:(?:DocumentID|InstanceID|OriginalDocumentID)="[^"]*""#)
        .expect("the pattern is valid");
    for object in doc.objects.values_mut() {
        let Object::Stream(stream) = object else {
            continue;
        };
        let is_xmp = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            == Some(b"XML");
        if !is_xmp {
            continue;
        }
        let Ok(xml) = decoded_content(stream) else {
            continue;
        };
        if ids.is_match(&xml) {
            let cleaned = ids.replace_all(&xml, &b""[..]).into_owned();
            let mut dict = stream.dict.clone();
            dict.remove(b"Length");
            // xmp is often read without decompressing, so it stays plain
            dict.remove(b"Filter");
            dict.remove(b"DecodeParms");
            *stream = lopdf::Stream::new(dict, cleaned);
            details.push("ids in the metadata".to_owned());
        }
    }
    finding(ScrubItem::Fingerprints, details)
}

pub(crate) fn random_id() -> Object {
    let mut bytes = [0u8; 16];
    // without a source of randomness the id is left all zeros, which
    // still links nothing to the original file
    let _ = getrandom::fill(&mut bytes);
    Object::String(bytes.to_vec(), StringFormat::Hexadecimal)
}

// ---- hidden layers ----

/// optional content groups (layers) that are off when the file opens.
fn hidden_groups(doc: &Document) -> (HashSet<ObjectId>, Vec<ObjectId>) {
    let Some(properties) = doc
        .catalog()
        .ok()
        .and_then(|c| dict_dict(doc, c, b"OCProperties"))
    else {
        return (HashSet::new(), Vec::new());
    };
    let all: Vec<ObjectId> = match properties.get(b"OCGs").ok().map(|o| resolve(doc, o)) {
        Some(Object::Array(groups)) => groups
            .iter()
            .filter_map(|g| g.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    let Some(default) = dict_dict(doc, properties, b"D") else {
        return (HashSet::new(), all);
    };
    let listed = |key: &[u8]| -> HashSet<ObjectId> {
        match default.get(key).ok().map(|o| resolve(doc, o)) {
            Some(Object::Array(groups)) => groups
                .iter()
                .filter_map(|g| g.as_reference().ok())
                .collect(),
            _ => HashSet::new(),
        }
    };
    let base_off = default
        .get(b"BaseState")
        .ok()
        .and_then(|b| b.as_name().ok())
        == Some(b"OFF");
    let hidden = if base_off {
        let on = listed(b"ON");
        all.iter()
            .copied()
            .filter(|group| !on.contains(group))
            .collect()
    } else {
        listed(b"OFF")
    };
    (hidden, all)
}

/// whether optional content (a group or a membership dictionary) is hidden.
fn is_hidden(doc: &Document, content: &Object, hidden: &HashSet<ObjectId>) -> bool {
    if let Ok(id) = content.as_reference() {
        if hidden.contains(&id) {
            return true;
        }
    }
    let Some(dict) = as_dict(resolve(doc, content)) else {
        return false;
    };
    if dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) != Some(b"OCMD") {
        return false;
    }
    let groups: Vec<bool> = match dict.get(b"OCGs").ok() {
        Some(Object::Array(items)) => items
            .iter()
            .map(|g| g.as_reference().is_ok_and(|id| hidden.contains(&id)))
            .collect(),
        Some(Object::Reference(id)) => vec![hidden.contains(id)],
        _ => return false,
    };
    if groups.is_empty() {
        return false;
    }
    match dict
        .get(b"P")
        .ok()
        .and_then(|p| p.as_name().ok())
        .unwrap_or(b"AnyOn")
    {
        b"AllOn" => groups.iter().any(|off| *off),
        b"AnyOff" => groups.iter().all(|off| !off),
        b"AllOff" => groups.iter().any(|off| !off),
        _ => groups.iter().all(|off| *off),
    }
}

/// removes everything drawn on hidden layers from every page and form,
/// then the layers themselves.
fn hidden_layers(doc: &mut Document) -> Result<ScrubFinding, RedactError> {
    let (hidden, all) = hidden_groups(doc);
    if hidden.is_empty() {
        return Ok(finding(ScrubItem::HiddenLayers, Vec::new()));
    }
    let names: Vec<String> = hidden
        .iter()
        .map(|id| {
            doc.get_dictionary(*id)
                .ok()
                .and_then(|group| group.get(b"Name").ok())
                .map(|name| describe_value(doc, name))
                .unwrap_or_else(|| "a layer".to_owned())
        })
        .collect();
    let rewrites = plan_layer_rewrites(doc, &hidden)?;
    let counts = reference_counts(doc);
    for rewrite in rewrites {
        apply_layer_rewrite(doc, rewrite, &counts);
    }
    remove_hidden_annotations(doc, &hidden);
    forget_groups(doc, &hidden, &all);
    Ok(finding(ScrubItem::HiddenLayers, names))
}

enum LayerRewrite {
    Page { page_id: ObjectId, content: Vec<u8> },
    Form { form_id: ObjectId, content: Vec<u8> },
}

/// works out every stream's new content first, so a page that can't be
/// read stops the scrub before anything has changed.
fn plan_layer_rewrites(
    doc: &Document,
    hidden: &HashSet<ObjectId>,
) -> Result<Vec<LayerRewrite>, RedactError> {
    let mut rewrites = Vec::new();
    for (index, page_id) in page_ids(doc).into_iter().enumerate() {
        let unreadable = |reason: String| RedactError::UnreadablePage(index as u32 + 1, reason);
        let bytes = crate::redact::page_content(doc, page_id).map_err(unreadable)?;
        let ops = content::parse(&bytes).map_err(unreadable)?;
        let resources = page_resources(doc, page_id);
        if let Some(kept) = drop_hidden_content(doc, ops, &resources, hidden) {
            rewrites.push(LayerRewrite::Page {
                page_id,
                content: content::serialize(&kept),
            });
        }
    }
    for (id, object) in &doc.objects {
        let Object::Stream(stream) = object else {
            continue;
        };
        if stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            != Some(b"Form")
        {
            continue;
        }
        let unreadable =
            |reason: String| RedactError::Damaged(format!("a drawing can't be read: {reason}"));
        let bytes = decoded_content(stream).map_err(unreadable)?;
        let ops = content::parse(&bytes).map_err(unreadable)?;
        let resources = dict_dict(doc, &stream.dict, b"Resources")
            .cloned()
            .unwrap_or_default();
        if let Some(kept) = drop_hidden_content(doc, ops, &resources, hidden) {
            rewrites.push(LayerRewrite::Form {
                form_id: *id,
                content: content::serialize(&kept),
            });
        }
    }
    Ok(rewrites)
}

/// the operations left once hidden layers are taken out, or none if
/// nothing was on a hidden layer.
fn drop_hidden_content(
    doc: &Document,
    ops: Vec<Op>,
    resources: &Dictionary,
    hidden: &HashSet<ObjectId>,
) -> Option<Vec<Op>> {
    let mut kept = Vec::with_capacity(ops.len());
    let mut skipping_depth: Option<usize> = None;
    let mut depth = 0usize;
    let mut skipped_saves = 0i64;
    let mut changed = false;
    for op in ops {
        let name = op.name().to_owned();
        if let Some(start) = skipping_depth {
            match name.as_str() {
                "BMC" | "BDC" => depth += 1,
                "EMC" => {
                    depth = depth.saturating_sub(1);
                    if depth == start {
                        skipping_depth = None;
                        // a q left open inside the hidden part stays open,
                        // so the Q that closes it later still matches
                        kept.extend((0..skipped_saves.max(0)).map(|_| Op::new("q", vec![])));
                        skipped_saves = 0;
                    }
                }
                "q" => skipped_saves += 1,
                "Q" => skipped_saves -= 1,
                _ => {}
            }
            continue;
        }
        match name.as_str() {
            "BDC" if marks_hidden_layer(doc, op.operands(), resources, hidden) => {
                skipping_depth = Some(depth);
                depth += 1;
                changed = true;
                continue;
            }
            "BMC" | "BDC" => depth += 1,
            "EMC" => depth = depth.saturating_sub(1),
            "Do" if xobject_on_hidden_layer(doc, op.operands(), resources, hidden) => {
                changed = true;
                continue;
            }
            _ => {}
        }
        kept.push(op);
    }
    changed.then_some(kept)
}

fn marks_hidden_layer(
    doc: &Document,
    operands: &[Object],
    resources: &Dictionary,
    hidden: &HashSet<ObjectId>,
) -> bool {
    if operands.first() != Some(&Object::Name(b"OC".to_vec())) {
        return false;
    }
    let Some(Object::Name(name)) = operands.get(1) else {
        return false;
    };
    dict_dict(doc, resources, b"Properties")
        .and_then(|properties| properties.get(name).ok())
        .is_some_and(|content| is_hidden(doc, content, hidden))
}

fn xobject_on_hidden_layer(
    doc: &Document,
    operands: &[Object],
    resources: &Dictionary,
    hidden: &HashSet<ObjectId>,
) -> bool {
    let Some(Object::Name(name)) = operands.first() else {
        return false;
    };
    dict_dict(doc, resources, b"XObject")
        .and_then(|xobjects| xobjects.get(name).ok())
        .and_then(|xobject| as_dict(resolve(doc, xobject)))
        .and_then(|xobject| xobject.get(b"OC").ok())
        .is_some_and(|content| is_hidden(doc, content, hidden))
}

fn apply_layer_rewrite(
    doc: &mut Document,
    rewrite: LayerRewrite,
    counts: &HashMap<ObjectId, usize>,
) {
    match rewrite {
        LayerRewrite::Page { page_id, content } => {
            let stream = flate_stream(Dictionary::new(), &content);
            let current = doc
                .get_dictionary(page_id)
                .ok()
                .and_then(|p| p.get(b"Contents").ok())
                .and_then(|c| c.as_reference().ok());
            let id = match current.filter(|id| {
                counts.get(id).copied().unwrap_or(0) <= 1
                    && matches!(doc.objects.get(id), Some(Object::Stream(_)))
            }) {
                Some(id) => {
                    doc.objects.insert(id, Object::Stream(stream));
                    id
                }
                None => doc.add_object(Object::Stream(stream)),
            };
            if let Ok(page) = doc.get_dictionary_mut(page_id) {
                page.set("Contents", Object::Reference(id));
            }
        }
        LayerRewrite::Form { form_id, content } => {
            if let Some(Object::Stream(stream)) = doc.objects.get(&form_id) {
                let mut dict = stream.dict.clone();
                dict.remove(b"Length");
                dict.remove(b"Filter");
                dict.remove(b"DecodeParms");
                doc.objects
                    .insert(form_id, Object::Stream(flate_stream(dict, &content)));
            }
        }
    }
}

fn remove_hidden_annotations(doc: &mut Document, hidden: &HashSet<ObjectId>) {
    let hidden = hidden.clone();
    remove_annotations_where(doc, move |doc, annot| {
        annot
            .get(b"OC")
            .ok()
            .is_some_and(|content| is_hidden(doc, content, &hidden))
    });
}

/// takes the hidden groups out of the layer list and every place that
/// names them, then empties them. with no layers left the list goes too.
fn forget_groups(doc: &mut Document, hidden: &HashSet<ObjectId>, all: &[ObjectId]) {
    let remaining = all.iter().filter(|group| !hidden.contains(group)).count();
    let properties_id = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"OCProperties").ok())
        .and_then(|p| p.as_reference().ok());
    let mut properties = match properties_id {
        Some(id) => doc.get_dictionary(id).ok().cloned(),
        None => doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"OCProperties").ok())
            .and_then(|p| p.as_dict().ok())
            .cloned(),
    };
    if let Some(dict) = properties.as_mut() {
        let mut object = Object::Dictionary(std::mem::take(dict));
        drop_references(&mut object, hidden, 0);
        if let Object::Dictionary(cleaned) = object {
            *dict = cleaned;
        }
    }
    match (properties_id, properties, remaining) {
        (_, _, 0) => {
            if let Ok(catalog) = doc.catalog_mut() {
                catalog.remove(b"OCProperties");
            }
        }
        (Some(id), Some(dict), _) => {
            doc.objects.insert(id, Object::Dictionary(dict));
        }
        (None, Some(dict), _) => {
            if let Ok(catalog) = doc.catalog_mut() {
                catalog.set("OCProperties", Object::Dictionary(dict));
            }
        }
        _ => {}
    }
    for group in hidden {
        doc.objects.insert(*group, Object::Null);
    }
}

/// removes references to some objects from every array inside an object.
fn drop_references(object: &mut Object, targets: &HashSet<ObjectId>, depth: usize) {
    if depth > 32 {
        return;
    }
    match object {
        Object::Array(items) => {
            items.retain(|item| {
                item.as_reference()
                    .map_or(true, |id| !targets.contains(&id))
            });
            for item in items {
                drop_references(item, targets, depth + 1);
            }
        }
        Object::Dictionary(dict) => {
            for (_, value) in dict.iter_mut() {
                drop_references(value, targets, depth + 1);
            }
        }
        _ => {}
    }
}
