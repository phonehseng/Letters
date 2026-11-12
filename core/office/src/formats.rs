//! every format a document can be saved in, from the one model, and what
//! each keeps, for the save as dialog. also page setup facts for printing.

use serde::Serialize;

use crate::bridge::{self, BridgeError};
use crate::error::OfficeError;
use crate::model::{Document, Orientation, Section, Twips};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveFormat {
    Docx,
    Odt,
    Rtf,
    Html,
    PlainText,
    Markdown,
    /// the old word format, through word or libreoffice
    Doc,
}

/// what one format keeps and loses, in plain words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    pub format: SaveFormat,
    pub label: &'static str,
    pub extension: &'static str,
    pub can_open: bool,
    pub can_save: bool,
    /// needs word or libreoffice on this computer
    pub needs_converter: bool,
    pub summary: &'static str,
}

/// the table the save as dialog shows.
pub fn format_capabilities() -> Vec<FormatCapability> {
    let entry = |format, label, extension, can_open, needs_converter, summary| FormatCapability {
        format,
        label,
        extension,
        can_open,
        can_save: true,
        needs_converter,
        summary,
    };
    vec![
        entry(SaveFormat::Docx, "Word Document", "docx", true, false,
            "Keeps everything, including what Letters doesn’t edit itself; the best choice to hand in or share."),
        entry(SaveFormat::Odt, "OpenDocument Text", "odt", true, false,
            "Keeps text, styles, lists, tables, notes, comments, changes, pictures and text boxes; Word-only extras are kept as text."),
        entry(SaveFormat::Rtf, "Rich Text Format", "rtf", true, false,
            "Keeps text, formatting, tables and pictures; list numbers become plain text, and comments and tracked changes are left out."),
        entry(SaveFormat::Html, "Web Page", "html", false, false,
            "Keeps headings, lists, tables, links and pictures in one file; page layout, headers and footers are left out."),
        entry(SaveFormat::Markdown, "Markdown", "md", false, false,
            "Keeps headings, emphasis, lists, links, tables and notes as plain text; pictures are saved beside it, other formatting is left out."),
        entry(SaveFormat::PlainText, "Plain Text", "txt", true, false,
            "Keeps only the words, with list numbers and notes written out; all formatting is left out."),
        entry(SaveFormat::Doc, "Word 97–2003 Document", "doc", true, true,
            "For very old versions of Word; needs Word or LibreOffice on this computer, and newer features may be simplified."),
    ]
}

/// a saved file: the main bytes, plus pictures for formats that keep them
/// beside the file (markdown).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub bytes: Vec<u8>,
    pub extra_files: Vec<(String, Vec<u8>)>,
}

/// saves in any format.
pub fn save_as(document: &Document, format: SaveFormat) -> Result<Saved, OfficeError> {
    let plain = |bytes: Vec<u8>| Saved {
        bytes,
        extra_files: Vec::new(),
    };
    Ok(match format {
        SaveFormat::Docx => plain(crate::write_docx(document)?),
        SaveFormat::Odt => plain(crate::write_odt(document)?),
        SaveFormat::Rtf => plain(crate::rtf::write_rtf(document)),
        SaveFormat::Html => plain(crate::export::to_html(document).into_bytes()),
        SaveFormat::PlainText => plain(crate::export::to_plain_text(document).into_bytes()),
        SaveFormat::Markdown => {
            let export = crate::markdown::to_markdown(document);
            Saved {
                bytes: export.text.into_bytes(),
                extra_files: export.files,
            }
        }
        SaveFormat::Doc => {
            let docx = crate::write_docx(document)?;
            let (bytes, _) = bridge::convert_word_format(&docx, true).map_err(bridge_error)?;
            plain(bytes)
        }
    })
}

fn bridge_error(error: BridgeError) -> OfficeError {
    match error {
        BridgeError::WordNotInstalled | BridgeError::NoConverter => OfficeError::NotSupportedYet(
            "old Word files without Word or LibreOffice on this computer".into(),
        ),
        BridgeError::Failed(detail) => OfficeError::WriteFailed(detail),
    }
}

