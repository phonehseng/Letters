use thiserror::Error;

/// everything that can go wrong while redacting, scrubbing or locking a file.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. when one of these is returned the document has not
/// been changed at all, because every operation checks everything first
/// and only then edits.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RedactError {
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("That password isn’t right. Check it and try again.")]
    WrongPassword,

    #[error("This PDF uses a kind of lock Letters can’t open. ({0})")]
    UnsupportedLock(String),

    #[error("Page {0} doesn’t exist in this document.")]
    UnknownPage(u32),

    #[error("A redaction box on page {0} has no size. Draw it again.")]
    EmptyArea(u32),

    /// the page's drawing instructions could not be read, so it is not
    /// possible to know what lies under the box. refusing is the only safe
    /// answer: covering it would only hide the content.
    #[error(
        "Page {0} can’t be redacted because part of it can’t be read. Nothing was changed. ({1})"
    )]
    UnreadablePage(u32, String),

    #[error("This file can’t be changed because it is damaged. ({0})")]
    Damaged(String),

    #[error("The search pattern isn’t valid. ({0})")]
    InvalidPattern(String),

    #[error("This file can’t be locked. ({0})")]
    CannotLock(String),

    #[error("This file can’t be saved. ({0})")]
    CannotSave(String),
}

impl RedactError {
    /// a page number the way people count them, from 1.
    pub(crate) fn page_number(page_index: u32) -> u32 {
        page_index + 1
    }
}
