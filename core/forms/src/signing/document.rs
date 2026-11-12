//! checking a whole signed document: every signature as `verify` checks
//! it, plus where each signature's time comes from (a time stamp authority
//! or the signer's clock), what kind of signature it is, and what changed
//! after it compared with what its author allowed.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use super::cms::{parse_signed_data, trim_to_der_length};
use super::mdp::CertificationLevel;
use super::oids;
use super::permissions::{declared_rules, judge, revision_of, SignatureChanges, SignedRevision};
use super::revisions::{revisions, RevisionReport};
use super::timestamp::{check_token, TimestampInfo, CLOCK_TIME_NOTE};
use super::verify::{verify, SignatureReport, DOCUMENT_TIMESTAMP};
use crate::error::FormResult;
use crate::objects::{get, get_name};
use crate::read::{kind_of, FieldKind};
use crate::tree::collect_fields;

/// what a signature is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureKind {
    /// an ordinary signature: someone signed the document as it was
    Approval,
    /// a certification: the first signature, saying which changes may follow
    Certification,
    /// a document time stamp: an authority vouches the file existed then
    DocumentTimestamp,
}

/// where a signature's time comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeSource {
    /// a time stamp authority vouches for it, and its token checks out
    TimestampAuthority,
    /// only the signer's computer says so
    SignerClock,
}

/// more about one signature than `SignatureReport` says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureDetails {
    pub field: String,
    pub kind: SignatureKind,
    /// the time stamp token, when the signature has one
    pub timestamp: Option<TimestampInfo>,
    pub time_source: TimeSource,
    /// one sentence for the interface about the time
    pub time_note: String,
}

/// everything Letters can say about a signed document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentVerification {
    /// one report per signature, as `verify` gives them
    pub signatures: Vec<SignatureReport>,
    /// in the same order as `signatures`
    pub details: Vec<SignatureDetails>,
    /// the certification level, when the document is certified
    pub certification: Option<CertificationLevel>,
    /// what the certification allows, in a sentence
    pub certification_note: Option<String>,
    /// every revision of the file and what it changed
    pub revisions: Vec<RevisionReport>,
    /// for each signature (same order), what changed after it and whether
    /// the author allowed it
    pub changes: Vec<SignatureChanges>,
}

/// checks every signature and document time stamp in a document.
pub fn verify_document(doc_bytes: &[u8]) -> FormResult<DocumentVerification> {
    let signatures = verify(doc_bytes)?;
    let doc = Document::load_mem(doc_bytes)?;
    let dictionaries = signature_dictionaries(&doc);
    let details: Vec<SignatureDetails> = dictionaries
        .iter()
        .map(|(field, dict)| details_of(&doc, doc_bytes, field, dict))
        .collect();
    let revisions = revisions(doc_bytes);
    let signed: Vec<SignedRevision> = dictionaries
        .iter()
        .map(|(field, dict)| signed_revision(&doc, field, dict, &revisions))
        .collect();
    let certification = signed.iter().find_map(|signature| signature.certifies);
    let changes = judge(&signed, &revisions);
    Ok(DocumentVerification {
        signatures,
        details,
        certification,
        certification_note: certification.map(|level| level.sentence().to_owned()),
        revisions,
        changes,
    })
}

/// the signature dictionaries of the form, in the order `verify` reports them.
pub(crate) fn signature_dictionaries(doc: &Document) -> Vec<(String, Dictionary)> {
    let mut found = Vec::new();
    for field in collect_fields(doc) {
        if kind_of(doc, &field) != Some(FieldKind::Signature) {
            continue;
        }
        if let Some(Object::Dictionary(signature)) = field.inherited(doc, b"V") {
            found.push((field.name.clone(), signature.clone()));
        }
    }
    found
}

