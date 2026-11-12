//! the errors of reading, writing and editing spreadsheets, each a
//! sentence written for the person using the app.

use thiserror::Error;

/// everything that can go wrong reading or writing a spreadsheet. the
/// messages are shown to the person as they are: a plain sentence saying
/// what happened and, where it helps, what to do.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SheetError {
    #[error("This file isn’t an Excel or OpenDocument spreadsheet, or it is damaged. ({0})")]
    NotASpreadsheet(String),

    #[error("This file is damaged: {0}")]
    Damaged(String),

    #[error("This file is damaged: part of it ({part}) can’t be read. {detail}")]
    BadXml { part: String, detail: String },

    #[error("This is an older Excel file (.xls) or a binary workbook (.xlsb). Save it as .xlsx in Excel or LibreOffice, then open that.")]
    LegacyFormat,

    #[error("This workbook is locked with a password.")]
    Encrypted,

    #[error("This file is too large to open safely. ({0})")]
    TooLarge(String),

    #[error("The workbook couldn’t be saved. ({0})")]
    WriteFailed(String),

    #[error("{0}")]
    Invalid(String),
}

impl SheetError {
    pub(crate) fn bad_xml(part: &str, detail: impl Into<String>) -> SheetError {
        SheetError::BadXml {
            part: part.to_string(),
            detail: detail.into(),
        }
    }
}
