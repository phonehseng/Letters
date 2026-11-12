//! "Export as PDF" for word processor documents.
//!
//! Letters has no page layout engine of its own for word documents, so a
//! pdf with the real page breaks comes from a word processor that has one:
//! microsoft word through automation when it is installed, otherwise
//! libreoffice in headless mode. when neither is there the ui prints its
//! own editing surface to a pdf instead (and says page breaks may differ).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

/// the word helper, kept beside this file and written out when needed.
const WORD_SCRIPT: &str = include_str!("word_export.ps1");

/// long enough for a big document in a cold word, short enough that a
/// hung converter does not keep the person waiting forever.
const CONVERTER_TIMEOUT: Duration = Duration::from_secs(150);

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

#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    /// neither word nor libreoffice is installed
    NoConverter,
    /// a converter ran but did not make the pdf; the text is for the person
    Failed(String),
}

/// makes `pdf` from `docx` with the best converter installed.
pub fn export_pdf(docx: &Path, pdf: &Path, work_dir: &Path) -> Result<ExportMethod, ExportError> {
    let mut problems = Vec::new();
    if word_installed() {
        match export_with_word(docx, pdf, work_dir) {
            Ok(()) => return Ok(ExportMethod::Word),
            Err(problem) => problems.push(problem),
        }
    }
    if let Some(soffice) = find_libreoffice() {
        match export_with_libreoffice(&soffice, docx, pdf, work_dir) {
            Ok(()) => return Ok(ExportMethod::LibreOffice),
            Err(problem) => problems.push(problem),
        }
    }
    if problems.is_empty() {
        Err(ExportError::NoConverter)
    } else {
        Err(ExportError::Failed(problems.join(" ")))
    }
}

/// word registers its automation name when it is installed.
pub fn word_installed() -> bool {
    if !cfg!(windows) {
        return false;
    }
    hidden(Command::new("reg").args(["query", r"HKCR\Word.Application\CLSID"]))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn export_with_word(docx: &Path, pdf: &Path, work_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
    let script = work_dir.join("word_export.ps1");
    fs::write(&script, WORD_SCRIPT).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(pdf);
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(docx)
        .arg("-Out")
        .arg(pdf);
    run_with_timeout(hidden(&mut command)).map_err(|detail| format!("Word couldn’t export it ({detail})."))?;
    made(pdf, "Word")
}

/// the usual places libreoffice installs itself.
pub fn libreoffice_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(folder) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(folder).join(r"LibreOffice\program\soffice.exe"));
        }
    }
    candidates.push(PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS/soffice"));
    candidates.push(PathBuf::from("/usr/bin/soffice"));
    candidates
}

pub fn find_libreoffice() -> Option<PathBuf> {
    libreoffice_candidates().into_iter().find(|path| path.is_file())
}

fn export_with_libreoffice(soffice: &Path, docx: &Path, pdf: &Path, work_dir: &Path) -> Result<(), String> {
    let out_dir = work_dir.join("libreoffice-out");
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).map_err(|error| error.to_string())?;
    let mut command = Command::new(soffice);
    command
        .args(["--headless", "--norestore", "--convert-to", "pdf", "--outdir"])
        .arg(&out_dir)
        .arg(docx);
    run_with_timeout(hidden(&mut command))
        .map_err(|detail| format!("LibreOffice couldn’t export it ({detail})."))?;
    let stem = docx.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let produced = out_dir.join(format!("{stem}.pdf"));
    fs::rename(&produced, pdf)
        .or_else(|_| fs::copy(&produced, pdf).map(drop))
        .map_err(|error| format!("LibreOffice’s PDF couldn’t be moved into place ({error})."))?;
    made(pdf, "LibreOffice")
}

fn made(pdf: &Path, by: &str) -> Result<(), String> {
    match fs::metadata(pdf) {
        Ok(meta) if meta.len() > 0 => Ok(()),
        _ => Err(format!("{by} finished but made no PDF.")),
    }
}

/// runs a converter, ending it if it takes too long.
pub(crate) fn run_with_timeout(command: &mut Command) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            if status.success() {
                return Ok(());
            }
            let mut detail = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let _ = stderr.read_to_string(&mut detail);
            }
            return Err(detail.trim().chars().take(300).collect());
        }
        if started.elapsed() > CONVERTER_TIMEOUT {
            let _ = child.kill();
            return Err("it took too long".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// no console window flashes up while a helper runs.
pub(crate) fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_helper_only_ever_stops_its_own_word() {
        assert!(WORD_SCRIPT.contains("*/Automation*"));
        assert!(WORD_SCRIPT.contains("DisplayAlerts = 0"));
        // read-only open of the copy, and the watchdog
        assert!(WORD_SCRIPT.contains("Documents.Open($full, $false, $true, $false)"));
        assert!(WORD_SCRIPT.contains("Start-Sleep -Seconds 120"));
    }

    #[test]
    fn libreoffice_is_looked_for_in_the_usual_places() {
        let candidates = libreoffice_candidates();
        assert!(candidates.iter().any(|path| path.ends_with("soffice.exe") || path.ends_with("soffice")));
    }

    #[test]
    fn methods_have_names_people_know() {
        assert_eq!(ExportMethod::Word.label(), "Microsoft Word");
        assert_eq!(ExportMethod::LibreOffice.label(), "LibreOffice");
    }

    /// runs the real word: `cargo test -p letters-shell-core -- --ignored`
    #[test]
    #[ignore]
    fn word_exports_a_document_to_pdf() {
        let dir = std::env::temp_dir().join(format!("letters-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let docx = dir.join("in.docx");
        let mut document = letters_office::model::Document::new();
        document.blocks = vec![letters_office::model::Block::text("Exported by Word.")];
        fs::write(&docx, letters_office::write_docx(&document).unwrap()).unwrap();
        let pdf = dir.join("out.pdf");
        assert_eq!(export_pdf(&docx, &pdf, &dir.join("work")), Ok(ExportMethod::Word));
        assert!(fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    }
}
