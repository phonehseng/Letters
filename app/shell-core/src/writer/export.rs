//! "Export as PDF" and printing for word processor documents.
//!
//! a pdf with word's real page breaks comes from word itself through the
//! office engine's bridge (`letters_office::bridge::print_pdf`): it runs a
//! private, hidden word, finds it by a unique window caption, stops only
//! that one and has a watchdog for hangs. without word the bridge uses
//! libreoffice (close, not exact, and it says so). when neither is there
//! the ui prints its own editing surface instead.

use std::path::Path;

use letters_office::bridge::{self, BridgeError, Engine};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportMethod {
    Word,
    LibreOffice,
}

impl ExportMethod {
    pub fn label(self) -> &'static str {
        match self {
            ExportMethod::Word => "Microsoft Word",
            ExportMethod::LibreOffice => "LibreOffice",
        }
    }
}

impl From<Engine> for ExportMethod {
    fn from(engine: Engine) -> Self {
        match engine {
            Engine::Word => ExportMethod::Word,
            Engine::LibreOffice => ExportMethod::LibreOffice,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    /// neither word nor libreoffice is installed
    NoConverter,
    /// a converter ran but did not make the pdf; the text is for the person
    Failed(String),
}

/// a pdf made for printing or export, and how exact it is.
#[derive(Debug, PartialEq, Eq)]
pub struct MadePdf {
    pub method: ExportMethod,
    /// made by word, so it matches word page for page
    pub exact: bool,
    /// a plain sentence when it is not exact
    pub note: Option<String>,
}

/// makes `pdf` from the bytes of a docx with the best converter installed.
pub fn export_pdf(docx: &[u8], pdf: &Path) -> Result<MadePdf, ExportError> {
    let made = bridge::print_pdf(docx).map_err(|error| match error {
        BridgeError::WordNotInstalled | BridgeError::NoConverter => ExportError::NoConverter,
        BridgeError::Failed(detail) => {
            ExportError::Failed(format!("The PDF couldn’t be made. ({detail})"))
        }
    })?;
    if !made.pdf.starts_with(b"%PDF") {
        return Err(ExportError::Failed(
            "The converter finished but made no PDF.".into(),
        ));
    }
    crate::writer::save::write_atomically(&made.pdf, pdf).map_err(ExportError::Failed)?;
    Ok(MadePdf {
        method: made.engine.into(),
        exact: made.exact,
        note: made.note,
    })
}

/// word registers its automation name when it is installed.
pub fn word_installed() -> bool {
    bridge::word_available()
}

/// libreoffice's program, where the bridge finds it.
pub fn find_libreoffice() -> Option<std::path::PathBuf> {
    bridge::libreoffice_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_have_names_people_know() {
        assert_eq!(ExportMethod::Word.label(), "Microsoft Word");
        assert_eq!(ExportMethod::LibreOffice.label(), "LibreOffice");
        assert_eq!(
            ExportMethod::from(Engine::LibreOffice),
            ExportMethod::LibreOffice
        );
    }

    /// runs the real word through the bridge: `cargo test -p letters-shell-core word_exports -- --ignored`
    #[test]
    #[ignore]
    fn word_exports_a_document_to_pdf() {
        let dir = std::env::temp_dir().join(format!("letters-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut document = letters_office::model::Document::new();
        document.blocks = vec![letters_office::model::Block::text("Exported by Word.")];
        let pdf = dir.join("out.pdf");
        let made = export_pdf(&letters_office::write_docx(&document).unwrap(), &pdf).unwrap();
        assert_eq!(made.method, ExportMethod::Word);
        assert!(made.exact);
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    }
}
