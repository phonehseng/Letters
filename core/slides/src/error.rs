//! the errors this crate reports, each a sentence for the person using
//! the app.

use thiserror::Error;

/// everything that can go wrong reading, writing or rendering a deck.
///
/// like the other crates' errors these are shown to the person as they
/// are: a plain sentence that says what happened.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SlidesError {
    #[error("This file isn’t a PowerPoint or OpenDocument presentation, or it is damaged. ({0})")]
    NotAPresentation(String),

    #[error("This presentation is damaged: {0}")]
    Damaged(String),

    #[error("This presentation is damaged: part of it ({part}) can’t be read. {detail}")]
    BadXml { part: String, detail: String },

    #[error("This presentation is locked with a password.")]
    Encrypted,

    #[error("This is an older PowerPoint file (.ppt). Save it as .pptx in PowerPoint or LibreOffice, then open that.")]
    LegacyFormat,

    #[error("This file is too large to open safely. ({0})")]
    TooLarge(String),

    #[error("The presentation couldn’t be saved. ({0})")]
    WriteFailed(String),

    #[error("That change can’t be made: {0}")]
    InvalidEdit(String),

    #[error("The slide couldn’t be drawn. ({0})")]
    RenderFailed(String),
}

impl SlidesError {
    pub(crate) fn bad_xml(part: &str, detail: impl Into<String>) -> SlidesError {
        SlidesError::BadXml {
            part: part.to_string(),
            detail: detail.into(),
        }
    }
}
