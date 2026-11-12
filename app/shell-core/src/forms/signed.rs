//! signing a document through its edit session.
//!
//! a digital signature covers the file's exact bytes. every other edit in
//! Letters loads the document with lopdf and writes it out again whole,
//! which moves those bytes and breaks the signature. so signing is done
//! differently:
//!
//! 1. the document must be saved first, so the bytes signed are the
//!    bytes of the person's file. when saves are rewritten (Protect's
//!    clean save after a redaction) the file on disk is what the clean
//!    writer made, not the working copy, so the file itself is signed:
//!    the working copy may still hold unused objects with removed content.
//! 2. the engine's `sign` appends an incremental update to those bytes.
//! 3. the result is made the new version and written over the person's
//!    file exactly as it is (`EditSession::adopt_exact_bytes`), never
//!    through lopdf or the clean writer.
//! 4. the session now protects the file: a later Save that would write a
//!    rewritten version over it is refused with a message pointing to
//!    Save As, and the ui warns before the first change.
//!
//! a password (or restrictions) can't go together with a signature yet:
//! the engine signs only unencrypted files, and locking a signed file
//! afterwards rewrites it and breaks the signature. so a document that is,
//! or will be saved, locked is refused with a plain sentence instead of
//! silently losing its password.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::protect::clean_save::LockOnSave;
use crate::protect::protection::Protection;
use crate::protect::unlock::inspect_lock;
use crate::session::{EditSession, SessionError};

/// why a document wasn't signed. in every case nothing changed.
#[derive(Debug, PartialEq)]
pub enum SignError<E> {
    /// there are unsaved changes, so the saved file isn't what is shown
    NotSaved,
    /// the file has (or will be saved with) a password or restrictions
    PasswordProtected,
    /// the saved file couldn't be read
    CannotRead(String),
    /// the engine refused (a wrong password, an already signed field...)
    Sign(E),
    /// the signed file couldn't be stored, shown or written
    Session(SessionError),
}

/// the sentence for a document with a password, also used by the ui.
pub const PASSWORD_MESSAGE: &str = "Letters can’t sign a document that has a password or restrictions yet. Remove them under Protect in Tools, save, then sign. Adding a password after signing would change the file and break the signature.";

impl<E: fmt::Display> fmt::Display for SignError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSaved => write!(
                f,
                "Save the document before signing it. A signature covers the saved file exactly."
            ),
            Self::PasswordProtected => write!(f, "{PASSWORD_MESSAGE}"),
            Self::CannotRead(detail) => write!(
                f,
                "The document couldn’t be read for signing. Try again. ({detail})"
            ),
            Self::Sign(error) => error.fmt(f),
            Self::Session(error) => error.fmt(f),
        }
    }
}

/// whether a document's protection (Protect's record) stands in the way
/// of signing: a password or restrictions set for the next save, a choice
/// about the password still open, or a file that was opened or unlocked
/// with one and not freed of it.
pub fn password_blocks_signing(protection: &Protection) -> bool {
    match &protection.lock {
        Some(LockOnSave::Lock(_)) | Some(LockOnSave::Undecided) => true,
        Some(LockOnSave::Unlocked) => false,
        None => protection.opened_with_password || protection.unlocked_from.is_some(),
    }
}

/// signs the saved document and makes the signed file the saved file.
///
/// `password_blocked` comes from `password_blocks_signing`. `sign` gets
/// the saved file's bytes and returns the signed bytes (the shell passes
/// core/forms' `sign`). `reload` shows a version, as for edits. returns
/// what `reload` returned for the signed version.
pub fn sign_saved_document<T, E>(
    session: &mut EditSession,
    password_blocked: bool,
    sign: impl FnOnce(&[u8]) -> Result<Vec<u8>, E>,
    reload: impl Fn(&Path) -> Result<T, String>,
) -> Result<T, SignError<E>> {
    if password_blocked {
        return Err(SignError::PasswordProtected);
    }
    if session.is_dirty() {
        return Err(SignError::NotSaved);
    }
    // not dirty: the file on disk is what the last save wrote. without a
    // writer that is the working copy byte for byte; with one it is the
    // clean rewrite, which is the only thing that may be signed
    let source = if session.saves_are_rewritten() {
        session.original_path()
    } else {
        session.working_copy()
    };
    let saved = fs::read(source).map_err(|error| SignError::CannotRead(error.to_string()))?;
    // a locked file on disk (however it got there) is never signed or rewritten
    if matches!(inspect_lock(&saved), Ok(Some(_))) {
        return Err(SignError::PasswordProtected);
    }
    let signed = sign(&saved).map_err(SignError::Sign)?;
    session
        .adopt_exact_bytes(&signed, reload)
        .map_err(SignError::Session)
}
