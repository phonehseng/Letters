use thiserror::Error;

/// everything that can go wrong while converting.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next. page numbers count from 1, the way people count.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConvertError {
    #[error("This PDF is locked with a password. Unlock it first, then try again.")]
    Locked,

    #[error("This file can’t be opened. {0}")]
    CannotOpen(String),

    #[error("This PDF has no pages, so there is nothing to convert.")]
    EmptyDocument,

    #[error("Page {} doesn’t exist in this document.", .0 + 1)]
    UnknownPage(usize),

    /// a setting from the ui that makes no sense; the text says what to change
    #[error("{0}")]
    InvalidSetting(String),

    #[error("The conversion was cancelled.")]
    Cancelled,

    #[error("A page couldn’t be drawn, so it couldn’t be converted. ({0})")]
    RenderFailed(String),

    #[error("The converted file couldn’t be saved. {0}")]
    WriteFailed(String),

    #[error("This PDF can’t be converted because part of it is damaged. ({0})")]
    Damaged(String),

    /// a file type this crate has no way to convert
    #[error("Letters can’t convert “{0}” files.")]
    UnsupportedFormat(String),

    /// office documents need a program on this computer to lay them out
    #[error("Letters can’t turn {format} files into PDFs on this computer, because it needs a program that can open them and none is installed. {options}")]
    NoConverter { format: String, options: String },

    #[error("{program} couldn’t convert this file. {detail}")]
    ConverterFailed { program: String, detail: String },

    #[error("{program} took longer than {seconds} seconds to convert this file, so Letters stopped it. Try again, or open the file in {program} and save it as a PDF there.")]
    TimedOut { program: String, seconds: u64 },

    /// passed on from making a pdf from pictures, already worded for people
    #[error("{0}")]
    Images(String),
}

impl From<letters_edit::EditError> for ConvertError {
    fn from(error: letters_edit::EditError) -> Self {
        match error {
            letters_edit::EditError::Locked => ConvertError::Locked,
            letters_edit::EditError::UnknownPage(page) => ConvertError::UnknownPage(page),
            other => ConvertError::Damaged(other.to_string()),
        }
    }
}

impl From<letters_ocr::OcrError> for ConvertError {
    fn from(error: letters_ocr::OcrError) -> Self {
        match error {
            letters_ocr::OcrError::Cancelled => ConvertError::Cancelled,
            other => ConvertError::Images(other.to_string()),
        }
    }
}

/// turns a file system error into the "couldn't be saved" message.
pub(crate) fn write_failed(error: std::io::Error) -> ConvertError {
    ConvertError::WriteFailed(format!("({error})"))
}
