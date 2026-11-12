//! office documents to pdf, through a program installed on this computer.
//!
//! Letters has no layout engine of its own for word processing, spread-
//! sheet or slide files, so it asks a program that has one:
//! - on windows, microsoft word, excel or powerpoint, driven through com
//!   automation by a small powershell script shipped inside Letters
//!   (`office-to-pdf.ps1`, which explains the rules it keeps)
//! - on any system, libreoffice, run headless with its own private
//!   profile so it never touches a copy the person has open
//!
//! each run has a time limit; a program that goes past it (waiting on a
//! dialog nobody can see, say) is stopped, and only the copy of the
//! program this run started.
//!
//! macos: Pages, Keynote and Numbers (and Word, Excel and PowerPoint for
//! mac) can be scripted with AppleScript through `osascript`, e.g.
//! `tell application "Pages" to export (open file) to file "out.pdf" as
//! PDF`. that isn't built yet, because it can't be run or tested here;
//! on a mac today only libreoffice is used.

mod libreoffice;
mod microsoft;
mod process;

use std::path::{Path, PathBuf};
use std::time::Duration;

use letters_ocr::CancelFlag;
use serde::{Deserialize, Serialize};

use crate::ConvertError;

/// the office file types Letters can ask another program to lay out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OfficeFormat {
    Docx,
    Doc,
    Odt,
    Rtf,
    Xlsx,
    Xls,
    Ods,
    Pptx,
    Ppt,
    Odp,
}

/// what kind of program lays a format out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OfficeKind {
    Document,
    Spreadsheet,
    Presentation,
}

impl OfficeFormat {
    pub const ALL: [OfficeFormat; 10] = [
        OfficeFormat::Docx,
        OfficeFormat::Doc,
        OfficeFormat::Odt,
        OfficeFormat::Rtf,
        OfficeFormat::Xlsx,
        OfficeFormat::Xls,
        OfficeFormat::Ods,
        OfficeFormat::Pptx,
        OfficeFormat::Ppt,
        OfficeFormat::Odp,
    ];

    /// the format of a file, from its extension.
    pub fn of_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|format| format.extension() == extension)
    }

    pub fn extension(&self) -> &'static str {
        match self {
            OfficeFormat::Docx => "docx",
            OfficeFormat::Doc => "doc",
            OfficeFormat::Odt => "odt",
            OfficeFormat::Rtf => "rtf",
            OfficeFormat::Xlsx => "xlsx",
            OfficeFormat::Xls => "xls",
            OfficeFormat::Ods => "ods",
            OfficeFormat::Pptx => "pptx",
            OfficeFormat::Ppt => "ppt",
            OfficeFormat::Odp => "odp",
        }
    }

    pub fn kind(&self) -> OfficeKind {
        match self {
            OfficeFormat::Docx | OfficeFormat::Doc | OfficeFormat::Odt | OfficeFormat::Rtf => {
                OfficeKind::Document
            }
            OfficeFormat::Xlsx | OfficeFormat::Xls | OfficeFormat::Ods => OfficeKind::Spreadsheet,
            OfficeFormat::Pptx | OfficeFormat::Ppt | OfficeFormat::Odp => OfficeKind::Presentation,
        }
    }

    /// how a person would name the files: "Word", "OpenDocument text".
    pub fn description(&self) -> &'static str {
        match self {
            OfficeFormat::Docx | OfficeFormat::Doc => "Word",
            OfficeFormat::Odt => "OpenDocument text",
            OfficeFormat::Rtf => "RTF",
            OfficeFormat::Xlsx | OfficeFormat::Xls => "Excel",
            OfficeFormat::Ods => "OpenDocument spreadsheet",
            OfficeFormat::Pptx | OfficeFormat::Ppt => "PowerPoint",
            OfficeFormat::Odp => "OpenDocument presentation",
        }
    }
}

/// a program that can turn office files into pdfs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum OfficeConverter {
    MicrosoftWord,
    MicrosoftExcel,
    MicrosoftPowerPoint,
    LibreOffice { program: PathBuf },
}

