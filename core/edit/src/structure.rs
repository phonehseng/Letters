//! keeping a document's structure intact around content edits: telling
//! when an edit would break digital signatures, and checking that marked
//! content stays balanced.
//!
//! the edits themselves are written so structure survives: they replace
//! single operators and wrap them in q ... Q, never moving a BDC or EMC,
//! so marked-content sequences stay balanced and every MCID keeps the
//! content it marks (moved content moves with its operators, MCID and all).

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::error::EditError;
use crate::interpret::MarkedRecord;
use crate::layout::PageModel;
use crate::lexer;
use crate::rewrite::Plan;
use crate::summary::EditConsent;
use crate::writer;

/// fields deeper than this are not followed (a loop must not hang)
const MAX_FIELD_DEPTH: usize = 32;

/// one digital signature in the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    /// the signature field's name
    pub field: String,
    /// who signed, when the signature says
    pub signer: Option<String>,
    /// when, as written in the signature (a pdf date such as D:2026...)
    pub signed_at: Option<String>,
}

/// what the app should know before changing a document's content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCheck {
    /// encrypted: it must be unlocked before any edit
    pub locked: bool,
    pub signatures: Vec<SignatureInfo>,
    /// the sentence to show and confirm, when there is something to say
    pub message: Option<String>,
}

/// looks for anything that makes content editing need care: a password
/// lock, or digital signatures that an edit would make invalid.
pub fn content_edit_check(doc: &Document) -> EditCheck {
    let locked = doc.is_encrypted();
    let signatures = if locked { Vec::new() } else { signatures(doc) };
    let message = if locked {
        Some(EditError::Locked.to_string())
    } else if !signatures.is_empty() {
        Some(EditError::SignaturesWouldBreak(signed_by(&signatures)).to_string())
    } else {
        None
    };
    EditCheck {
        locked,
        signatures,
        message,
    }
}

/// refuses an edit of a locked file, and of a signed file unless the
/// person agreed that the signatures will no longer be valid.
pub(crate) fn guard(doc: &Document, consent: &EditConsent) -> Result<(), EditError> {
    if doc.is_encrypted() {
        return Err(EditError::Locked);
    }
    if consent.invalidate_signatures {
        return Ok(());
    }
    let found = signatures(doc);
    if found.is_empty() {
        Ok(())
    } else {
        Err(EditError::SignaturesWouldBreak(signed_by(&found)))
    }
}

fn signed_by(signatures: &[SignatureInfo]) -> String {
    match signatures
        .iter()
        .find_map(|signature| signature.signer.clone())
    {
        Some(signer) => format!(" by {signer}"),
        None => String::new(),
    }
}

/// every signed signature field of the form.
fn signatures(doc: &Document) -> Vec<SignatureInfo> {
    let Some(form) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| resolve(doc, form).as_dict().ok())
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if let Ok(Object::Array(fields)) = form.get(b"Fields").map(|fields| resolve(doc, fields)) {
        for field in fields {
            collect_signatures(doc, field, None, String::new(), 0, &mut found);
        }
    }
    found
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    crate::pdf_values::resolve(doc, object)
}

fn collect_signatures(
    doc: &Document,
    field: &Object,
    inherited_type: Option<Vec<u8>>,
    parent_name: String,
    depth: usize,
    found: &mut Vec<SignatureInfo>,
) {
    let Ok(dict) = resolve(doc, field).as_dict() else {
        return;
    };
    if depth > MAX_FIELD_DEPTH {
        return;
    }
    let field_type = dict
        .get(b"FT")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(<[u8]>::to_vec)
        .or(inherited_type);
    let own_name = text_of(doc, dict, b"T").unwrap_or_default();
    let name = match (parent_name.is_empty(), own_name.is_empty()) {
        (true, _) => own_name,
        (false, true) => parent_name,
        (false, false) => format!("{parent_name}.{own_name}"),
    };
    if field_type.as_deref() == Some(b"Sig") {
        if let Some(value) = dict.get(b"V").ok().map(|value| resolve(doc, value)) {
            if let Ok(signature) = value.as_dict() {
                found.push(SignatureInfo {
                    field: name.clone(),
                    signer: text_of(doc, signature, b"Name"),
                    signed_at: text_of(doc, signature, b"M"),
                });
            }
        }
    }
    if let Ok(Object::Array(kids)) = dict.get(b"Kids").map(|kids| resolve(doc, kids)) {
        for kid in kids {
            collect_signatures(doc, kid, field_type.clone(), name.clone(), depth + 1, found);
        }
    }
}

