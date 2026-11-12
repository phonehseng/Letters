//! reading and writing the audit trail's embedded file. each new entry is
//! appended as an incremental update that writes the whole list again as
//! a new version of the file; earlier versions stay in earlier revisions,
//! which is how the checker sees that nothing was taken out.

use std::time::SystemTime;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::entry::{AuditEntry, AuditEvent};
use crate::error::{FormError, FormResult};
use crate::objects::{catalog_id, check_document, get_array, get_dict, name, resolve, text_string};
use crate::signing::{readable_date, write_update, AUDIT_TRAIL_FILE};

/// the media type of the trail, json (rfc 8259).
const MEDIA_TYPE: &str = "application/json";

fn failed(why: &str) -> FormError {
    FormError::AuditFailed(why.to_owned())
}

/// the trail's file specification in a document, if it has one.
pub(crate) fn trail_filespec(doc: &Document) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    let names = get_dict(doc, catalog, b"Names")?;
    let tree = get_dict(doc, names, b"EmbeddedFiles")?;
    let pairs = get_array(doc, tree, b"Names")?;
    for pair in pairs.chunks(2) {
        let [key, value] = pair else { continue };
        let is_trail = matches!(resolve(doc, key), Object::String(bytes, _) if bytes == AUDIT_TRAIL_FILE.as_bytes());
        if is_trail {
            return value.as_reference().ok();
        }
    }
    None
}

/// the entries stored in a document (none when it has no trail). an
/// unreadable trail is an error, so it is never silently replaced.
pub(crate) fn read_entries(doc: &Document) -> FormResult<Vec<AuditEntry>> {
    let Some(filespec) = trail_filespec(doc) else {
        return Ok(Vec::new());
    };
    let spec = doc.get_dictionary(filespec)?;
    let file = get_dict(doc, spec, b"EF")
        .and_then(|ef| ef.get(b"F").ok())
        .and_then(|value| value.as_reference().ok())
        .ok_or_else(|| failed("the audit trail’s file is missing"))?;
    let stream = doc.get_object(file)?.as_stream()?;
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    serde_json::from_slice(&bytes)
        .map_err(|error| failed(&format!("the audit trail can’t be read: {error}")))
}

/// adds one entry to the trail as an incremental update and returns the
/// new file. `actor` is the name the person set in Letters.
pub fn append_audit_entry(
    doc_bytes: &[u8],
    actor: &str,
    event: &AuditEvent,
) -> FormResult<Vec<u8>> {
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    let mut entries = read_entries(&before)?;
    let time = readable_date(SystemTime::now());
    let entry = AuditEntry::next(entries.last(), event.clone(), actor, time, doc_bytes);
    entries.push(entry);
    let mut after = before.clone();
    store_entries(&mut after, &entries)?;
    Ok(write_update(doc_bytes, &before, &after, &[])?.bytes)
}

fn store_entries(doc: &mut Document, entries: &[AuditEntry]) -> FormResult<()> {
    let json = serde_json::to_vec_pretty(entries).map_err(|error| failed(&error.to_string()))?;
    let mut params = Dictionary::new();
    params.set("Size", Object::Integer(json.len() as i64));
    let mut stream_dict = Dictionary::new();
    stream_dict.set("Type", name("EmbeddedFile"));
    stream_dict.set("Subtype", name(MEDIA_TYPE));
    stream_dict.set("Params", Object::Dictionary(params));
    let file = doc.add_object(Stream::new(stream_dict, json));
    match trail_filespec(doc) {
        Some(filespec) => point_filespec_at(doc, filespec, file),
        None => add_filespec(doc, file),
    }
}

fn point_filespec_at(doc: &mut Document, filespec: ObjectId, file: ObjectId) -> FormResult<()> {
    let mut embedded = Dictionary::new();
    embedded.set("F", Object::Reference(file));
    embedded.set("UF", Object::Reference(file));
    doc.get_dictionary_mut(filespec)?
        .set("EF", Object::Dictionary(embedded));
    Ok(())
}

/// a new file specification, listed in the catalog's embedded files and
/// as an associated file (/AF, iso 32000-2 14.13), which pdf/a-3 asks for.
fn add_filespec(doc: &mut Document, file: ObjectId) -> FormResult<()> {
    let mut spec = Dictionary::new();
    spec.set("Type", name("Filespec"));
    spec.set("F", text_string(AUDIT_TRAIL_FILE));
    spec.set("UF", text_string(AUDIT_TRAIL_FILE));
    spec.set(
        "Desc",
        text_string("Letters audit trail: what happened to this document in Letters"),
    );
    spec.set("AFRelationship", name("Data"));
    let filespec = doc.add_object(Object::Dictionary(spec));
    point_filespec_at(doc, filespec, file)?;
    let catalog = catalog_id(doc)?;
    let catalog_dict = doc.get_dictionary(catalog)?.clone();
    let mut names = get_dict(doc, &catalog_dict, b"Names")
        .cloned()
        .unwrap_or_default();
    let mut tree = get_dict(doc, &names, b"EmbeddedFiles")
        .cloned()
        .unwrap_or_default();
    if tree.has(b"Kids") {
        return Err(failed(
            "this file’s list of attachments is too complex for Letters to add to",
        ));
    }
    let mut pairs = get_array(doc, &tree, b"Names").cloned().unwrap_or_default();
    insert_sorted(doc, &mut pairs, filespec);
    tree.set("Names", Object::Array(pairs));
    names.set("EmbeddedFiles", Object::Dictionary(tree));
    let mut associated = match catalog_dict
        .get(b"AF")
        .ok()
        .map(|value| resolve(doc, value))
    {
        Some(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    associated.push(Object::Reference(filespec));
    let catalog = doc.get_dictionary_mut(catalog)?;
    // written into the catalog itself, so the change reads as the trail's
    catalog.set("Names", Object::Dictionary(names));
    catalog.set("AF", Object::Array(associated));
    Ok(())
}

/// name trees keep their keys in order (iso 32000 7.9.6).
fn insert_sorted(doc: &Document, pairs: &mut Vec<Object>, filespec: ObjectId) {
    let key = AUDIT_TRAIL_FILE.as_bytes();
    let mut at = pairs.len();
    for (index, pair) in pairs.chunks(2).enumerate() {
        if let Object::String(existing, _) = resolve(doc, &pair[0]) {
            if existing.as_slice() > key {
                at = index * 2;
                break;
            }
        }
    }
    pairs.insert(at, Object::Reference(filespec));
    pairs.insert(at, text_string(AUDIT_TRAIL_FILE));
}
