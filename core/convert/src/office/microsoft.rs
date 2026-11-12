//! microsoft word, excel and powerpoint on windows, driven through com
//! automation by `office-to-pdf.ps1` (shipped inside Letters and written
//! out next to each conversion).
//!
//! a script in windows powershell, which every windows has, was chosen
//! over a com client in rust: the gnu toolchain Letters builds with can
//! compile the windows crate, but late-bound com calls (IDispatch with
//! named arguments) need a lot of unsafe code a student would have to
//! read, while the script says each step in office's own words. a hang
//! inside office also can't take Letters down with it this way.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use letters_ocr::CancelFlag;

use super::process::{hide_window, run_with_limit, RunEnd};
use super::OfficeConverter;
use crate::ConvertError;

const SCRIPT: &str = include_str!("office-to-pdf.ps1");

/// the office programs registered for automation on this computer.
pub(super) fn installed() -> Vec<OfficeConverter> {
    if !cfg!(windows) {
        return Vec::new();
    }
    [
        ("Word.Application", OfficeConverter::MicrosoftWord),
        ("Excel.Application", OfficeConverter::MicrosoftExcel),
        (
            "PowerPoint.Application",
            OfficeConverter::MicrosoftPowerPoint,
        ),
    ]
    .into_iter()
    .filter(|(prog_id, _)| registered(prog_id))
    .map(|(_, converter)| converter)
    .collect()
}

/// whether windows knows the automation name (it has a class id).
fn registered(prog_id: &str) -> bool {
    let mut query = Command::new("reg");
    query.args(["query", &format!("HKCR\\{prog_id}\\CLSID")]);
    hide_window(&mut query);
    query
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// converts one file; on success returns the pdf and the process id of
/// the office copy the script started (and quit), when it started one.
pub(super) fn convert(
    converter: &OfficeConverter,
    input: &Path,
    work: &Path,
    limit: Duration,
    cancel: &CancelFlag,
) -> Result<(PathBuf, Option<u32>), ConvertError> {
    let program = converter.name();
    let app = match converter {
        OfficeConverter::MicrosoftWord => "Word",
        OfficeConverter::MicrosoftExcel => "Excel",
        OfficeConverter::MicrosoftPowerPoint => "PowerPoint",
        OfficeConverter::LibreOffice { .. } => unreachable!("libreoffice has its own module"),
    };
    // office wants whole paths: it resolves others against its own folder
    let input =
        std::path::absolute(input).map_err(|e| ConvertError::CannotOpen(format!("({e})")))?;
    let script = work.join("office-to-pdf.ps1");
    // windows powershell reads a script without a byte order mark as the
    // local code page, so the mark keeps any non-english text intact
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(SCRIPT.as_bytes());
    std::fs::write(&script, bytes).map_err(crate::error::write_failed)?;
    let output = work.join("converted.pdf");

    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(["-App", app, "-InputPath"])
        .arg(&input)
        .arg("-OutputPath")
        .arg(&output);
    let our_process =
        |line: &str| -> Option<u32> { line.strip_prefix("ours=").and_then(|pid| pid.parse().ok()) };
    match run_with_limit(command, limit, cancel, our_process) {
        RunEnd::Finished { status, lines, errors } => {
            if status.success() && lines.iter().any(|l| l == "done") && output.is_file() {
                let helper = lines.iter().find_map(|l| our_process(l));
                return Ok((output, helper));
            }
            let said = lines
                .iter()
                .find_map(|l| l.strip_prefix("error="))
                .map(str::to_string)
                .unwrap_or_else(|| first_line(&errors));
            let detail = if status.code() == Some(3) {
                format!("It couldn’t be started. ({said})")
            } else {
                format!("The file may be damaged or locked with a password. ({said})")
            };
            Err(ConvertError::ConverterFailed {
                program: program.to_string(),
                detail,
            })
        }
        RunEnd::TimedOut => Err(ConvertError::TimedOut {
            program: program.to_string(),
            seconds: limit.as_secs(),
        }),
        RunEnd::Cancelled => Err(ConvertError::Cancelled),
        RunEnd::NotStarted(reason) => Err(ConvertError::ConverterFailed {
            program: program.to_string(),
            detail: format!("Windows PowerShell, which Letters uses to talk to it, couldn’t be started. ({reason})"),
        }),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no reason given")
        .to_string()
}
