//! checking the audit trail: each entry's own hash, the chain from entry
//! to entry, the file hash each entry recorded against the revisions that
//! really are in the file, that no revision took entries out, and which
//! entries a valid signature or document time stamp seals.

use lopdf::Document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::entry::{hex, AuditEntry, FIRST_PREVIOUS};
use super::store::read_entries;
use super::AUDIT_TRAIL_NOTE;
use crate::error::FormResult;
use crate::signing::{revision_ends, verify_document};

/// how the trail as a whole stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditVerdict {
    /// the document has no audit trail
    NoTrail,
    /// every check passed
    Intact,
    /// something was changed, removed or doesn't add up
    Broken,
}

/// one entry and what was found about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckedEntry {
    pub entry: AuditEntry,
    /// the entry's hash matches its contents
    pub hash_matches: bool,
    /// it names the entry before it correctly
    pub chain_intact: bool,
    /// the file hash it recorded is the file as it really was
    pub document_matches: bool,
    /// a valid signature or document time stamp made later covers it
    pub sealed: bool,
}

/// the trail and the verdict on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailReport {
    pub verdict: AuditVerdict,
    pub entries: Vec<CheckedEntry>,
    /// what doesn't add up, in sentences
    pub problems: Vec<String>,
    /// one or two sentences for the interface
    pub summary: String,
    /// what the trail can and can't know (`AUDIT_TRAIL_NOTE`)
    pub note: String,
}

/// the trail as each revision of the file held it.
struct Versions {
    /// (revision end, entries) for every revision that could be read
    revisions: Vec<(usize, Vec<AuditEntry>)>,
    problems: Vec<String>,
}

/// reads and checks the audit trail of a file. changes nothing.
pub fn audit_trail(doc_bytes: &[u8]) -> FormResult<AuditTrailReport> {
    Document::load_mem(doc_bytes)?;
    let versions = versions(doc_bytes);
    let Some((_, entries)) = versions.revisions.last().cloned() else {
        return Ok(report(AuditVerdict::NoTrail, Vec::new(), versions.problems));
    };
    if entries.is_empty() {
        return Ok(report(AuditVerdict::NoTrail, Vec::new(), versions.problems));
    }
    let mut problems = versions.problems.clone();
    problems.extend(append_only_problems(&versions));
    let seals = seal_ends(doc_bytes);
    let mut checked = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let previous = index.checked_sub(1).map(|before| &entries[before]);
        checked.push(check_entry(
            entry,
            previous,
            doc_bytes,
            &versions,
            &seals,
            &mut problems,
        ));
    }
    let verdict = if problems.is_empty() {
        AuditVerdict::Intact
    } else {
        AuditVerdict::Broken
    };
    Ok(report(verdict, checked, problems))
}

fn versions(bytes: &[u8]) -> Versions {
    let mut out = Versions {
        revisions: Vec::new(),
        problems: Vec::new(),
    };
    for end in revision_ends(bytes) {
        let Ok(doc) = Document::load_mem(&bytes[..end]) else {
            continue;
        };
        match read_entries(&doc) {
            Ok(entries) => out.revisions.push((end, entries)),
            Err(error) => out.problems.push(error.to_string()),
        }
    }
    out
}

/// each revision's trail must start with the whole trail of the one before.
fn append_only_problems(versions: &Versions) -> Vec<String> {
    let mut problems = Vec::new();
    for pair in versions.revisions.windows(2) {
        let (earlier, later) = (&pair[0].1, &pair[1].1);
        let kept = later.len() >= earlier.len() && later[..earlier.len()] == earlier[..];
        if !kept {
            problems.push(format!(
                "A later save changed or removed audit entries that were already recorded (the trail had {} entries, then {}).",
                earlier.len(),
                later.len()
            ));
        }
    }
    problems
}

/// where the file ends for each valid signature or document time stamp:
/// everything before such an end is sealed by it.
fn seal_ends(bytes: &[u8]) -> Vec<usize> {
    let Ok(checked) = verify_document(bytes) else {
        return Vec::new();
    };
    checked
        .signatures
        .iter()
        .zip(&checked.changes)
        .filter(|(report, _)| report.valid)
        .filter_map(|(_, changes)| changes.revision)
        .filter_map(|revision| checked.revisions.get(revision).map(|report| report.end))
        .collect()
}

fn check_entry(
    entry: &AuditEntry,
    previous: Option<&AuditEntry>,
    bytes: &[u8],
    versions: &Versions,
    seals: &[usize],
    problems: &mut Vec<String>,
) -> CheckedEntry {
    let number = entry.sequence;
    let hash_matches = entry.hash == entry.expected_hash();
    if !hash_matches {
        problems.push(format!("Entry {number} was changed after it was written."));
    }
    let expected_previous = previous
        .map(|entry| entry.hash.as_str())
        .unwrap_or(FIRST_PREVIOUS);
    let expected_sequence = previous.map(|entry| entry.sequence + 1).unwrap_or(1);
    let chain_intact = entry.previous_hash == expected_previous && number == expected_sequence;
    if !chain_intact {
        problems.push(format!(
            "Entry {number} doesn’t follow on from the entry before it."
        ));
    }
    // the revision that added this entry, and the file just before it
    let added_in = versions
        .revisions
        .iter()
        .position(|(_, entries)| entries.len() >= number as usize);
    let before_end = added_in
        .and_then(|index| index.checked_sub(1))
        .map(|index| versions.revisions[index].0);
    let document_matches =
        before_end.is_some_and(|end| hex(&Sha256::digest(&bytes[..end])) == entry.document_hash);
    if !document_matches {
        problems.push(format!(
            "Entry {number} records a version of the file that isn’t the one in the file’s history."
        ));
    }
    let added_end = added_in.map(|index| versions.revisions[index].0);
    let sealed = added_end.is_some_and(|end| seals.iter().any(|seal| *seal >= end));
    CheckedEntry {
        entry: entry.clone(),
        hash_matches,
        chain_intact,
        document_matches,
        sealed,
    }
}

fn report(
    verdict: AuditVerdict,
    entries: Vec<CheckedEntry>,
    problems: Vec<String>,
) -> AuditTrailReport {
    AuditTrailReport {
        summary: summary(verdict, &entries, &problems),
        verdict,
        entries,
        problems,
        note: AUDIT_TRAIL_NOTE.to_owned(),
    }
}

fn summary(verdict: AuditVerdict, entries: &[CheckedEntry], problems: &[String]) -> String {
    match verdict {
        AuditVerdict::NoTrail => "This document has no Letters audit trail.".to_owned(),
        AuditVerdict::Broken => format!("The audit trail doesn’t add up: {}", problems.join(" ")),
        AuditVerdict::Intact => {
            let sealed = entries.iter().filter(|entry| entry.sealed).count();
            let open = entries.len() - sealed;
            let mut text = format!(
                "The audit trail is intact: {} entries, each linked to the one before; {sealed} sealed by a signature or time stamp.",
                entries.len()
            );
            if open > 0 {
                text.push_str(&format!(
                    " The last {open} aren’t sealed yet, so they could still be changed without it showing until the next signature or time stamp."
                ));
            }
            text
        }
    }
}
