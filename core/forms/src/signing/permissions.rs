//! judging the changes after each signature against what the author
//! allowed: the certification level (docmdp) and the field locks
//! (fieldmdp) of the signatures made so far. the result is one of "nothing
//! changed", "filled in and signed as intended", "changed in ways the
//! author did not allow", or "changed, and the author set no limits".

use lopdf::{Dictionary, Document};
use serde::{Deserialize, Serialize};

use super::mdp::{reference_level, CertificationLevel, FieldLock};
use super::revisions::{Change, ChangeKind, RevisionReport};
use crate::objects::{get_dict, get_name, resolve};

/// how the changes after one signature stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeVerdict {
    /// nothing was changed after this signature
    Unchanged,
    /// everything changed afterwards was allowed (or was only signatures)
    AsIntended,
    /// something changed that the certification or a field lock forbids
    NotAllowed,
    /// changed, and the author set no limits
    NoLimits,
}

/// the changes after one signature and how they stand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureChanges {
    pub field: String,
    /// the revision this signature closed, if found
    pub revision: Option<usize>,
    pub verdict: ChangeVerdict,
    /// one sentence for the interface
    pub sentence: String,
    /// every change made after this signature
    pub changes: Vec<Change>,
    /// the ones the author did not allow
    pub not_allowed: Vec<Change>,
}

/// one signature as the judge needs it.
pub(crate) struct SignedRevision {
    pub field: String,
    pub revision: Option<usize>,
    pub certifies: Option<CertificationLevel>,
    pub lock: Option<FieldLock>,
}

/// the docmdp level and fieldmdp lock a signature dictionary declares.
pub(crate) fn declared_rules(
    doc: &Document,
    signature: &Dictionary,
) -> (Option<CertificationLevel>, Option<FieldLock>) {
    let level = reference_level(doc, signature);
    let lock = crate::objects::get_array(doc, signature, b"Reference")
        .into_iter()
        .flatten()
        .filter_map(|reference| resolve(doc, reference).as_dict().ok())
        .filter(|reference| {
            get_name(doc, reference, b"TransformMethod").as_deref() == Some(b"FieldMDP")
        })
        .find_map(|reference| {
            get_dict(doc, reference, b"TransformParams")
                .and_then(|params| FieldLock::read(doc, params))
        });
    (level, lock)
}

/// the revision whose end matches where a signature's bytes end. a
/// signature covers its revision up to the end, so the ends meet within
/// the line break after %%EOF.
pub(crate) fn revision_of(covered_end: usize, revisions: &[RevisionReport]) -> Option<usize> {
    revisions
        .iter()
        .find(|revision| revision.end.abs_diff(covered_end) <= 2 || revision.end >= covered_end)
        .map(|revision| revision.index)
}

/// changes that are always fine: they add evidence, not content
/// (etsi en 319 142-1 5.4 allows the security store and document time
/// stamps even under certification level 1).
fn always_allowed(kind: ChangeKind) -> bool {
    matches!(
        kind,
        ChangeKind::ValidationData | ChangeKind::DocumentTimestamp | ChangeKind::AuditTrail
    )
}

/// whether a certification level allows a kind of change
/// (iso 32000 12.8.2.2, table 257).
fn level_allows(level: CertificationLevel, kind: ChangeKind) -> bool {
    if always_allowed(kind) {
        return true;
    }
    let filling = matches!(
        kind,
        ChangeKind::FieldValue | ChangeKind::Signature | ChangeKind::Metadata
    );
    match level {
        CertificationLevel::NoChanges => false,
        CertificationLevel::FormFillingAndSigning => filling,
        CertificationLevel::FormFillingSigningAndComments => {
            filling || kind == ChangeKind::Annotation
        }
    }
}

/// judges the changes after every signature.
pub(crate) fn judge(
    signed: &[SignedRevision],
    revisions: &[RevisionReport],
) -> Vec<SignatureChanges> {
    let certification = signed
        .iter()
        .find_map(|signature| Some((signature.certifies?, signature.revision?)));
    signed
        .iter()
        .map(|signature| judge_one(signature, signed, certification, revisions))
        .collect()
}

fn judge_one(
    signature: &SignedRevision,
    signed: &[SignedRevision],
    certification: Option<(CertificationLevel, usize)>,
    revisions: &[RevisionReport],
) -> SignatureChanges {
    let after = signature
        .revision
        .map(|index| index + 1)
        .unwrap_or(revisions.len());
    let mut changes = Vec::new();
    let mut not_allowed = Vec::new();
    for revision in revisions.iter().filter(|revision| revision.index >= after) {
        for change in &revision.changes {
            if !is_allowed(change, revision.index, signed, certification) {
                not_allowed.push(change.clone());
            }
            changes.push(change.clone());
        }
    }
    let has_rules = certification.is_some() || signed.iter().any(|other| other.lock.is_some());
    let only_evidence = changes
        .iter()
        .all(|change| always_allowed(change.kind) || change.kind == ChangeKind::Signature);
    let verdict = if changes.is_empty() {
        ChangeVerdict::Unchanged
    } else if !not_allowed.is_empty() {
        ChangeVerdict::NotAllowed
    } else if has_rules || only_evidence {
        ChangeVerdict::AsIntended
    } else {
        ChangeVerdict::NoLimits
    };
    SignatureChanges {
        field: signature.field.clone(),
        revision: signature.revision,
        verdict,
        sentence: sentence(verdict, &changes, &not_allowed),
        changes,
        not_allowed,
    }
}

/// a change made in `revision` is allowed when the certification (if it
/// came before) allows its kind, and no earlier signature locked its field.
fn is_allowed(
    change: &Change,
    revision: usize,
    signed: &[SignedRevision],
    certification: Option<(CertificationLevel, usize)>,
) -> bool {
    if let Some((level, certified_at)) = certification {
        if revision > certified_at && !level_allows(level, change.kind) {
            return false;
        }
    }
    let Some(field) = &change.field else {
        return true;
    };
    if change.kind != ChangeKind::FieldValue && change.kind != ChangeKind::FormDefinition {
        return true;
    }
    !signed.iter().any(|other| {
        let locked_before = other.revision.is_some_and(|locked_at| locked_at < revision);
        locked_before && other.lock.as_ref().is_some_and(|lock| lock.covers(field))
    })
}

fn sentence(verdict: ChangeVerdict, changes: &[Change], not_allowed: &[Change]) -> String {
    let list = |items: &[Change]| {
        items
            .iter()
            .map(|change| change.description.clone())
            .collect::<Vec<_>>()
            .join("; ")
    };
    match verdict {
        ChangeVerdict::Unchanged => "Nothing was changed after this signature.".to_owned(),
        ChangeVerdict::AsIntended => {
            format!("Filled in and signed as intended: every later change was allowed ({}).", list(changes))
        }
        ChangeVerdict::NotAllowed => {
            format!("Changed in ways the author did not allow: {}.", list(not_allowed))
        }
        ChangeVerdict::NoLimits => format!(
            "Changed after this signature ({}). The author set no limits on changes, but this signature doesn’t cover them.",
            list(changes)
        ),
    }
}
