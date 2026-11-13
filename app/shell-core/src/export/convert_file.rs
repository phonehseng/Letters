//! Convert File…: any file Letters reads, saved as another type without
//! opening it in a window. documents go between word and opendocument,
//! spreadsheets between excel, opendocument and csv, presentations between
//! powerpoint and opendocument, all with Letters' own readers and writers;
//! each of them also becomes a pdf through microsoft office or libreoffice
//! when one is installed. pdfs have their own Export To… with page and
//! picture options, and pictures their own New from Images…, so the ui
//! sends those there (`ConvertSource`).

use std::path::Path;

use letters_convert::{find_office_converters, CancelFlag, OfficeConverters, OfficeFormat};
use serde::{Deserialize, Serialize};

use super::office::convert_to_pdf;
use crate::sheets::files::{read_workbook, save_losses, save_workbook, SheetFormat};
use crate::slides::deck::{DeckFormat, SlideDeck};
use crate::writer::document::WriterDocument;
use crate::writer::save::{odt_losses, save_document};
use crate::writer::FileKind;

/// the types a file can be converted to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetFormat {
    Docx,
    Odt,
    Xlsx,
    Ods,
    Csv,
    Pptx,
    Odp,
    Pdf,
}

impl TargetFormat {
    pub fn extension(self) -> &'static str {
        match self {
            TargetFormat::Docx => "docx",
            TargetFormat::Odt => "odt",
            TargetFormat::Xlsx => "xlsx",
            TargetFormat::Ods => "ods",
            TargetFormat::Csv => "csv",
            TargetFormat::Pptx => "pptx",
            TargetFormat::Odp => "odp",
            TargetFormat::Pdf => "pdf",
        }
    }

    /// the name people know the type by
    fn label(self) -> &'static str {
        match self {
            TargetFormat::Docx => "Word document (.docx)",
            TargetFormat::Odt => "OpenDocument text (.odt)",
            TargetFormat::Xlsx => "Excel workbook (.xlsx)",
            TargetFormat::Ods => "OpenDocument spreadsheet (.ods)",
            TargetFormat::Csv => "CSV, values only (.csv)",
            TargetFormat::Pptx => "PowerPoint presentation (.pptx)",
            TargetFormat::Odp => "OpenDocument presentation (.odp)",
            TargetFormat::Pdf => "PDF (.pdf)",
        }
    }
}

/// what kind of file was chosen, which decides where the ui goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConvertSource {
    Document,
    Spreadsheet,
    Presentation,
    /// Export To… converts pdfs, with its page and picture options
    Pdf,
    /// New from Images… makes pdfs of pictures, with text recognition
    Picture,
    /// nothing Letters can convert
    Other,
}

/// one type the file can become, for the list in the sheet.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertTarget {
    pub format: TargetFormat,
    pub label: String,
    pub extension: String,
    pub available: bool,
    /// how it is made, or why it can't be made here
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertChoices {
    pub source: ConvertSource,
    pub targets: Vec<ConvertTarget>,
}

/// the file made, and what the new type couldn't keep.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertedFile {
    pub path: String,
    pub notes: Vec<String>,
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// what a file is, by its name.
pub fn source_of(path: &Path) -> ConvertSource {
    match extension(path).as_str() {
        "docx" | "odt" | "rtf" | "doc" => ConvertSource::Document,
        "xlsx" | "xlsm" | "ods" | "csv" | "tsv" => ConvertSource::Spreadsheet,
        "pptx" | "odp" => ConvertSource::Presentation,
        "pdf" => ConvertSource::Pdf,
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "gif" | "webp" | "heic" => {
            ConvertSource::Picture
        }
        _ => ConvertSource::Other,
    }
}

/// the types `path` can become on this computer.
pub fn choices(path: &Path) -> ConvertChoices {
    choices_with(path, &find_office_converters())
}

