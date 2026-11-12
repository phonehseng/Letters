//! document time stamps (iso 32000-2 12.8.5, /SubFilter /ETSI.RFC3161): a
//! time stamp authority's token over the whole file so far, added as an
//! incremental update. it proves the document existed in this form at that
//! time, and seals everything before it, earlier signatures and the audit
//! trail included.

use lopdf::{Document, Object};

use super::incremental::{write_update, HandWritten};
use super::placeholder::{fill_placeholders, signature_body};
use super::sign::{add_invisible_field, mark_form_signed, TIMESTAMP_ROOM};
use super::timestamp::{fetch_token, TimestampAuthority};
use crate::error::FormResult;
use crate::objects::check_document;

/// the name new document time stamp fields get (made unique when taken).
const FIELD_NAME: &str = "DocumentTimestamp";

/// adds a document time stamp from the authority the person chose and
/// returns the new file. only the sha-256 of the file's bytes is sent.
pub fn add_document_timestamp(
    doc_bytes: &[u8],
    authority: &TimestampAuthority,
) -> FormResult<Vec<u8>> {
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    let mut after = before.clone();
    let (field_id, _) = add_invisible_field(&mut after, Some(FIELD_NAME))?;
    let timestamp_id = after.new_object_id();
    after
        .get_dictionary_mut(field_id)?
        .set("V", Object::Reference(timestamp_id));
    mark_form_signed(&mut after)?;
    let kind = "/Type /DocTimeStamp /Filter /Adobe.PPKLite /SubFilter /ETSI.RFC3161";
    let body = signature_body(kind, TIMESTAMP_ROOM, &[], &[])?;
    let written = HandWritten {
        id: timestamp_id,
        body,
    };
    let update = write_update(doc_bytes, &before, &after, &[written])?;
    let start = update.offsets[&timestamp_id];
    fill_placeholders(update.bytes, start, TIMESTAMP_ROOM, |covered| {
        fetch_token(authority, &covered)
    })
}
