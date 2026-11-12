//! one entry of the audit trail and the hash that chains it to the one
//! before. the hash covers every other part of the entry, written as json
//! in a fixed field order, so the same entry always gives the same hash.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// what happened. names are the ones the person set in Letters; Letters
/// can't confirm who was really at the keyboard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AuditEvent {
    /// the document was made in Letters
    Created,
    /// the form was prepared (fields added, rules set) for others to fill
    Prepared {
        #[serde(default)]
        note: Option<String>,
    },
    /// someone signed
    Signed {
        field: String,
        signer: String,
        certificate_subject: String,
        /// the sha-256 of the signer's certificate, in hex
        certificate_fingerprint: String,
        signing_time: String,
        /// "a time stamp authority" or "the signer's computer clock"
        time_source: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        location: Option<String>,
    },
    /// the document was saved after it had been signed
    Saved,
    /// a document time stamp was added
    Timestamped { authority: String },
    /// anything else the app wants on record
    Note { text: String },
}

/// one entry, as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// 1 for the first entry, counting up
    pub sequence: u32,
    pub event: AuditEvent,
    /// the name the person set in Letters
    pub actor: String,
    /// when, in utc, by this computer's clock
    pub time: String,
    /// the sha-256 of the whole file as it was just before this entry was added
    pub document_hash: String,
    /// the hash of the entry before, or 64 zeros for the first
    pub previous_hash: String,
    /// sha-256 of all of the above
    pub hash: String,
}

/// the previous hash of the first entry.
pub(crate) const FIRST_PREVIOUS: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

impl AuditEntry {
    /// the hash this entry should have: sha-256 over its json with the
    /// hash itself left empty.
    pub fn expected_hash(&self) -> String {
        let mut unhashed = self.clone();
        unhashed.hash = String::new();
        let text = serde_json::to_string(&unhashed).unwrap_or_default();
        hex(&Sha256::digest(text.as_bytes()))
    }

    /// makes the next entry after `previous`, hashed.
    pub(crate) fn next(
        previous: Option<&AuditEntry>,
        event: AuditEvent,
        actor: &str,
        time: String,
        document: &[u8],
    ) -> AuditEntry {
        let mut entry = AuditEntry {
            sequence: previous.map(|entry| entry.sequence + 1).unwrap_or(1),
            event,
            actor: actor.to_owned(),
            time,
            document_hash: hex(&Sha256::digest(document)),
            previous_hash: previous
                .map(|entry| entry.hash.clone())
                .unwrap_or_else(|| FIRST_PREVIOUS.to_owned()),
            hash: String::new(),
        };
        entry.hash = entry.expected_hash();
        entry
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_chain_by_hash() {
        let first = AuditEntry::next(
            None,
            AuditEvent::Created,
            "Ada",
            "2026-11-12 09:00 UTC".into(),
            b"one",
        );
        let second = AuditEntry::next(
            Some(&first),
            AuditEvent::Saved,
            "Ada",
            "2026-11-12 09:05 UTC".into(),
            b"two",
        );
        assert_eq!(first.previous_hash, FIRST_PREVIOUS);
        assert_eq!(second.previous_hash, first.hash);
        assert_eq!(second.sequence, 2);
        assert_eq!(second.hash, second.expected_hash());
        let mut altered = second.clone();
        altered.actor = "Eve".into();
        assert_ne!(altered.expected_hash(), altered.hash);
    }
}
