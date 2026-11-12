//! drives microsoft word to export docx files as pdf, through
//! `word-export.ps1` (shipped inside Letters and written out next to each
//! run).
//!
//! the safety rules (the person may have their own word windows open):
//! - a private, invisible copy of word that the script starts, alerts off,
//!   documents opened read only, a time limit for every call
//! - that copy's process id is known from its marked window (and checked
//!   against the processes that were running before), and after every
//!   call, even a failed or stopped one, exactly that process is ended if
//!   it is still there. no other word process is ever touched
//! - word's application options are never changed
//!
//! a script in windows powershell was chosen over a com client in rust for
//! the same reason as in `core/convert`: late-bound com calls need a lot of
//! unsafe code, while the script says each step in word's own words.

pub(crate) mod process;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use process::{hide_window, make_sure_word_exits, run_with_limit, RunEnd};

use crate::{CancelFlag, ExactError};

const SCRIPT: &str = include_str!("word-export.ps1");

/// word as installed on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordInstall {
    pub program: PathBuf,
    /// the file version of WINWORD.EXE, e.g. 16.0.20527.20150
    pub build: String,
}

/// looks for word without starting it: the registered program and its
/// file version.
pub fn find_word() -> Option<WordInstall> {
    if !cfg!(windows) {
        return None;
    }
    let script = concat!(
        "$p = (Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\Winword.exe' -ErrorAction SilentlyContinue).'(default)';",
        "if (-not $p -or -not (Test-Path -LiteralPath $p)) { exit 1 };",
        "if (-not (Test-Path 'Registry::HKEY_CLASSES_ROOT\\Word.Application\\CLSID')) { exit 1 };",
        "$v = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($p);",
        "Write-Output $p;",
        "Write-Output ('{0}.{1}.{2}.{3}' -f $v.FileMajorPart, $v.FileMinorPart, $v.FileBuildPart, $v.FilePrivatePart)"
    );
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    hide_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let program = PathBuf::from(lines.next()?);
    let build = lines.next()?.to_string();
    Some(WordInstall { program, build })
}

/// what a person does to the open document before it is exported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    /// types `text` right after `needle`
    Insert,
    /// deletes `needle`
    Delete,
    /// types `text` after `needle`, then undoes it
    InsertAndUndo,
}

/// one in-memory edit: the first match of `needle` in the main text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub action: EditAction,
    pub needle: String,
    pub text: String,
}

impl Edit {
    /// the extra fields of a job line: tab, action, needle and text in base64
    /// (so tabs, quotes and non-english text reach the script intact).
    fn job_fields(&self) -> String {
        let action = match self.action {
            EditAction::Insert => "insert",
            EditAction::Delete => "delete",
            EditAction::InsertAndUndo => "insert-undo",
        };
        format!(
            "\t{action}\t{}\t{}",
            base64(self.needle.as_bytes()),
            base64(self.text.as_bytes())
        )
    }
}

/// a docx, its pdf and an optional edit.
pub type EditJob = (PathBuf, PathBuf, Option<Edit>);

/// standard base64 with padding (rfc 4648).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// what one call to word did.
#[derive(Clone, Debug, Default)]
pub struct ExportRun {
    /// the process of the word copy this call started (and ended)
    pub word_process: Option<u32>,
    pub build: Option<String>,
    /// per job: `Ok` or why the pdf wasn't written
    pub results: Vec<Result<(), String>>,
    pub seconds: f64,
    /// false if our word process could not be ended (it is named in the report)
    pub word_ended: bool,
}

/// the settings of the word bridge.
#[derive(Clone, Debug)]
pub struct WordBridge {
    /// the longest one document may take, in seconds
    pub seconds_per_document: u64,
    /// where the script and job files go
    pub work_dir: PathBuf,
}

/// seconds every call may take on top of its documents: starting word.
pub const START_SECONDS: u64 = 30;
/// when a call is stopped before the script named its word copy, how long
/// to wait for that name, so the copy can be ended instead of left running.
const NAME_GRACE: Duration = Duration::from_secs(30);

impl WordBridge {
    /// exports each docx to its pdf with one private copy of word.
    pub fn export(
        &self,
        jobs: &[(PathBuf, PathBuf)],
        cancel: &CancelFlag,
    ) -> Result<ExportRun, ExactError> {
        let limit =
            Duration::from_secs(START_SECONDS + self.seconds_per_document * jobs.len() as u64);
        self.export_within(jobs, cancel, limit)
    }

    /// `export` with an explicit time limit for the whole call.
    pub fn export_within(
        &self,
        jobs: &[(PathBuf, PathBuf)],
        cancel: &CancelFlag,
        limit: Duration,
    ) -> Result<ExportRun, ExactError> {
        let jobs: Vec<EditJob> = jobs
            .iter()
            .map(|(docx, pdf)| (docx.clone(), pdf.clone(), None))
            .collect();
        self.run_jobs(&jobs, cancel, limit)
    }

    /// exports each docx after word made one edit to it in memory, the way
    /// a person would (the edit tests). nothing is saved.
    pub fn export_edited(
        &self,
        jobs: &[EditJob],
        cancel: &CancelFlag,
    ) -> Result<ExportRun, ExactError> {
        let limit =
            Duration::from_secs(START_SECONDS + self.seconds_per_document * jobs.len() as u64);
        self.run_jobs(jobs, cancel, limit)
    }

