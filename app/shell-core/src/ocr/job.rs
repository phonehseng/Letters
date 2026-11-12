//! one recognition run over some pages of a document.
//!
//! the run reads the pages one at a time through a `PageReader` (the real
//! one draws the page and has tesseract read it; the tests use a fake),
//! reports after every page, and checks the cancel flag before each page
//! and while tesseract works. nothing is written into the document here:
//! the finished pages are handed back, and the caller adds them all in one
//! edit. so a run that is cancelled, or fails part way, leaves the
//! document exactly as it was.

use letters_ocr::{CancelFlag, OcrError, OcrPage};
use serde::Serialize;

/// reads the text of one page. `page` counts from 0.
pub trait PageReader {
    fn read(&mut self, page: u32, cancel: &CancelFlag) -> Result<OcrPage, OcrError>;
}

/// how far a run has got, as the tab shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    /// pages finished so far
    pub done: usize,
    pub total: usize,
    /// the page being read now, counted from 0
    pub current: Option<u32>,
}

/// how a run ended.
#[derive(Clone, Debug, PartialEq)]
pub enum JobOutcome {
    /// every page was read, in the order asked
    Finished(Vec<(u32, OcrPage)>),
    /// the person stopped it; `done` pages had been read (and are dropped)
    Cancelled { done: usize },
    /// a page could not be read; nothing is kept
    Failed { page: u32, message: String },
}

/// reads `pages` in order. `progress` hears before each page starts and
/// after each one finishes.
pub fn run_job(
    pages: &[u32],
    reader: &mut dyn PageReader,
    cancel: &CancelFlag,
    progress: &mut dyn FnMut(JobProgress),
) -> JobOutcome {
    let total = pages.len();
    let mut read = Vec::with_capacity(total);
    for (done, page) in pages.iter().copied().enumerate() {
        if cancel.is_cancelled() {
            return JobOutcome::Cancelled { done };
        }
        progress(JobProgress {
            done,
            total,
            current: Some(page),
        });
        match reader.read(page, cancel) {
            Ok(words) => read.push((page, words)),
            Err(OcrError::Cancelled) => return JobOutcome::Cancelled { done },
            // the cancel can land while tesseract is stopped half way,
            // which may show up as a failed read rather than "cancelled"
            Err(_) if cancel.is_cancelled() => return JobOutcome::Cancelled { done },
            Err(error) => {
                return JobOutcome::Failed {
                    page,
                    message: error.to_string(),
                }
            }
        }
    }
    progress(JobProgress {
        done: total,
        total,
        current: None,
    });
    JobOutcome::Finished(read)
}
