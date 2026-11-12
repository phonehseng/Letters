//! many conversions in one application start: a private word works
//! through a list of pdf exports and saves (`word_batch`), a headless
//! libreoffice converts many files at once (`libreoffice_convert_files`).
//! starting word costs seconds per call, which a corpus of documents
//! times several renderings would otherwise pay again and again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::{failed, run_libreoffice_files, run_word, BridgeError, Workspace};

/// word's save format numbers (`WdSaveFormat`).
pub const WORD_FORMAT_DOCX: u32 = 16;
pub const WORD_FORMAT_ODT: u32 = 23;

/// what a batch job does with its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordJobKind {
    /// export as pdf, as word prints it
    Pdf,
    /// save in another format (`WORD_FORMAT_DOCX`, `WORD_FORMAT_ODT`...)
    Convert { format: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub kind: WordJobKind,
}

/// one job as the bridge script reads it.
#[derive(Serialize)]
struct JobLine {
    op: &'static str,
    #[serde(rename = "in")]
    input: String,
    out: String,
    format: u32,
}

/// runs every job in one private word. the outer error is word itself
/// failing; each job's own result is `Ok` or word's error message.
pub fn word_batch(jobs: &[WordJob]) -> Result<Vec<Result<(), String>>, BridgeError> {
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    let workspace = Workspace::new().map_err(failed)?;
    let lines: Vec<JobLine> = jobs
        .iter()
        .map(|job| JobLine {
            op: match job.kind {
                WordJobKind::Pdf => "pdf",
                WordJobKind::Convert { .. } => "convert",
            },
            input: job.input.to_string_lossy().into_owned(),
            out: job.output.to_string_lossy().into_owned(),
            format: match job.kind {
                WordJobKind::Convert { format } => format,
                WordJobKind::Pdf => 0,
            },
        })
        .collect();
    let list = workspace.path.join("jobs.json");
    let results = workspace.path.join("results.json");
    let json = serde_json::to_string(&lines).map_err(failed)?;
    std::fs::write(&list, json).map_err(failed)?;
    run_word(
        &workspace,
        &[
            "-Mode",
            "batch",
            "-Path",
            &list.to_string_lossy(),
            "-Out",
            &results.to_string_lossy(),
        ],
    )?;
    let text = std::fs::read_to_string(&results).map_err(failed)?;
    let outcomes: Vec<String> =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(failed)?;
    if outcomes.len() != jobs.len() {
        return Err(BridgeError::Failed(format!(
            "Word answered for {} of {} documents",
            outcomes.len(),
            jobs.len()
        )));
    }
    Ok(outcomes
        .into_iter()
        .map(|outcome| {
            if outcome == "ok" {
                Ok(())
            } else {
                Err(outcome)
            }
        })
        .collect())
}

/// converts every file in `inputs` with one headless libreoffice into
/// `format` (its --convert-to name: "pdf", "odt", "docx:MS Word 2007
/// XML"...), writing the results into `out_dir` under their own names.
/// libreoffice gets two minutes plus twenty seconds per file.
pub fn libreoffice_convert_files(
    inputs: &[PathBuf],
    format: &str,
    out_dir: &Path,
) -> Result<(), BridgeError> {
    if inputs.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(out_dir).map_err(failed)?;
    let limit = Duration::from_secs(120 + 20 * inputs.len() as u64);
    run_libreoffice_files(inputs, format, out_dir, limit)
}
