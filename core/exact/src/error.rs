//! the errors of a conversion, written as sentences the person can act on
//! (the app shows them as they are, in the style of `core/pdf`).

use thiserror::Error;

/// everything that can stop a conversion.
///
/// the app shows these messages as they are, so they are written for the
/// person using it: a plain sentence, no blame, and where possible what to
/// do next. problems that only affect part of a document are not errors:
/// they are warnings in the conversion report.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ExactError {
    #[error("This file can’t be opened. {0}")]
    CannotOpen(String),

    #[error("This PDF is locked with a password. Unlock it first, then convert it again.")]
    Locked,

    #[error("This PDF has no text Letters can place, so there is nothing to convert. If it is a scan, recognise its text first.")]
    NoText,

    #[error("The Word file couldn’t be saved. {0}")]
    WriteFailed(String),

    #[error("The conversion was stopped.")]
    Cancelled,

    #[error("Microsoft Word took longer than {0} seconds and was stopped. Try again, or convert without checking.")]
    WordTimedOut(u64),

    #[error("Microsoft Word couldn’t check the converted file. {0}")]
    WordFailed(String),
}

pub(crate) fn write_failed(error: impl std::fmt::Display) -> ExactError {
    ExactError::WriteFailed(format!("({error})"))
}
