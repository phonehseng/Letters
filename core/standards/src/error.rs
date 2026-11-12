//! the errors of standards checks and conversions, each a sentence the app
//! shows to the person as it is.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum StandardsError {
    #[error("This file can’t be read as a PDF. ({0})")]
    BadPdf(String),

    #[error(
        "This PDF is locked with a password. Unlock it first; archival PDFs can’t be encrypted."
    )]
    Encrypted,

    /// an encrypted document handed to a tool that rewrites it: its objects
    /// can't be read without the password, so the copy would be empty
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("The document couldn’t be saved. ({0})")]
    SaveFailed(String),
}

pub type StandardsResult<T> = Result<T, StandardsError>;

impl From<lopdf::Error> for StandardsError {
    fn from(error: lopdf::Error) -> Self {
        StandardsError::BadPdf(error.to_string())
    }
}