/// where a signature sits among the revisions, and the rules it declares.
fn signed_revision(
    doc: &Document,
    field: &str,
    dict: &Dictionary,
    revisions: &[RevisionReport],
) -> SignedRevision {
    let covered_end = get(doc, dict, b"ByteRange")
        .and_then(|range| range.as_array().ok())
        .filter(|range| range.len() == 4)
        .and_then(|range| Some(range[2].as_i64().ok()? + range[3].as_i64().ok()?))
        .and_then(|end| usize::try_from(end).ok());
    let (certifies, lock) = declared_rules(doc, dict);
    SignedRevision {
        field: field.to_owned(),
        revision: covered_end.and_then(|end| revision_of(end, revisions)),
        certifies,
        lock,
    }
}

fn details_of(doc: &Document, bytes: &[u8], field: &str, dict: &Dictionary) -> SignatureDetails {
    let sub_filter = get_name(doc, dict, b"SubFilter").unwrap_or_default();
    let kind = if sub_filter == DOCUMENT_TIMESTAMP.as_bytes() {
        SignatureKind::DocumentTimestamp
    } else if declared_rules(doc, dict).0.is_some() {
        SignatureKind::Certification
    } else {
        SignatureKind::Approval
    };
    let contents = match get(doc, dict, b"Contents") {
        Some(Object::String(contents, _)) => contents.clone(),
        _ => Vec::new(),
    };
    let timestamp = match kind {
        SignatureKind::DocumentTimestamp => document_token(doc, bytes, dict, &contents),
        SignatureKind::Approval | SignatureKind::Certification => signature_token(&contents),
    };
    let (time_source, time_note) = time_of(timestamp.as_ref());
    SignatureDetails {
        field: field.to_owned(),
        kind,
        timestamp,
        time_source,
        time_note,
    }
}

fn time_of(timestamp: Option<&TimestampInfo>) -> (TimeSource, String) {
    match timestamp {
        Some(info) if info.is_valid() => {
            let authority = info
                .authority
                .as_deref()
                .and_then(super::keys::common_name)
                .unwrap_or_else(|| "a time stamp authority".to_owned());
            let time = info.time.clone().unwrap_or_default();
            let note = format!(
                "The time ({time}) is vouched for by the time stamp authority {authority}."
            );
            (TimeSource::TimestampAuthority, note)
        }
        Some(_) => {
            let note = format!("A time stamp is attached but doesn’t check out. {CLOCK_TIME_NOTE}");
            (TimeSource::SignerClock, note)
        }
        None => (TimeSource::SignerClock, CLOCK_TIME_NOTE.to_owned()),
    }
}

/// the token of a document time stamp, checked against its byte range.
fn document_token(
    doc: &Document,
    bytes: &[u8],
    dict: &Dictionary,
    contents: &[u8],
) -> Option<TimestampInfo> {
    let token = trim_to_der_length(contents)?;
    let range: Vec<usize> = get(doc, dict, b"ByteRange")?
        .as_array()
        .ok()?
        .iter()
        .filter_map(|value| value.as_i64().ok())
        .filter_map(|value| usize::try_from(value).ok())
        .collect();
    let [start, first_length, second_start, second_length] = range[..] else {
        return None;
    };
    let first = bytes.get(start..start.checked_add(first_length)?)?;
    let second = bytes.get(second_start..second_start.checked_add(second_length)?)?;
    Some(check_token(token, &[first, second]).info)
}

/// the signature time stamp in a cms signature's unsigned attributes,
/// checked against the signature value it covers (rfc 3161 appendix a).
pub(crate) fn signature_token(contents: &[u8]) -> Option<TimestampInfo> {
    let signed = parse_signed_data(contents).ok()?;
    let signer = signed.signer_infos.0.iter().next()?;
    let attribute = signer
        .unsigned_attrs
        .as_ref()?
        .iter()
        .find(|attribute| attribute.oid == oids::SIGNATURE_TIMESTAMP_TOKEN)?;
    let token = der::Encode::to_der(attribute.values.iter().next()?).ok()?;
    let value = signer.signature.as_bytes();
    Some(check_token(&token, &[value]).info)
}
