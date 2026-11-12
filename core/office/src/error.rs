use thiserror::Error;

/// everything that can go wrong reading or writing an office document.
///
/// like the pdf errors, these are shown to the person as they are: a plain
/// sentence that says what happened and, where it helps, what to do.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum OfficeError {
    #[error("This file isn’t a Word or OpenDocument file, or it is damaged. ({0})")]
    NotAPackage(String),

    #[error("This file is damaged: {0}")]
    Damaged(String),

    #[error("This file is damaged: part of it ({part}) can’t be read. {detail}")]
    BadXml { part: String, detail: String },

    #[error(
        "Letters can’t open {0} yet. Only text documents (.docx and .odt) can be opened for now."
    )]
    NotSupportedYet(String),

    #[error("This is an older Word file (.doc). Save it as .docx in Word or another word processor, then open that.")]
    LegacyWordFormat,

    #[error("This document is locked with a password.")]
    Encrypted,

    #[error("This file is too large to open safely. ({0})")]
    TooLarge(String),

    #[error("The document couldn’t be saved. ({0})")]
    WriteFailed(String),
}

impl OfficeError {
    pub(crate) fn bad_xml(part: &str, detail: impl Into<String>) -> OfficeError {
        OfficeError::BadXml {
            part: part.to_string(),
            detail: detail.into(),
        }
    }
}
