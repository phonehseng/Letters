use thiserror::Error;

/// everything that can go wrong when adding or changing annotations.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. page numbers in messages count from 1, the way
/// people count pages, even though the api counts from 0.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum AnnotateError {
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("Page {} doesn’t exist in this document.", .0 + 1)]
    UnknownPage(usize),

    #[error("That comment or mark isn’t on this page any more. It may have been deleted.")]
    UnknownAnnotation,

    #[error("There is nothing to mark here. Select some text or draw an area first.")]
    EmptyArea,

    #[error("The text box can’t show these characters with its built-in fonts: “{0}”. Remove them and try again.")]
    UnsupportedCharacters(String),

    #[error("This picture can’t be used. {0}")]
    UnsupportedImage(String),

    #[error("Letters can’t change this kind of annotation, but it can be deleted.")]
    CannotChange,

    #[error("This is a form field, not a comment. Change it with the form tools.")]
    FormField,

    /// a value from the ui that makes no sense, such as a negative line width.
    /// the text is a whole sentence saying what to change.
    #[error("{0}")]
    InvalidValue(String),

    #[error("This page can’t be changed because part of the file is damaged. ({0})")]
    Damaged(String),
}
