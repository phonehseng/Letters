//! how a session saves once content was removed or a password was set.
//!
//! an ordinary save copies the working copy as it is. that is not good
//! enough after a redaction: lopdf keeps objects nothing uses any more
//! (the image and the content stream that held the secret) in the file
//! it writes, so the secret would still be in it. and a password has to
//! be applied to the file that is written, not to the working copy the
//! viewer shows. so these sessions save through a writer that loads the
//! current version, locks it if asked, and writes it from scratch with
//! the engine's `save_clean`.

use std::fs;
use std::path::Path;

use letters_redact::lopdf::Document;
use letters_redact::{encrypt, save_clean, LockOptions, RedactError};

use crate::session::SaveWriter;

/// what save does about a password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockOnSave {
    /// the file is saved without a password
    Unlocked,
    /// the file is locked with these settings when it is saved
    Lock(LockOptions),
    /// the file was unlocked with its owner password, so the password
    /// needed to open it is not known. save waits until the person
    /// chooses, rather than quietly dropping the password
    Undecided,
}

impl LockOnSave {
    /// true when the saved file will need a password to open.
    pub fn needs_open_password(&self) -> bool {
        match self {
            Self::Lock(options) => !options.open_password.is_empty(),
            // the file had one, and it isn't decided away yet
            Self::Undecided => true,
            Self::Unlocked => false,
        }
    }
}

/// the sentence save shows while the password is undecided.
pub const UNDECIDED_MESSAGE: &str =
    "Choose whether it keeps a password to open under Protect in Tools, then save again.";

/// a writer for `EditSession::write_saves_with`.
pub fn clean_writer(lock: LockOnSave) -> SaveWriter {
    Box::new(move |current: &Path| write_clean(current, &lock))
}

/// the bytes of a clean save of the version in `current`.
pub fn write_clean(current: &Path, lock: &LockOnSave) -> Result<Vec<u8>, String> {
    if *lock == LockOnSave::Undecided {
        return Err(UNDECIDED_MESSAGE.to_owned());
    }
    let bytes = fs::read(current).map_err(|error| error.to_string())?;
    let mut document = letters_redact::open(&bytes).map_err(|error| error.to_string())?;
    if let LockOnSave::Lock(options) = lock {
        encrypt(&mut document, options).map_err(|error| error.to_string())?;
    }
    save_clean(&mut document).map_err(|error| error.to_string())
}

/// writes a document from scratch in memory and loads it again, so the
/// version stored in the cache after a redaction or a scrub holds no
/// unused objects either. the engine's own clean save does the work.
pub fn rewrite_clean(document: &mut Document) -> Result<(), RedactError> {
    let bytes = save_clean(document)?;
    *document = letters_redact::open(&bytes)?;
    Ok(())
}
