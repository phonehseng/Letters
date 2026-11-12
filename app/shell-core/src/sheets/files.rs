//! reading a spreadsheet file into the engine's model and writing it back.
//!
//! the format is read from the bytes (core/sheets `package::detect`), not
//! the name, so a renamed file still opens; the name only decides the
//! format a save writes. every write goes through the writer's safe save:
//! a new file beside the old one, renamed over it in one step.

use std::path::{Path, PathBuf};

use letters_sheets::csv::{read_csv, write_csv, CsvOptions, CsvWriteOptions};
use letters_sheets::package::{detect, FileKind};
use letters_sheets::{ods, xlsx, Workbook};
use serde::Serialize;

use crate::session::file_name;
use crate::writer::save::write_atomically;

/// the formats a workbook can be saved in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetFormat {
    Xlsx,
    /// an excel workbook with macros, which are kept and never run
    Xlsm,
    Ods,
    Csv,
}

impl SheetFormat {
    pub fn extension(self) -> &'static str {
        match self {
            SheetFormat::Xlsx => "xlsx",
            SheetFormat::Xlsm => "xlsm",
            SheetFormat::Ods => "ods",
            SheetFormat::Csv => "csv",
        }
    }
}

/// the lower-case extension of a path, without the dot.
fn extension(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// whether the sheets screen opens this file (by its name).
pub fn is_sheet_path(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "xlsx" | "xlsm" | "ods" | "csv" | "tsv"
    )
}

/// the format a name asks for, or `None` for other names.
pub fn format_for_path(path: &Path) -> Option<SheetFormat> {
    match extension(path).as_str() {
        "xlsx" => Some(SheetFormat::Xlsx),
        "xlsm" => Some(SheetFormat::Xlsm),
        "ods" => Some(SheetFormat::Ods),
        "csv" | "tsv" => Some(SheetFormat::Csv),
        _ => None,
    }
}

/// "Budget" saved from an xlsx becomes "Budget.xlsx"; "Budget.ods" stays.
pub fn with_sheet_extension(path: &Path, current: SheetFormat) -> PathBuf {
    if format_for_path(path).is_some() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(current.extension());
    PathBuf::from(name)
}

/// reads a spreadsheet file. the error is a sentence for the person.
pub fn read_workbook(path: &Path) -> Result<(Workbook, SheetFormat), String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("“{}” can’t be read. ({error})", file_name(path)))?;
    read_workbook_bytes(&bytes, path)
}

/// reads bytes whose file name is `path` (the name only tells xlsx from xlsm
/// and tab-separated text from commas).
pub fn read_workbook_bytes(bytes: &[u8], path: &Path) -> Result<(Workbook, SheetFormat), String> {
    let kind = detect(bytes).map_err(|error| error.to_string())?;
    let book = match kind {
        FileKind::Xlsx => xlsx::read_xlsx(bytes),
        FileKind::Ods => ods::read_ods(bytes),
        FileKind::Csv => read_csv(bytes, &csv_options(path)),
    }
    .map_err(|error| error.to_string())?;
    let format = match kind {
        FileKind::Xlsx if book.has_macros => SheetFormat::Xlsm,
        FileKind::Xlsx => SheetFormat::Xlsx,
        FileKind::Ods => SheetFormat::Ods,
        FileKind::Csv => SheetFormat::Csv,
    };
    Ok((book, format))
}

/// a .tsv is tab separated whatever its first line looks like; anything
/// else lets the engine find the delimiter.
fn csv_options(path: &Path) -> CsvOptions {
    let delimiter = (extension(path) == "tsv").then_some('\t');
    CsvOptions {
        delimiter,
        ..CsvOptions::default()
    }
}

/// the bytes of a workbook in `format`. csv holds one sheet: `sheet`.
pub fn workbook_bytes(
    book: &Workbook,
    format: SheetFormat,
    sheet: usize,
    path: &Path,
) -> Result<Vec<u8>, String> {
    match format {
        SheetFormat::Xlsx | SheetFormat::Xlsm => {
            xlsx::write_xlsx(book).map_err(|error| error.to_string())
        }
        SheetFormat::Ods => ods::write_ods(book).map_err(|error| error.to_string()),
        SheetFormat::Csv => {
            let delimiter = if extension(path) == "tsv" { '\t' } else { ',' };
            let options = CsvWriteOptions {
                delimiter,
                ..CsvWriteOptions::default()
            };
            Ok(write_csv(book, sheet, &options))
        }
    }
}

/// refuses names that would break the file before anything is written.
pub fn check_save_target(book: &Workbook, path: &Path, format: SheetFormat) -> Result<(), String> {
    let name = file_name(path);
    if book.has_macros && format == SheetFormat::Xlsx {
        // the package keeps its macro content type, which excel refuses
        // to open under an .xlsx name
        return Err(format!(
            "“{name}” would hold macros under an .xlsx name, which Excel refuses to open. Save it as .xlsm to keep the macros, or as .ods without them."
        ));
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => Err(format!("“{name}” is a folder. Choose another name.")),
        Ok(meta) if meta.permissions().readonly() => Err(format!(
            "“{name}” is read-only, so it can’t be replaced. Use Save As to save a copy."
        )),
        _ => Ok(()),
    }
}

/// writes the workbook to `path` safely.
pub fn save_workbook(
    book: &Workbook,
    path: &Path,
    format: SheetFormat,
    sheet: usize,
) -> Result<(), String> {
    check_save_target(book, path, format)?;
    let bytes = workbook_bytes(book, format, sheet, path)?;
    write_atomically(&bytes, path)
}

/// what saving in `format` leaves out, in a sentence, or `None`.
pub fn save_losses(book: &Workbook, format: SheetFormat) -> Option<String> {
    match format {
        SheetFormat::Csv => {
            let others = book.sheets.len().saturating_sub(1);
            let sheets = match others {
                0 => String::new(),
                1 => " The other sheet isn’t saved.".into(),
                n => format!(" The other {n} sheets aren’t saved."),
            };
            Some(format!(
                "CSV keeps only the values of one sheet: formulas become their results, and formatting, charts and notes are left out.{sheets}"
            ))
        }
        SheetFormat::Ods => {
            let notes = letters_sheets::help::compat::compatibility(
                book,
                letters_sheets::help::compat::Target::Ods,
            );
            (!notes.is_empty()).then(|| {
                let texts: Vec<String> = notes.into_iter().map(|note| note.text).collect();
                texts.join(" ")
            })
        }
        SheetFormat::Xlsx | SheetFormat::Xlsm => None,
    }
}
