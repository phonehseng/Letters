//! making sure nothing old survives in the saved file.
//!
//! a pdf can be saved in two ways. an incremental save adds the changes
//! to the end of the file and leaves everything before them in place,
//! including the content that was just redacted: anyone can cut the file
//! back to its earlier version. `save_clean` always writes the whole file
//! from scratch, keeps only the objects still in use, numbers them again
//! from 1 so the gaps do not hint at what was removed, and writes no
//! object streams, so no compressed bundle of old objects can come along.

use lopdf::xref::XrefType;
use lopdf::{Document, Object, ObjectId};

use crate::error::RedactError;
use crate::objects::as_dict_mut;

/// keys of a page that can show or hold its old content: the thumbnail
/// picture, private data of the program that made it, and its metadata.
const PAGE_EXTRAS: [&[u8]; 3] = [b"Thumb", b"PieceInfo", b"Metadata"];

pub fn remove_page_extras(doc: &mut Document, page_id: ObjectId) {
    if let Ok(page) = doc.get_dictionary_mut(page_id) {
        for key in PAGE_EXTRAS {
            page.remove(key);
        }
    }
}

/// removes document wide copies of content: the xmp metadata (which can
/// hold the title, description and keywords), private application data,
/// and the xfa copy of a form (which holds every field value again).
pub fn remove_document_extras(doc: &mut Document) -> Vec<String> {
    let mut warnings = Vec::new();
    let form_id = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| form.as_reference().ok());
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
        catalog.remove(b"PieceInfo");
        if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
            if form.remove(b"XFA").is_some() {
                warnings.push(XFA_WARNING.to_owned());
            }
        }
    }
    if let Some(form) = form_id
        .and_then(|id| doc.objects.get_mut(&id))
        .and_then(as_dict_mut)
    {
        if form.remove(b"XFA").is_some() {
            warnings.push(XFA_WARNING.to_owned());
        }
    }
    warnings
}

const XFA_WARNING: &str = "The form also had an XFA copy, which can hold every field value again. It was removed, so some form software may show the form differently.";

/// writes the whole document from scratch, with only the objects still
/// in use. use this, never an incremental save, after redacting or
/// scrubbing.
pub fn save_clean(doc: &mut Document) -> Result<Vec<u8>, RedactError> {
    keep_only_trailer_essentials(doc);
    doc.prune_objects();
    // renumbering would break the keys of rc4 and aes-128 encryption,
    // which mix each object's number into its key
    if !doc.is_encrypted() {
        doc.renumber_objects();
    }
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|error| RedactError::CannotSave(error.to_string()))?;
    Ok(bytes)
}

/// the trailer may point at earlier revisions of the file (/Prev,
/// /XRefStm). only the entries a fresh file needs are kept.
fn keep_only_trailer_essentials(doc: &mut Document) {
    let keep: [&[u8]; 4] = [b"Root", b"Info", b"ID", b"Encrypt"];
    let keys: Vec<Vec<u8>> = doc.trailer.iter().map(|(key, _)| key.clone()).collect();
    for key in keys {
        if !keep.contains(&key.as_slice()) {
            doc.trailer.remove(&key);
        }
    }
}
