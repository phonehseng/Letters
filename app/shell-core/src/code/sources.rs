//! reads the document an export or a code search works on: a word
//! document from its file, or a pdf rebuilt as one the way "Open PDF as
//! Document…" does it (core/convert's easy to edit structure).

use std::path::Path;

use letters_convert::{CancelFlag, PageSelection, PdfSource};
use letters_office::model::Document;
use serde::Deserialize;

use crate::export::run::easy_to_edit_document;

/// which kind of document a command reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Writer,
    Pdf,
}

/// the document as saved on disk, in the office model.
pub fn office_document(path: &Path, kind: SourceKind) -> Result<Document, String> {
    match kind {
        SourceKind::Writer => {
            let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
            letters_office::open(&bytes).map_err(|error| error.to_string())
        }
        SourceKind::Pdf => {
            let source = PdfSource::open(path).map_err(|error| error.to_string())?;
            let (document, _) = easy_to_edit_document(
                &source,
                &PageSelection::All,
                &mut |_| {},
                &CancelFlag::new(),
            )?;
            Ok(document)
        }
    }
}
