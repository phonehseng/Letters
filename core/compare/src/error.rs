use thiserror::Error;

/// everything that can go wrong while comparing or writing a report.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. page numbers in messages count from 1.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CompareError {
    #[error("This PDF is locked with a password. Unlock it first, then compare again.")]
    Locked,

    #[error("This file can’t be read, so it can’t be compared. ({0})")]
    CannotRead(String),

    /// holds the 1-based page number, ready to show
    #[error("Page {0} doesn’t exist in this document.")]
    NoSuchPage(u32),

    /// holds the 1-based page number, ready to show
    #[error("Page {page} can’t be drawn. ({reason})")]
    RenderFailed { page: u32, reason: String },

    #[error("Pages can’t be drawn here, so pictures can’t be compared.")]
    NoRenderer,

    #[error("The comparison was stopped.")]
    Cancelled,

    #[error("The comparison report can’t be written. ({0})")]
    Report(String),
}
