//! reads and writes word processor documents (docx and odt) through one
//! shared model.
//!
//! - `model` is the document every format is read into
//! - `docx` and `odt` read and write the two file formats
//! - anything a reader does not understand is kept (see `model::Carry`
//!   and the `Preserved` fields) and written back in place
//! - the other modules are pure functions on the model that the editor
//!   uses: styles, lists, outline, typing helpers, presets, counts,
//!   plain text and html export and import

pub mod base64;
pub mod bridge;
pub mod clipboard;
pub mod compat;
pub mod counts;
pub mod docx;
mod error;
pub mod export;
pub mod floating;
pub mod formats;
pub mod import;
pub mod indent;
pub mod markdown;
pub mod model;
pub mod numbering;
pub mod odt;
pub mod outline;
pub mod package;
pub mod presets;
pub mod rtf;
pub mod ruler;
pub mod styles;
pub mod tables;
pub mod typing;
pub mod units;
pub mod xml;

pub use docx::{read_docx, write_docx};
pub use error::OfficeError;
pub use odt::{read_odt, write_odt};
pub use package::{detect, FileKind};

/// opens a word processor document of either format, telling them apart
/// by content. spreadsheets and presentations are refused with a clear
/// "not supported yet".
pub fn open(bytes: &[u8]) -> Result<model::Document, OfficeError> {
    match detect(bytes)? {
        FileKind::Docx => read_docx(bytes),
        FileKind::Odt => read_odt(bytes),
    }
}

/// saves a document in the chosen format.
pub fn save(document: &model::Document, kind: FileKind) -> Result<Vec<u8>, OfficeError> {
    match kind {
        FileKind::Docx => write_docx(document),
        FileKind::Odt => write_odt(document),
    }
}
