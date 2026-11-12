//! redaction, hidden information and passwords, as the app runs them.
//!
//! the engine is core/redact. it works on a document in memory; this
//! module holds the rules the app adds around it, where a mistake would
//! leave a secret on someone's disk:
//!
//! - `clean_save.rs`: after a redaction, a scrub or with a password, a
//!   session saves by writing the whole file from scratch (the engine's
//!   `save_clean`), locking it again first when it should be locked. the
//!   session can't be switched back to plain copying (see
//!   `EditSession::write_saves_with`), and the versions kept for undo are
//!   deleted once a redaction is saved.
//! - `unlock.rs`: reading how a file is locked, and unlocking it for
//!   editing with its password.
//! - `protection.rs`: what each open document's protection is: the
//!   password save will apply, how the file was locked, and whether
//!   anything was removed. the shell keeps one table of these.

pub mod clean_save;
pub mod permissions;
pub mod protection;
pub mod unlock;

#[cfg(test)]
mod tests;