impl OfficeConverter {
    /// the program's name, for messages
    pub fn name(&self) -> &'static str {
        match self {
            OfficeConverter::MicrosoftWord => "Microsoft Word",
            OfficeConverter::MicrosoftExcel => "Microsoft Excel",
            OfficeConverter::MicrosoftPowerPoint => "Microsoft PowerPoint",
            OfficeConverter::LibreOffice { .. } => "LibreOffice",
        }
    }

    /// whether it can lay out files of this kind
    pub fn handles(&self, kind: OfficeKind) -> bool {
        match self {
            OfficeConverter::MicrosoftWord => kind == OfficeKind::Document,
            OfficeConverter::MicrosoftExcel => kind == OfficeKind::Spreadsheet,
            OfficeConverter::MicrosoftPowerPoint => kind == OfficeKind::Presentation,
            OfficeConverter::LibreOffice { .. } => true,
        }
    }
}

/// what was found on this computer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeConverters {
    /// usable converters, best first
    pub available: Vec<OfficeConverter>,
    /// office programs found that Letters can't drive yet (on macos)
    pub found_but_unused: Vec<String>,
}

impl OfficeConverters {
    /// the converters for a kind of file, best first.
    pub fn for_kind(&self, kind: OfficeKind) -> Vec<&OfficeConverter> {
        self.available.iter().filter(|c| c.handles(kind)).collect()
    }
}

/// looks for microsoft office (windows) and libreoffice (anywhere). this
/// starts no office program; it only looks at what is installed.
pub fn find_office_converters() -> OfficeConverters {
    let mut found = OfficeConverters::default();
    found.available.extend(microsoft::installed());
    if let Some(program) = libreoffice::find_program() {
        found
            .available
            .push(OfficeConverter::LibreOffice { program });
    }
    found.found_but_unused = mac_apps();
    found
}

/// office programs on a mac that could be scripted but aren't yet.
fn mac_apps() -> Vec<String> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    [
        "Pages",
        "Numbers",
        "Keynote",
        "Microsoft Word",
        "Microsoft Excel",
        "Microsoft PowerPoint",
    ]
    .iter()
    .filter(|name| Path::new(&format!("/Applications/{name}.app")).exists())
    .map(|name| name.to_string())
    .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfficeOptions {
    /// the longest one file may take, in seconds
    pub timeout_seconds: u64,
    /// use this kind of converter first when there is a choice
    pub prefer_libreoffice: bool,
}

impl Default for OfficeOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: 180,
            prefer_libreoffice: false,
        }
    }
}

/// what made the pdf.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeConversion {
    pub program: String,
    /// the copy of the office program this conversion started (and quit
    /// again), when it started one; for checking nothing was left running
    pub helper_process: Option<u32>,
}

/// turns an office document into a pdf at `output` (replacing it: the
/// save panel already asked). the first suitable converter is tried, and
/// if it fails on the file, the next.
pub fn office_to_pdf(
    input: &Path,
    output: &Path,
    converters: &OfficeConverters,
    options: &OfficeOptions,
    cancel: &CancelFlag,
) -> Result<OfficeConversion, ConvertError> {
    let format = OfficeFormat::of_path(input).ok_or_else(|| {
        let extension = input
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_else(|| "these".into());
        ConvertError::UnsupportedFormat(extension)
    })?;
    if !input.is_file() {
        return Err(ConvertError::CannotOpen(
            "The file isn’t there any more.".into(),
        ));
    }
    let mut candidates = converters.for_kind(format.kind());
    if options.prefer_libreoffice {
        candidates.sort_by_key(|c| !matches!(c, OfficeConverter::LibreOffice { .. }));
    }
    if candidates.is_empty() {
        return Err(no_converter(format, converters));
    }
    let limit = Duration::from_secs(options.timeout_seconds.max(5));
    let work = WorkFolder::new()?;
    let mut last_error = None;
    for converter in candidates {
        let made = match converter {
            OfficeConverter::LibreOffice { program } => {
                libreoffice::convert(program, input, &work.path, limit, cancel)
            }
            microsoft => microsoft::convert(microsoft, input, &work.path, limit, cancel),
        };
        match made {
            Ok((pdf, helper_process)) => {
                check_pdf(&pdf, converter.name())?;
                move_into_place(&pdf, output)?;
                return Ok(OfficeConversion {
                    program: converter.name().to_string(),
                    helper_process,
                });
            }
            // a file one program can't read, another sometimes can
            Err(error @ ConvertError::ConverterFailed { .. }) => last_error = Some(error),
            Err(other) => return Err(other),
        }
    }
    Err(last_error.expect("at least one converter was tried"))
}

