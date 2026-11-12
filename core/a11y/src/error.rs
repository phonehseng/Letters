use thiserror::Error;

/// everything that can stop a check or a fix.
///
/// the ui shows these messages as they are, so they are plain sentences
/// that say what to do next. page numbers in messages count from 1, the
/// way people count, even though the api counts from 0.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum A11yError {
    #[error("This PDF is locked with a password. Unlock it first, then check it again.")]
    Locked,

    #[error("This PDF’s list of pages is damaged, so it can’t be checked. ({0})")]
    Damaged(String),

    /// holds the 1-based page number, ready to show
    #[error("Page {0} doesn’t exist in this document.")]
    NoSuchPage(u32),

    #[error("That part of the document’s tags couldn’t be found. Check the document again and pick it from the new list.")]
    NoSuchElement,

    #[error("That picture couldn’t be found on the page. Check the document again and pick it from the new list.")]
    NoSuchImage,

    #[error("That link, note or form field couldn’t be found. Check the document again and pick it from the new list.")]
    NoSuchAnnotation,

    #[error("This document has no tags yet. Use “Add tags” first.")]
    NotTagged,

    #[error("This document already has tags. Adding tags again would replace them, so choose “Replace existing tags” if that is what you want.")]
    AlreadyTagged,

    #[error("“{0}” isn’t a language code a screen reader understands. Use a code like en, en-US, fr or zh-Hans.")]
    InvalidLanguage(String),

    /// a setting outside what makes sense; the text is the whole sentence
    #[error("{0}")]
    InvalidInput(String),

    #[error(
        "This document still has problems that must be fixed, so it can’t be marked as PDF/UA yet."
    )]
    NotReadyForPdfUa,

    #[error("The drawing instructions on page {0} couldn’t be read, so they weren’t changed.")]
    UnreadableContent(u32),

    #[error("Something went wrong while changing the document. Nothing was changed. ({0})")]
    Internal(String),
}
