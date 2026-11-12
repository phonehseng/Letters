//! long-term validation data that works offline: a document security
//! store (/DSS, iso 32000-2 12.8.4.3, etsi en 319 142-1 5.4) holding the
//! signers' certificate chains and whatever revocation answers (crls,
//! ocsp responses) the caller already has. Letters never fetches
//! revocation data itself; that would need the network.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::cms::parse_signed_data;
use super::document::signature_dictionaries;
use super::incremental::write_update;
use super::verify::certificates_of;
use crate::error::{FormError, FormResult};
use crate::objects::{catalog_id, check_document, get, get_dict, name, references_in};

/// validation material the caller has, all der encoded.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationData {
    /// more certificates, such as issuers the signature didn't carry
    #[serde(default)]
    pub certificates: Vec<Vec<u8>>,
    /// certificate revocation lists
    #[serde(default)]
    pub crls: Vec<Vec<u8>>,
    /// ocsp responses
    #[serde(default)]
    pub ocsp_responses: Vec<Vec<u8>>,
}

/// the parts of a document security store, as object ids of streams.
#[derive(Default)]
struct Store {
    certificates: Vec<ObjectId>,
    crls: Vec<ObjectId>,
    ocsps: Vec<ObjectId>,
}

/// adds (or extends) the document security store as an incremental update:
/// every certificate the signatures carry, plus `data`. each signature gets
/// a /VRI entry naming the material that applies to it.
pub fn add_validation_data(doc_bytes: &[u8], data: &ValidationData) -> FormResult<Vec<u8>> {
    let before = Document::load_mem(doc_bytes)?;
    check_document(&before)?;
    let mut after = before.clone();
    let signatures = signature_dictionaries(&before);
    if signatures.is_empty() {
        return Err(FormError::SigningFailed(
            "the document has no signatures to add validation data for".into(),
        ));
    }
    let existing = existing_store(&after);
    let mut store = Store::default();
    let mut vri = Dictionary::new();
    for (_, signature) in &signatures {
        let contents = match get(&before, signature, b"Contents") {
            Some(Object::String(contents, _)) => contents.clone(),
            _ => continue,
        };
        let mut certificates = carried_certificates(&contents);
        certificates.extend(data.certificates.iter().cloned());
        let entry = Store {
            certificates: add_streams(&mut after, &certificates, &existing),
            crls: add_streams(&mut after, &data.crls, &existing),
            ocsps: add_streams(&mut after, &data.ocsp_responses, &existing),
        };
        vri.set(vri_key(&contents), Object::Dictionary(vri_entry(&entry)));
        merge(&mut store, entry);
    }
    write_store(&mut after, store, vri)?;
    Ok(write_update(doc_bytes, &before, &after, &[])?.bytes)
}

/// the vri key: the upper case hex sha-1 of the signature's /Contents
/// bytes as stored, padding included, as itext and pyhanko write it
/// (etsi en 319 142-1 5.4.2.2 names sha-1 for this key).
fn vri_key(contents: &[u8]) -> Vec<u8> {
    let hash = Sha1::digest(contents);
    hash.iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>()
        .into_bytes()
}

/// every certificate inside a signature, and inside its time stamp token.
fn carried_certificates(contents: &[u8]) -> Vec<Vec<u8>> {
    let Ok(signed) = parse_signed_data(contents) else {
        return Vec::new();
    };
    let mut out: Vec<Vec<u8>> = certificates_of(&signed)
        .iter()
        .filter_map(|certificate| der::Encode::to_der(certificate).ok())
        .collect();
    let token = signed
        .signer_infos
        .0
        .iter()
        .next()
        .and_then(|signer| signer.unsigned_attrs.as_ref())
        .and_then(|attributes| {
            attributes
                .iter()
                .find(|a| a.oid == super::oids::SIGNATURE_TIMESTAMP_TOKEN)
        })
        .and_then(|attribute| attribute.values.iter().next())
        .and_then(|value| der::Encode::to_der(value).ok());
    if let Some(token) = token {
        if let Ok(token_signed) = parse_signed_data(&token) {
            for certificate in certificates_of(&token_signed) {
                if let Ok(der) = der::Encode::to_der(&certificate) {
                    out.push(der);
                }
            }
        }
    }
    out
}

/// streams already in the store, by content, so nothing is stored twice.
fn existing_store(doc: &Document) -> Vec<(Vec<u8>, ObjectId)> {
    let Some(dss) = doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(doc, catalog, b"DSS"))
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for key in [b"Certs".as_slice(), b"CRLs", b"OCSPs"] {
        for id in references_in(doc, dss, key) {
            if let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) {
                let bytes = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                found.push((bytes, id));
            }
        }
    }
    found
}

/// a stream for each item, reusing one with the same bytes.
fn add_streams(
    doc: &mut Document,
    items: &[Vec<u8>],
    existing: &[(Vec<u8>, ObjectId)],
) -> Vec<ObjectId> {
    let mut ids: Vec<ObjectId> = Vec::new();
    for item in items {
        let reused = existing
            .iter()
            .find(|(bytes, _)| bytes == item)
            .map(|(_, id)| *id);
        let already = ids.iter().copied().find(|id| {
            doc.get_object(*id)
                .and_then(Object::as_stream)
                .is_ok_and(|stream| &stream.content == item)
        });
        let id = match reused.or(already) {
            Some(id) => id,
            None => doc.add_object(Stream::new(Dictionary::new(), item.clone())),
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn references(ids: &[ObjectId]) -> Object {
    Object::Array(ids.iter().map(|id| Object::Reference(*id)).collect())
}

fn vri_entry(entry: &Store) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Cert", references(&entry.certificates));
    if !entry.crls.is_empty() {
        dict.set("CRL", references(&entry.crls));
    }
    if !entry.ocsps.is_empty() {
        dict.set("OCSP", references(&entry.ocsps));
    }
    dict
}

fn merge(store: &mut Store, entry: Store) {
    let add = |list: &mut Vec<ObjectId>, more: Vec<ObjectId>| {
        for id in more {
            if !list.contains(&id) {
                list.push(id);
            }
        }
    };
    add(&mut store.certificates, entry.certificates);
    add(&mut store.crls, entry.crls);
    add(&mut store.ocsps, entry.ocsps);
}

/// writes /DSS into the catalog, keeping what an earlier store held.
fn write_store(doc: &mut Document, store: Store, vri: Dictionary) -> FormResult<()> {
    let old = doc
        .catalog()
        .ok()
        .and_then(|catalog| get_dict(doc, catalog, b"DSS"))
        .cloned()
        .unwrap_or_default();
    let mut dss = Dictionary::new();
    dss.set("Type", name("DSS"));
    for (key, ids) in [
        ("Certs", store.certificates),
        ("CRLs", store.crls),
        ("OCSPs", store.ocsps),
    ] {
        let mut all = references_in(doc, &old, key.as_bytes());
        for id in ids {
            if !all.contains(&id) {
                all.push(id);
            }
        }
        if !all.is_empty() {
            dss.set(key, references(&all));
        }
    }
    let mut all_vri = get_dict(doc, &old, b"VRI").cloned().unwrap_or_default();
    for (key, value) in vri.iter() {
        all_vri.set(key.clone(), value.clone());
    }
    dss.set("VRI", Object::Dictionary(all_vri));
    let dss_id = doc.add_object(Object::Dictionary(dss));
    let catalog = catalog_id(doc)?;
    doc.get_dictionary_mut(catalog)?
        .set("DSS", Object::Reference(dss_id));
    Ok(())
}
