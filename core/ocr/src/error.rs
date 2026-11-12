use thiserror::Error;

/// everything that can go wrong while reading text from a page image.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum OcrError {
    /// the tesseract program or its language folder is not where it should be
    #[error("Letters can’t read text from scans because part of the app is missing. Install Letters again to fix this. ({0})")]
    EngineMissing(String),

    /// a language was chosen whose data file is not installed
    #[error("Text in {name} can’t be read because the {name} language data is missing. Install Letters again, or choose a different language. (missing {code}.traineddata)")]
    MissingLanguage { code: String, name: String },

    /// a language code that is not even shaped like one
    #[error("“{0}” isn’t a language Letters can read. Choose a language from the list.")]
    UnknownLanguage(String),

    #[error("Choose at least one language to read the text in.")]
    NoLanguage,

    #[error("Reading the text stopped unexpectedly. Try again. ({0})")]
    EngineFailed(String),

    #[error("Reading the text was cancelled.")]
    Cancelled,

    #[error("This picture can’t be read. {0}")]
    UnreadableImage(String),

    #[error("This picture is too large to read. Use a picture under {0} megapixels.")]
    ImageTooLarge(u32),

    #[error("There are no pictures to make a PDF from.")]
    NoImages,

    /// zero based page number; the message counts from one like a person does
    #[error("Page {} doesn’t exist in this document.", .0 + 1)]
    UnknownPage(u32),

    #[error("This PDF can’t be changed because part of it is damaged. ({0})")]
    Damaged(String),
}
