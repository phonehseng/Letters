//! signing in turn: a form with several signature fields signed in a set
//! order, each signer's fields locked once they sign. the order is the
//! order of the signature fields in the form (/AcroForm /Fields), so any
//! pdf reader sees the same order; nothing outside the file is kept.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use super::mdp::{lock_of_field, FieldLock};
use crate::add::ensure_form;
use crate::error::{FormError, FormResult};
use crate::objects::{check_document, get_text, references_in};
use crate::read::{kind_of, FieldKind};
use crate::tree::{collect_fields, find_field};

/// one signature field in the signing order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub field: String,
    pub signed: bool,
    /// the name in the signature, once signed
    pub signer: Option<String>,
    /// the fields this signature locks
    pub locks: Option<FieldLock>,
    /// the first field not signed yet
    pub is_next: bool,
}

/// the signature fields in signing order, and who has signed.
pub fn signing_workflow(doc: &Document) -> FormResult<Vec<WorkflowStep>> {
    check_document(doc)?;
    let mut steps: Vec<WorkflowStep> = Vec::new();
    for field in collect_fields(doc) {
        if kind_of(doc, &field) != Some(FieldKind::Signature) {
            continue;
        }
        let signature = field
            .inherited(doc, b"V")
            .and_then(|value| value.as_dict().ok());
        steps.push(WorkflowStep {
            field: field.name.clone(),
            signed: signature.is_some(),
            signer: signature.and_then(|dict| get_text(doc, dict, b"Name")),
            locks: lock_of_field(doc, field.id),
            is_next: false,
        });
    }
    if let Some(next) = steps.iter_mut().find(|step| !step.signed) {
        next.is_next = true;
    }
    Ok(steps)
}

/// puts signature fields in the order they are to be signed. they must be
/// top-level fields; other fields keep their places.
pub fn set_signing_order(doc: &mut Document, order: &[String]) -> FormResult<()> {
    check_document(doc)?;
    let form = ensure_form(doc)?;
    let top_level = references_in(doc, doc.get_dictionary(form)?, b"Fields");
    let mut ordered: Vec<ObjectId> = Vec::new();
    for name in order {
        let field = find_field(doc, name)?;
        if kind_of(doc, &field) != Some(FieldKind::Signature) {
            return Err(FormError::NotASignatureField(name.clone()));
        }
        if !top_level.contains(&field.id) {
            return Err(FormError::WrongValueKind {
                field: name.clone(),
                hint: "Only signature fields that aren’t inside a group can be put in order."
                    .into(),
            });
        }
        ordered.push(field.id);
    }
    // signature fields keep the places signature fields had: the listed
    // ones first, in the new order, then the others as they were
    let signature_ids: Vec<ObjectId> = top_level
        .iter()
        .copied()
        .filter(|id| is_signature_field(doc, *id))
        .collect();
    let mut sequence = ordered.clone();
    sequence.extend(signature_ids.iter().filter(|id| !ordered.contains(id)));
    let mut queue = sequence.into_iter();
    let rearranged: Vec<Object> = top_level
        .iter()
        .map(|id| {
            let place = if signature_ids.contains(id) {
                queue.next()
            } else {
                None
            };
            Object::Reference(place.unwrap_or(*id))
        })
        .collect();
    doc.get_dictionary_mut(form)?
        .set("Fields", Object::Array(rearranged));
    Ok(())
}

fn is_signature_field(doc: &Document, id: ObjectId) -> bool {
    collect_fields(doc)
        .iter()
        .any(|field| field.id == id && kind_of(doc, field) == Some(FieldKind::Signature))
}

/// refuses to sign a field while an earlier one in the order is unsigned.
pub(crate) fn check_turn(doc: &Document, field: &str) -> FormResult<()> {
    let steps = signing_workflow(doc)?;
    for step in &steps {
        if step.field == field {
            return Ok(());
        }
        if !step.signed {
            return Err(FormError::SignOutOfOrder {
                field: field.to_owned(),
                before: step.field.clone(),
            });
        }
    }
    Ok(())
}