/// `choices` with the office programs already looked up (tests pass none).
pub fn choices_with(path: &Path, converters: &OfficeConverters) -> ConvertChoices {
    let source = source_of(path);
    let own = match source {
        ConvertSource::Document => vec![TargetFormat::Docx, TargetFormat::Odt],
        ConvertSource::Spreadsheet => {
            vec![TargetFormat::Xlsx, TargetFormat::Ods, TargetFormat::Csv]
        }
        ConvertSource::Presentation => vec![TargetFormat::Pptx, TargetFormat::Odp],
        _ => Vec::new(),
    };
    let from = extension(path);
    let mut targets: Vec<ConvertTarget> = own
        .into_iter()
        // the file's own type isn't a conversion (xlsm and tsv are: they
        // become xlsx and csv)
        .filter(|format| format.extension() != from)
        .map(|format| ConvertTarget {
            format,
            label: format.label().into(),
            extension: format.extension().into(),
            available: true,
            note: own_note(format),
        })
        .collect();
    if let Some(office) = OfficeFormat::of_path(path) {
        let program = converters.for_kind(office.kind()).first().map(|c| c.name());
        targets.push(ConvertTarget {
            format: TargetFormat::Pdf,
            label: TargetFormat::Pdf.label().into(),
            extension: "pdf".into(),
            available: program.is_some(),
            note: match program {
                Some(name) => format!("Laid out by {name}, so the pages look as they do there."),
                // letters has no page layout engine for office files of its own
                None => "Needs Microsoft Office or the free LibreOffice, and neither is installed. You can also open the file and print it to PDF.".into(),
            },
        });
    }
    ConvertChoices { source, targets }
}

fn own_note(format: TargetFormat) -> String {
    match format {
        TargetFormat::Csv => "Keeps the values of the first sheet only.".into(),
        _ => "Made by Letters itself; nothing else needs to be installed.".into(),
    }
}

/// converts `path` to `format`, writing `destination`. the original file
/// is only read. the message of an error is written for the person.
pub fn convert(
    path: &Path,
    format: TargetFormat,
    destination: &Path,
) -> Result<ConvertedFile, String> {
    // a name typed without the type's extension still gets the chosen type
    let destination = &with_extension(destination, format);
    // a conversion onto the file itself would replace it with a copy of itself
    if same_file(path, destination) {
        return Err(
            "Choose a different name: the new file would replace the one being converted.".into(),
        );
    }
    let notes = match (source_of(path), format) {
        (_, TargetFormat::Pdf) => {
            let made = convert_to_pdf(path, destination, &CancelFlag::new())?;
            vec![made.summary]
        }
        (ConvertSource::Document, TargetFormat::Docx | TargetFormat::Odt) => {
            let kind = if format == TargetFormat::Docx {
                FileKind::Docx
            } else {
                FileKind::Odt
            };
            let (writer, document) =
                WriterDocument::open(path).map_err(|error| error.to_string())?;
            save_document(&document, destination, kind)?;
            match kind {
                FileKind::Odt => odt_losses(&document, writer.source_kind)
                    .into_iter()
                    .collect(),
                FileKind::Docx => Vec::new(),
            }
        }
        (
            ConvertSource::Spreadsheet,
            TargetFormat::Xlsx | TargetFormat::Ods | TargetFormat::Csv,
        ) => {
            let sheet_format = match format {
                TargetFormat::Xlsx => SheetFormat::Xlsx,
                TargetFormat::Ods => SheetFormat::Ods,
                _ => SheetFormat::Csv,
            };
            let (book, _) = read_workbook(path)?;
            // csv holds one sheet: the first, as a new file has no other choice
            save_workbook(&book, destination, sheet_format, 0)?;
            save_losses(&book, sheet_format).into_iter().collect()
        }
        (ConvertSource::Presentation, TargetFormat::Pptx | TargetFormat::Odp) => {
            let deck_format = if format == TargetFormat::Pptx {
                DeckFormat::Pptx
            } else {
                DeckFormat::Odp
            };
            let mut deck = SlideDeck::open(path)?;
            // save picks the format by the name's extension, set above
            debug_assert_eq!(DeckFormat::for_path(destination), Some(deck_format));
            deck.save(Some(destination))?
        }
        _ => {
            return Err(format!(
                "Letters can’t make a .{} file from this one.",
                format.extension()
            ))
        }
    };
    Ok(ConvertedFile {
        path: destination.to_string_lossy().into_owned(),
        notes,
    })
}

/// "Essay" becomes "Essay.odt"; "Essay.odt" stays; "Essay.docx" asked
/// for as odt becomes "Essay.docx.odt", so the name never claims a type
/// the file isn't.
fn with_extension(path: &Path, format: TargetFormat) -> std::path::PathBuf {
    if extension(path) == format.extension() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(format.extension());
    name.into()
}

/// whether two paths name one file (by its canonical place when it exists).
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
#[path = "convert_file_tests.rs"]
mod tests;