fn text_of(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    match resolve(doc, dict.get(key).ok()?) {
        Object::String(bytes, _) => Some(decode_text(bytes)),
        _ => None,
    }
}

/// a pdf text string: utf-16 with a byte order mark, else one byte per
/// character.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|byte| char::from(*byte)).collect()
    }
}

/// a pdf text string for any text: plain bytes when every character fits
/// in one byte, else utf-16 with a byte order mark.
pub(crate) fn encode_text(text: &str) -> Vec<u8> {
    if text.chars().all(|c| (c as u32) < 0x80) {
        return text.as_bytes().to_vec();
    }
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

/// gives the new text to the /ActualText and /Alt of each marked-content
/// sequence that holds nothing but the edited glyphs. a sequence that
/// also holds other text keeps what it says, since only part of it
/// changed. returns how many sequences were updated.
pub(crate) fn update_actual_text(
    model: &PageModel,
    plan: &mut Plan,
    edited: &HashSet<usize>,
    text: &str,
) -> usize {
    let run = &model.run;
    let sequences: BTreeSet<usize> = edited
        .iter()
        .filter_map(|glyph| run.shows[run.glyphs[*glyph].show].marked)
        .collect();
    let mut updated = 0;
    for sequence in sequences {
        let record = &run.marked[sequence];
        let Some(properties) = inline_properties(model, record) else {
            continue;
        };
        if !(properties.has(b"ActualText") || properties.has(b"Alt"))
            || !holds_only(model, sequence, edited)
        {
            continue;
        }
        let mut properties = properties;
        for key in [b"ActualText".as_slice(), b"Alt"] {
            if properties.has(key) {
                properties.set(
                    key.to_vec(),
                    Object::String(encode_text(text), lopdf::StringFormat::Literal),
                );
            }
        }
        let operands = [
            Object::Name(record.tag.clone()),
            Object::Dictionary(properties),
        ];
        plan.op(record.source, record.begin_op).replace = Some(writer::operation("BDC", &operands));
        updated += 1;
    }
    updated
}

/// the properties written inside a BDC operator (not a named list from
/// the resources, which other sequences may share).
fn inline_properties(model: &PageModel, record: &MarkedRecord) -> Option<Dictionary> {
    let operation = &model.run.sources[record.source].operations[record.begin_op];
    match operation.operands.get(1) {
        Some(Object::Dictionary(dict)) => Some(dict.clone()),
        _ => None,
    }
}

/// whether every glyph drawn inside a sequence is one of the edited ones.
fn holds_only(model: &PageModel, sequence: usize, edited: &HashSet<usize>) -> bool {
    let run = &model.run;
    run.glyphs
        .iter()
        .enumerate()
        .filter(|(_, glyph)| run.shows[glyph.show].marked == Some(sequence))
        .all(|(index, _)| edited.contains(&index))
}

/// true when every BMC/BDC in a content stream has its EMC and none
/// closes what isn't open. edits must keep this true.
pub fn marked_content_balanced(content: &[u8]) -> bool {
    let mut depth: i64 = 0;
    for operation in lexer::parse(content) {
        match operation.operator.as_str() {
            "BMC" | "BDC" => depth += 1,
            "EMC" => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// the MCIDs a content stream marks, in order.
pub fn marked_content_ids(content: &[u8]) -> Vec<i64> {
    lexer::parse(content)
        .into_iter()
        .filter(|operation| operation.operator == "BDC")
        .filter_map(|operation| match operation.operands.get(1) {
            Some(Object::Dictionary(dict)) => dict.get(b"MCID").ok()?.as_i64().ok(),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_is_checked() {
        assert!(marked_content_balanced(b"/P <</MCID 0>> BDC (a) Tj EMC"));
        assert!(!marked_content_balanced(b"/P BMC (a) Tj"));
        assert!(!marked_content_balanced(b"EMC /P BMC"));
        assert_eq!(
            marked_content_ids(b"/P <</MCID 3>> BDC EMC /Span <</MCID 4>> BDC EMC"),
            [3, 4]
        );
    }

    #[test]
    fn text_strings_round_trip() {
        for text in ["plain", "café ☕"] {
            assert_eq!(decode_text(&encode_text(text)), text);
        }
    }
}
