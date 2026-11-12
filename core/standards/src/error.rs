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

    #[error("The document couldn’t be saved. ({0})")]
    SaveFailed(String),
}

pub type StandardsResult<T> = Result<T, StandardsError>;

impl From<lopdf::Error> for StandardsError {
    fn from(error: lopdf::Error) -> Self {
        StandardsError::BadPdf(error.to_string())
    }
}