/// opens docx, odt, rtf and plain text by content, and old .doc files when
/// word or libreoffice can convert them.
pub fn open_any(bytes: &[u8]) -> Result<Document, OfficeError> {
    if bytes.starts_with(b"{\\rtf") {
        return crate::rtf::read_rtf(bytes);
    }
    match crate::open(bytes) {
        Err(OfficeError::LegacyWordFormat) => match bridge::convert_word_format(bytes, false) {
            Ok((docx, _)) => crate::read_docx(&docx),
            Err(_) => Err(OfficeError::LegacyWordFormat),
        },
        Err(OfficeError::NotAPackage(reason)) => match std::str::from_utf8(bytes) {
            Ok(text) if !bytes.contains(&0) => Ok(crate::import::from_plain_text(text)),
            _ => Err(OfficeError::NotAPackage(reason)),
        },
        other => other,
    }
}

/// a section's page setup in the terms a print dialog uses.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintPageSetup {
    pub section: usize,
    /// "Letter", "A4"... or "Custom"
    pub paper: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
    pub landscape: bool,
    pub margins_inches: [f64; 4],
}

const PAPERS: &[(&str, Twips, Twips)] = &[
    ("Letter", 12240, 15840),
    ("Legal", 12240, 20160),
    ("Tabloid", 15840, 24480),
    ("Executive", 10440, 15120),
    ("A3", 16838, 23811),
    ("A4", 11906, 16838),
    ("A5", 8391, 11906),
    ("B5", 9979, 14175),
];

/// the paper name for a page size (either way round), within a millimetre.
pub fn paper_name(width: Twips, height: Twips) -> &'static str {
    let (short, long) = (width.min(height), width.max(height));
    PAPERS
        .iter()
        .find(|(_, w, h)| (short - w).abs() <= 57 && (long - h).abs() <= 57)
        .map(|(name, _, _)| *name)
        .unwrap_or("Custom")
}

/// page setup of every section, for printing.
pub fn page_setups(document: &Document) -> Vec<PrintPageSetup> {
    let mm = |twips: Twips| (f64::from(twips) * 25.4 / 1440.0 * 10.0).round() / 10.0;
    let inches = |twips: Twips| (f64::from(twips) / 1440.0 * 100.0).round() / 100.0;
    let default = [Section::default()];
    let sections = if document.sections.is_empty() {
        &default[..]
    } else {
        &document.sections[..]
    };
    sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let page = &section.page;
            let m = &page.margins;
            PrintPageSetup {
                section: index,
                paper: paper_name(page.width, page.height),
                width_mm: mm(page.width),
                height_mm: mm(page.height),
                landscape: page.orientation == Orientation::Landscape || page.width > page.height,
                margins_inches: [
                    inches(m.top),
                    inches(m.right),
                    inches(m.bottom),
                    inches(m.left),
                ],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_names_and_setups() {
        assert_eq!(paper_name(12240, 15840), "Letter");
        assert_eq!(paper_name(16838, 11906), "A4");
        assert_eq!(paper_name(1000, 1000), "Custom");
        let setups = page_setups(&Document::new());
        assert_eq!(setups[0].paper, "Letter");
        assert_eq!(setups[0].margins_inches, [1.0; 4]);
        assert_eq!(setups[0].width_mm, 215.9);
    }

    #[test]
    fn every_format_saves_except_the_converted_one() {
        let document = crate::import::from_html("<h1>Title</h1><p>Text</p>");
        for capability in format_capabilities() {
            if capability.needs_converter {
                continue;
            }
            let saved = save_as(&document, capability.format).unwrap();
            assert!(!saved.bytes.is_empty(), "{:?}", capability.format);
        }
        let rtf = save_as(&document, SaveFormat::Rtf).unwrap().bytes;
        assert_eq!(
            open_any(&rtf).unwrap().blocks[0]
                .paragraph()
                .unwrap()
                .plain_text(),
            "Title"
        );
        assert_eq!(
            open_any(b"just text").unwrap().blocks[0]
                .paragraph()
                .unwrap()
                .plain_text(),
            "just text"
        );
    }
}
