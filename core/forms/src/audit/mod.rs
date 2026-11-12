//! a tamper-evident audit trail kept inside the pdf, with no server.
//!
//! the trail is an append-only list of what happened to the document in
//! Letters on this computer: prepared, signed, saved, time stamped. it is
//! stored as an embedded file (letters-audit-trail.json, iso 32000 7.11.4)
//! and each new entry is an incremental update. every entry carries the
//! sha-256 of the file as it was and of the entry before it, so changing
//! or removing one breaks the chain; and the next signature or document
//! time stamp covers the trail's bytes, so the chain itself is sealed.
//!
//! what it cannot know is said plainly in `AUDIT_TRAIL_NOTE`.

mod check;
mod entry;
mod page;
mod store;

pub use check::{audit_trail, AuditTrailReport, AuditVerdict, CheckedEntry};
pub use entry::{AuditEntry, AuditEvent};
pub use page::add_audit_page;
pub use store::append_audit_entry;

/// shown wherever the audit trail is, word for word.
pub const AUDIT_TRAIL_NOTE: &str = "Letters records what happens to this document in Letters on this computer: who prepared, signed and saved it, and when. It can’t know who opened or viewed the file anywhere else, and it doesn’t use a tracking service.";
