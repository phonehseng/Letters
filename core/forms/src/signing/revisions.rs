//! what changed between the revisions of a signed file.
//!
//! every incremental update ends with its own %%EOF, so each prefix of the
//! file up to one is a whole earlier version. comparing each version with
//! the one before shows which objects were added or changed, and each one
//! is sorted into a kind of change (a field filled in, a new signature, a
//! comment, page content...) by what it is and what points at it.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::objects::{annotation_pages, get_name, page_ids};
use crate::read::{kind_of, FieldKind};
use crate::tree::collect_fields;

/// the name of the embedded file that holds Letters' audit trail.
pub(crate) const AUDIT_TRAIL_FILE: &str = "letters-audit-trail.json";

/// a kind of change between two revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    /// a field's answer (and its drawing)
    FieldValue,
    /// a signature added, or an empty signature field signed
    Signature,
    /// a document time stamp added
    DocumentTimestamp,
    /// fields added, removed or redefined (not their answers)
    FormDefinition,
    /// comments and other annotations that aren't form fields
    Annotation,
    /// what pages show, and pages added or removed
    PageContent,
    /// certificates and revocation answers (the document security store)
    ValidationData,
    /// Letters' own audit trail
    AuditTrail,
    /// document information and metadata
    Metadata,
    /// anything else
    Other,
}

/// one change, in words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: ChangeKind,
    /// the field it concerns, if any
    pub field: Option<String>,
    /// 0-based page, if it concerns one
    pub page: Option<u32>,
    pub description: String,
}

/// one revision of the file: where it ends and what it changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionReport {
    /// 0 is the original file
    pub index: usize,
    /// the byte just after this revision's %%EOF
    pub end: usize,
    /// changes against the revision before (none for the first)
    pub changes: Vec<Change>,
}

/// the revisions of a file, with their changes. revisions that can't be
/// read on their own are left out.
pub fn revisions(bytes: &[u8]) -> Vec<RevisionReport> {
    let ends = super::verify::revision_ends(bytes);
    let mut reports = Vec::new();
    let mut previous: Option<Document> = None;
    for end in ends {
        let Ok(doc) = Document::load_mem(&bytes[..end]) else {
            continue;
        };
        let changes = match &previous {
            Some(earlier) => compare(earlier, &doc),
            None => Vec::new(),
        };
        reports.push(RevisionReport {
            index: reports.len(),
            end,
            changes,
        });
        previous = Some(doc);
    }
    reports
}

/// the objects that were added or changed from one revision to the next,
/// sorted into kinds, one change per kind and field or page.
fn compare(earlier: &Document, later: &Document) -> Vec<Change> {
    let context = Context::read(later);
    let mut changes: Vec<Change> = Vec::new();
    for (id, object) in &later.objects {
        let old = earlier.objects.get(id);
        if old == Some(object) {
            continue;
        }
        if let Some(change) = context.classify(earlier, *id, object, old) {
            let seen = changes.iter().any(|existing| {
                existing.kind == change.kind
                    && existing.field == change.field
                    && existing.page == change.page
            });
            if !seen {
                changes.push(change);
            }
        }
    }
    changes.sort_by_key(|change| (change.page, change.field.clone(), change.kind as u8));
    changes
}

/// what the later revision says about its objects: which are fields and
/// widgets (and of which field), pages, annotations, and who points at what.
struct Context<'a> {
    doc: &'a Document,
    /// field and widget ids to (field name, is a signature field)
    fields: HashMap<ObjectId, (String, bool)>,
    pages: HashMap<ObjectId, u32>,
    annotation_pages: HashMap<ObjectId, u32>,
    /// for each object, the objects whose value refers to it
    referrers: HashMap<ObjectId, Vec<ObjectId>>,
    info: Option<ObjectId>,
}

/// how far up the chain of referrers a stream's owner is looked for.
const MAX_OWNER_DEPTH: usize = 6;

