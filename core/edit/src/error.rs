//! the errors an edit can end with, each a sentence for the person using
//! the app.

use thiserror::Error;

/// everything that can go wrong when editing page content.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. page numbers count from 1, the way people count.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum EditError {
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("Page {} doesn’t exist in this document.", .0 + 1)]
    UnknownPage(usize),

    /// ids are only valid until the next edit of the page
    #[error("That text or picture has changed since it was selected. Select it again.")]
    UnknownObject,

    #[error("That position is outside the selected text.")]
    OutOfRange,

    #[error("These characters can’t be written with any font Letters can use here: “{0}”. Remove them and try again.")]
    UnsupportedCharacters(String),

    #[error("The font “{0}” doesn’t allow being copied into a PDF, so Letters can’t use it here. Choose another font.")]
    EmbeddingNotAllowed(String),

    #[error("This font file can’t be used. {0}")]
    UnsupportedFont(String),

    #[error("This picture can’t be used. {0}")]
    UnsupportedImage(String),

    #[error("This picture can’t be saved on its own. {0}")]
    CannotExtract(String),

    /// a value from the ui that makes no sense; the text says what to change
    #[error("{0}")]
    InvalidValue(String),

    #[error("This page can’t be changed because part of the file is damaged. ({0})")]
    Damaged(String),

    /// the file carries digital signatures that any content change breaks;
    /// the ui asks first and then repeats the edit with consent
    #[error("This PDF is digitally signed{0}. Changing its content will make the signature invalid. Confirm to edit it anyway.")]
    SignaturesWouldBreak(String),
}