/// the friendly "nothing can open this" message, naming the options.
fn no_converter(format: OfficeFormat, converters: &OfficeConverters) -> ConvertError {
    let options = if cfg!(windows) {
        let program = match format.kind() {
            OfficeKind::Document => "Microsoft Word",
            OfficeKind::Spreadsheet => "Microsoft Excel",
            OfficeKind::Presentation => "Microsoft PowerPoint",
        };
        format!("Install {program} or the free LibreOffice, then try again.")
    } else if cfg!(target_os = "macos") {
        let found = if converters.found_but_unused.is_empty() {
            String::new()
        } else {
            format!(
                " Letters can’t use {} yet, but you can open the file there and choose File > Export To > PDF.",
                converters.found_but_unused.join(", ")
            )
        };
        format!("Install the free LibreOffice, then try again.{found}")
    } else {
        "Install the free LibreOffice, then try again.".to_string()
    };
    ConvertError::NoConverter {
        format: format.description().to_string(),
        options,
    }
}

/// a converter that says it finished must have left a real pdf.
fn check_pdf(path: &Path, program: &str) -> Result<(), ConvertError> {
    let starts_right = std::fs::read(path)
        .map(|bytes| bytes.starts_with(b"%PDF"))
        .unwrap_or(false);
    if starts_right {
        Ok(())
    } else {
        Err(ConvertError::ConverterFailed {
            program: program.to_string(),
            detail: "It didn’t make a PDF.".into(),
        })
    }
}

/// moves the finished pdf to where it was asked for, copying when the
/// two places are on different drives.
fn move_into_place(made: &Path, output: &Path) -> Result<(), ConvertError> {
    if std::fs::rename(made, output).is_ok() {
        return Ok(());
    }
    std::fs::copy(made, output).map_err(crate::error::write_failed)?;
    let _ = std::fs::remove_file(made);
    Ok(())
}

/// a fresh folder in the temporary folder for one conversion, deleted
/// with everything in it afterwards.
struct WorkFolder {
    path: PathBuf,
}

impl WorkFolder {
    fn new() -> Result<Self, ConvertError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir()
            .join("letters-convert")
            .join(format!("office-{}-{number}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).map_err(crate::error::write_failed)?;
        Ok(Self { path })
    }
}

impl Drop for WorkFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_come_from_extensions() {
        assert_eq!(
            OfficeFormat::of_path(Path::new("a/Essay.DOCX")),
            Some(OfficeFormat::Docx)
        );
        assert_eq!(
            OfficeFormat::of_path(Path::new("b.odp")).map(|f| f.kind()),
            Some(OfficeKind::Presentation)
        );
        assert_eq!(OfficeFormat::of_path(Path::new("c.txt")), None);
    }

    #[test]
    fn missing_converters_name_the_options() {
        let none = OfficeConverters::default();
        let dir = std::env::temp_dir().join(format!("letters-office-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("Essay.docx");
        std::fs::write(&input, b"not really a document").unwrap();
        let error = office_to_pdf(
            &input,
            &dir.join("Essay.pdf"),
            &none,
            &OfficeOptions::default(),
            &CancelFlag::new(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.starts_with("Letters can’t turn Word files into PDFs on this computer"),
            "{message}"
        );
        assert!(message.contains("LibreOffice"), "{message}");
        if cfg!(windows) {
            assert!(message.contains("Microsoft Word"), "{message}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_files_are_refused() {
        let error = office_to_pdf(
            Path::new("notes.pages"),
            Path::new("x.pdf"),
            &OfficeConverters::default(),
            &OfficeOptions::default(),
            &CancelFlag::new(),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Letters can’t convert “.pages” files.");
    }

    #[test]
    fn each_program_handles_its_own_kind() {
        let converters = OfficeConverters {
            available: vec![
                OfficeConverter::MicrosoftWord,
                OfficeConverter::LibreOffice {
                    program: "soffice".into(),
                },
            ],
            found_but_unused: Vec::new(),
        };
        assert_eq!(converters.for_kind(OfficeKind::Document).len(), 2);
        assert_eq!(converters.for_kind(OfficeKind::Spreadsheet).len(), 1);
    }
}