impl<'a> Context<'a> {
    fn read(doc: &'a Document) -> Self {
        let mut fields = HashMap::new();
        for field in collect_fields(doc) {
            let signature = kind_of(doc, &field) == Some(FieldKind::Signature);
            fields.insert(field.id, (field.name.clone(), signature));
            for widget in &field.widgets {
                fields.insert(*widget, (field.name.clone(), signature));
            }
        }
        let pages = page_ids(doc)
            .into_iter()
            .enumerate()
            .map(|(index, id)| (id, index as u32))
            .collect();
        let mut referrers: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        for (id, object) in &doc.objects {
            let mut found = Vec::new();
            collect_references(object, &mut found, 0);
            for target in found {
                referrers.entry(target).or_default().push(*id);
            }
        }
        let info = doc.trailer.get(b"Info").and_then(Object::as_reference).ok();
        Context {
            doc,
            fields,
            pages,
            annotation_pages: annotation_pages(doc),
            referrers,
            info,
        }
    }

    fn change(&self, kind: ChangeKind, field: Option<String>, page: Option<u32>) -> Option<Change> {
        let description = describe(kind, field.as_deref(), page);
        Some(Change {
            kind,
            field,
            page,
            description,
        })
    }

    fn classify(
        &self,
        earlier: &Document,
        id: ObjectId,
        object: &Object,
        old: Option<&Object>,
    ) -> Option<Change> {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &stream.dict,
            _ => return self.owned_change(id, 0),
        };
        let type_name = get_name(self.doc, dict, b"Type").unwrap_or_default();
        match type_name.as_slice() {
            b"XRef" | b"ObjStm" => return None,
            b"DocTimeStamp" => return self.change(ChangeKind::DocumentTimestamp, None, None),
            b"Sig" => return self.change(ChangeKind::Signature, None, None),
            b"DSS" => return self.change(ChangeKind::ValidationData, None, None),
            b"Catalog" => return self.catalog_change(dict, old),
            b"Page" => return self.page_change(earlier, id, dict, old),
            b"Pages" => return self.change(ChangeKind::PageContent, None, None),
            b"Filespec" if is_audit_trail(self.doc, dict) => {
                return self.change(ChangeKind::AuditTrail, None, None)
            }
            _ => {}
        }
        if dict.has(b"ByteRange") {
            return self.change(ChangeKind::Signature, None, None);
        }
        if let Some((field, signature)) = self.fields.get(&id) {
            return self.field_change(field, *signature, dict, old);
        }
        if dict.has(b"Fields") && (dict.has(b"DR") || dict.has(b"SigFlags") || dict.has(b"DA")) {
            return self.form_change(dict, old);
        }
        if Some(id) == self.info {
            return self.change(ChangeKind::Metadata, None, None);
        }
        if get_name(self.doc, dict, b"Subtype").is_some() && dict.has(b"Rect") {
            let page = self.annotation_pages.get(&id).copied();
            return self.change(ChangeKind::Annotation, None, page);
        }
        self.owned_change(id, 0)
    }

    /// a stream or helper object: it belongs to whatever points at it.
    fn owned_change(&self, id: ObjectId, depth: usize) -> Option<Change> {
        if depth > MAX_OWNER_DEPTH {
            return self.change(ChangeKind::Other, None, None);
        }
        let owners = self.referrers.get(&id).cloned().unwrap_or_default();
        for owner in owners {
            if let Some(change) = self.owner_kind(owner, id, depth) {
                return Some(change);
            }
        }
        self.change(ChangeKind::Other, None, None)
    }

    fn owner_kind(&self, owner: ObjectId, target: ObjectId, depth: usize) -> Option<Change> {
        if let Some((field, signature)) = self.fields.get(&owner) {
            let kind = if *signature {
                ChangeKind::Signature
            } else {
                ChangeKind::FieldValue
            };
            return self.change(kind, Some(field.clone()), None);
        }
        if let Some(page) = self.pages.get(&owner) {
            return self.change(ChangeKind::PageContent, None, Some(*page));
        }
        if let Some(page) = self.annotation_pages.get(&owner) {
            return self.change(ChangeKind::Annotation, None, Some(*page));
        }
        let dict = self
            .doc
            .get_object(owner)
            .ok()
            .and_then(|object| match object {
                Object::Dictionary(dict) => Some(dict),
                Object::Stream(stream) => Some(&stream.dict),
                _ => None,
            })?;
        let type_name = get_name(self.doc, dict, b"Type").unwrap_or_default();
        match type_name.as_slice() {
            b"DSS" => self.change(ChangeKind::ValidationData, None, None),
            b"Catalog" if refers_under(dict, b"Metadata", target) => {
                self.change(ChangeKind::Metadata, None, None)
            }
            b"Filespec" if is_audit_trail(self.doc, dict) => {
                self.change(ChangeKind::AuditTrail, None, None)
            }
            // the form's fonts are added to draw answers
            _ if dict.has(b"Fields") && dict.has(b"DR") => {
                self.change(ChangeKind::FieldValue, None, None)
            }
            _ => self.owned_change(owner, depth + 1),
        }
    }

    fn field_change(
        &self,
        field: &str,
        signature: bool,
        dict: &Dictionary,
        old: Option<&Object>,
    ) -> Option<Change> {
        let field = Some(field.to_owned());
        if signature {
            return self.change(ChangeKind::Signature, field, None);
        }
        let Some(old) = old.and_then(|old| old.as_dict().ok()) else {
            return self.change(ChangeKind::FormDefinition, field, None);
        };
        // an answer changes /V, the drawing and the chosen options; the
        // read-only flag is what a signature's field lock sets
        let answer_keys: [&[u8]; 6] = [b"V", b"AP", b"AS", b"I", b"RV", b"Ff"];
        let only_answers = changed_keys(old, dict)
            .iter()
            .all(|key| answer_keys.contains(&key.as_slice()));
        let lock_only = changed_keys(old, dict) == vec![b"Ff".to_vec()];
        if lock_only {
            return self.change(ChangeKind::Signature, field, None);
        }
        let kind = if only_answers {
            ChangeKind::FieldValue
        } else {
            ChangeKind::FormDefinition
        };
        self.change(kind, field, None)
    }

    fn form_change(&self, dict: &Dictionary, old: Option<&Object>) -> Option<Change> {
        let Some(old) = old.and_then(|old| old.as_dict().ok()) else {
            return self.change(ChangeKind::FormDefinition, None, None);
        };
        let keys = changed_keys(old, dict);
        let signing_keys: [&[u8]; 2] = [b"SigFlags", b"Fields"];
        let drawing_keys: [&[u8]; 3] = [b"DR", b"DA", b"NeedAppearances"];
        if keys
            .iter()
            .all(|key| signing_keys.contains(&key.as_slice()))
        {
            // new top level fields are judged by themselves; this is bookkeeping
            return None;
        }
        if keys.iter().all(|key| {
            drawing_keys.contains(&key.as_slice()) || signing_keys.contains(&key.as_slice())
        }) {
            return self.change(ChangeKind::FieldValue, None, None);
        }
        self.change(ChangeKind::FormDefinition, None, None)
    }

    fn catalog_change(&self, dict: &Dictionary, old: Option<&Object>) -> Option<Change> {
        let old = old.and_then(|old| old.as_dict().ok())?;
        let keys = changed_keys(old, dict);
        let kind = |key: &[u8]| match key {
            b"DSS" => Some(ChangeKind::ValidationData),
            b"Perms" => Some(ChangeKind::Signature),
            b"AcroForm" => Some(ChangeKind::FormDefinition),
            b"Metadata" => Some(ChangeKind::Metadata),
            b"Names" | b"AF" => None,
            b"Pages" => Some(ChangeKind::PageContent),
            _ => Some(ChangeKind::Other),
        };
        let kinds: Vec<ChangeKind> = keys.iter().filter_map(|key| kind(key)).collect();
        // the most serious kind decides how the catalog change is judged
        let worst = kinds.into_iter().max_by_key(|kind| severity(*kind))?;
        self.change(worst, None, None)
    }

    fn page_change(
        &self,
        earlier: &Document,
        id: ObjectId,
        dict: &Dictionary,
        old: Option<&Object>,
    ) -> Option<Change> {
        let page = self.pages.get(&id).copied();
        let Some(old) = old.and_then(|old| old.as_dict().ok()) else {
            return self.change(ChangeKind::PageContent, None, page);
        };
        let keys = changed_keys(old, dict);
        // /Tabs follows the fields; /Annots lists them and comments
        let bookkeeping: [&[u8]; 2] = [b"Annots", b"Tabs"];
        if !keys.iter().all(|key| bookkeeping.contains(&key.as_slice())) {
            return self.change(ChangeKind::PageContent, None, page);
        }
        let before: HashSet<ObjectId> = annotation_ids(earlier, old).into_iter().collect();
        let after: HashSet<ObjectId> = annotation_ids(self.doc, dict).into_iter().collect();
        let removed_comment = before
            .difference(&after)
            .any(|gone| !self.fields.contains_key(gone));
        if removed_comment {
            return self.change(ChangeKind::Annotation, None, page);
        }
        // added annotations are judged as objects of their own
        None
    }
}