    fn run_jobs(
        &self,
        jobs: &[EditJob],
        cancel: &CancelFlag,
        limit: Duration,
    ) -> Result<ExportRun, ExactError> {
        if cancel.is_cancelled() {
            return Err(ExactError::Cancelled);
        }
        std::fs::create_dir_all(&self.work_dir)
            .map_err(|e| ExactError::WordFailed(format!("({e})")))?;
        let stamp = unique_stamp();
        let script = self.work_dir.join("word-export.ps1");
        // windows powershell reads a script without a byte order mark as the
        // local code page, so the mark keeps any non-english text intact
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(SCRIPT.as_bytes());
        std::fs::write(&script, bytes).map_err(|e| ExactError::WordFailed(format!("({e})")))?;
        let jobs_file = self.work_dir.join(format!("jobs-{stamp}.txt"));
        let mut listing = String::new();
        for (docx, pdf, edit) in jobs {
            let _ = std::fs::remove_file(pdf);
            listing.push_str(&format!(
                "{}\t{}{}\n",
                absolute(docx).display(),
                absolute(pdf).display(),
                edit.as_ref().map(Edit::job_fields).unwrap_or_default()
            ));
        }
        std::fs::write(&jobs_file, listing)
            .map_err(|e| ExactError::WordFailed(format!("({e})")))?;
        let marker = format!("Letters exact {stamp}");
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
            .arg("-JobsPath")
            .arg(&jobs_file)
            .arg("-Marker")
            .arg(&marker);
        let ours = |line: &str| {
            line.strip_prefix("ours=")
                .and_then(|pid| pid.parse::<u32>().ok())
        };
        let started = std::time::Instant::now();
        let end = run_with_limit(command, limit, cancel, ours, NAME_GRACE);
        let _ = std::fs::remove_file(&jobs_file);
        let lines = match &end {
            RunEnd::Finished { lines, .. }
            | RunEnd::TimedOut { lines }
            | RunEnd::Cancelled { lines } => lines.clone(),
            RunEnd::NotStarted(_) => Vec::new(),
        };
        let word_process = lines.iter().find_map(|l| ours(l));
        // whatever happened, our word copy must not be left running
        let word_ended = word_process
            .map(|pid| make_sure_word_exits(pid, Duration::from_secs(15)))
            .unwrap_or(true);
        if !word_ended {
            return Err(ExactError::WordFailed(format!(
                "Letters started a hidden copy of Word (process {}) that couldn’t be ended. End it in Task Manager.",
                word_process.unwrap_or(0)
            )));
        }
        let mut run = ExportRun {
            word_process,
            build: lines
                .iter()
                .find_map(|l| l.strip_prefix("build=").map(str::to_string)),
            results: vec![Err("Word didn’t get to it.".into()); jobs.len()],
            seconds: started.elapsed().as_secs_f64(),
            word_ended,
        };
        match end {
            RunEnd::TimedOut { .. } => return Err(ExactError::WordTimedOut(limit.as_secs())),
            RunEnd::Cancelled { .. } => return Err(ExactError::Cancelled),
            RunEnd::NotStarted(reason) => {
                return Err(ExactError::WordFailed(format!(
                    "Windows PowerShell, which Letters uses to talk to Word, couldn’t be started. ({reason})"
                )))
            }
            RunEnd::Finished { lines, errors, status } => {
                if let Some(pid) = lines.iter().find_map(|l| l.strip_prefix("shared=")) {
                    return Err(ExactError::WordFailed(format!(
                        "Word handed over a copy that was already running (process {pid}), so it was left alone."
                    )));
                }
                if let Some(said) = lines.iter().find_map(|l| l.strip_prefix("error=")) {
                    return Err(ExactError::WordFailed(format!("({said})")));
                }
                if !lines.iter().any(|l| l == "done") {
                    let reason = errors.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no reason given").to_string();
                    return Err(ExactError::WordFailed(format!("(exit {:?}: {reason})", status.code())));
                }
                for line in &lines {
                    if let Some(index) = line.strip_prefix("exported=").and_then(|i| i.parse::<usize>().ok()) {
                        if let Some(slot) = run.results.get_mut(index) {
                            *slot = Ok(());
                        }
                    } else if let Some(rest) = line.strip_prefix("failed=") {
                        let (index, why) = rest.split_once(':').unwrap_or((rest, ""));
                        if let Some(slot) = index.parse::<usize>().ok().and_then(|i| run.results.get_mut(i)) {
                            *slot = Err(why.trim().to_string());
                        }
                    }
                }
                for (slot, (_, pdf, _)) in run.results.iter_mut().zip(jobs) {
                    if slot.is_ok() && !pdf.is_file() {
                        *slot = Err("Word said it exported the file, but there is no PDF.".into());
                    }
                }
            }
        }
        Ok(run)
    }

    /// exports one docx; the pdf's path, or why it failed.
    pub fn export_one(
        &self,
        docx: &Path,
        pdf: &Path,
        cancel: &CancelFlag,
    ) -> Result<ExportRun, ExactError> {
        let run = self.export(&[(docx.to_path_buf(), pdf.to_path_buf())], cancel)?;
        if let Some(Err(reason)) = run.results.first() {
            return Err(ExactError::WordFailed(format!(
                "Word couldn’t export the file. ({reason})"
            )));
        }
        Ok(run)
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// a stamp unique to this process and call, for the window marker and files.
fn unique_stamp() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!(
        "{}-{}-{nanos:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_follows_rfc_4648() {
        // the rfc's test vectors
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