fn collect_references(object: &Object, out: &mut Vec<ObjectId>, depth: usize) {
    if depth > 16 {
        return;
    }
    match object {
        Object::Reference(id) => out.push(*id),
        Object::Array(items) => items
            .iter()
            .for_each(|item| collect_references(item, out, depth + 1)),
        Object::Dictionary(dict) => dict
            .iter()
            .for_each(|(_, value)| collect_references(value, out, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .for_each(|(_, value)| collect_references(value, out, depth + 1)),
        _ => {}
    }
}

fn refers_under(dict: &Dictionary, key: &[u8], target: ObjectId) -> bool {
    let mut found = Vec::new();
    if let Ok(value) = dict.get(key) {
        collect_references(value, &mut found, 0);
    }
    found.contains(&target)
}

fn changed_keys(old: &Dictionary, new: &Dictionary) -> Vec<Vec<u8>> {
    let mut keys: Vec<Vec<u8>> = Vec::new();
    for (key, value) in new.iter() {
        if old.get(key).ok() != Some(value) {
            keys.push(key.clone());
        }
    }
    for (key, _) in old.iter() {
        if !new.has(key) {
            keys.push(key.clone());
        }
    }
    keys
}

fn annotation_ids(doc: &Document, page: &Dictionary) -> Vec<ObjectId> {
    crate::objects::references_in(doc, page, b"Annots")
}

fn is_audit_trail(doc: &Document, dict: &Dictionary) -> bool {
    [b"UF".as_slice(), b"F"]
        .iter()
        .any(|key| crate::objects::get_text(doc, dict, key).as_deref() == Some(AUDIT_TRAIL_FILE))
}

/// how much a kind of change matters, for picking one to describe.
pub(crate) fn severity(kind: ChangeKind) -> u8 {
    match kind {
        ChangeKind::ValidationData | ChangeKind::AuditTrail | ChangeKind::DocumentTimestamp => 0,
        ChangeKind::Metadata => 1,
        ChangeKind::Signature | ChangeKind::FieldValue => 2,
        ChangeKind::Annotation => 3,
        ChangeKind::FormDefinition | ChangeKind::Other => 4,
        ChangeKind::PageContent => 5,
    }
}

fn describe(kind: ChangeKind, field: Option<&str>, page: Option<u32>) -> String {
    let on_page = page
        .map(|page| format!(" on page {}", page + 1))
        .unwrap_or_default();
    match (kind, field) {
        (ChangeKind::FieldValue, Some(field)) => format!("the answer in “{field}” changed"),
        (ChangeKind::FieldValue, None) => "answers were drawn again".to_owned(),
        (ChangeKind::Signature, Some(field)) => format!("“{field}” was signed"),
        (ChangeKind::Signature, None) => "a signature was added".to_owned(),
        (ChangeKind::DocumentTimestamp, _) => "a document time stamp was added".to_owned(),
        (ChangeKind::FormDefinition, Some(field)) => {
            format!("the field “{field}” was added or redefined")
        }
        (ChangeKind::FormDefinition, None) => "the form’s fields were changed".to_owned(),
        (ChangeKind::Annotation, _) => format!("comments or markup were changed{on_page}"),
        (ChangeKind::PageContent, _) => format!("page content was changed{on_page}"),
        (ChangeKind::ValidationData, _) => {
            "validation data (certificates, revocation answers) was added".to_owned()
        }
        (ChangeKind::AuditTrail, _) => "the audit trail was extended".to_owned(),
        (ChangeKind::Metadata, _) => "the document’s information was updated".to_owned(),
        (ChangeKind::Other, _) => "other parts of the file were changed".to_owned(),
    }
}
