//! the recognition runs going on in the app, so the tab can ask how one is
//! doing, cancel it, and collect its pages when it is done.
//!
//! a run belongs to one document, and a document has at most one run at a
//! time. the run itself happens on its own thread (the shell starts it and
//! calls `run`); everything here is guarded by one lock that is only held
//! for a moment, never while a page is read.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use letters_ocr::{CancelFlag, OcrPage};
use serde::Serialize;

use super::job::{run_job, JobOutcome, JobProgress, PageReader};
use super::summary::{summarize, OcrSummary};

/// where a run is, as the ui's job state machine reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Running,
    /// cancel was asked; the page being read is stopping
    Cancelling,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub id: u64,
    pub document: u64,
    pub state: JobState,
    pub progress: JobProgress,
    /// what went wrong, for a failed run
    pub error: Option<String>,
    /// what was found, for a finished run
    pub summary: Option<OcrSummary>,
}

struct Entry {
    cancel: CancelFlag,
    status: JobStatus,
    pages: Vec<(u32, OcrPage)>,
}

#[derive(Default)]
pub struct OcrJobs {
    next_id: AtomicU64,
    entries: Mutex<HashMap<u64, Entry>>,
}

impl OcrJobs {
    pub fn new() -> Self {
        Self::default()
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<u64, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// registers a run of `total` pages for a document. refused while the
    /// document has a run that hasn't ended.
    pub fn start(&self, document: u64, total: usize) -> Result<(u64, CancelFlag), String> {
        let mut entries = self.entries();
        let busy = entries.values().any(|entry| {
            entry.status.document == document
                && matches!(entry.status.state, JobState::Running | JobState::Cancelling)
        });
        if busy {
            return Err("Text is already being recognized in this document. Wait for it to finish, or cancel it first.".to_owned());
        }
        // a finished run of this document that was never collected is dropped
        entries.retain(|_, entry| entry.status.document != document);
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let cancel = CancelFlag::new();
        entries.insert(
            id,
            Entry {
                cancel: cancel.clone(),
                status: JobStatus {
                    id,
                    document,
                    state: JobState::Running,
                    progress: JobProgress {
                        done: 0,
                        total,
                        current: None,
                    },
                    error: None,
                    summary: None,
                },
                pages: Vec::new(),
            },
        );
        Ok((id, cancel))
    }

    /// reads the pages and records how it went. meant for the run's own
    /// thread: it returns when the run has ended.
    pub fn run(&self, id: u64, pages: &[u32], reader: &mut dyn PageReader, cancel: &CancelFlag) {
        let outcome = run_job(pages, reader, cancel, &mut |progress| {
            if let Some(entry) = self.entries().get_mut(&id) {
                entry.status.progress = progress;
            }
        });
        self.finish(id, outcome);
    }

    pub fn finish(&self, id: u64, outcome: JobOutcome) {
        let mut entries = self.entries();
        let Some(entry) = entries.get_mut(&id) else {
            return;
        };
        match outcome {
            JobOutcome::Finished(pages) => {
                entry.status.state = JobState::Finished;
                entry.status.summary = Some(summarize(&pages));
                entry.pages = pages;
            }
            JobOutcome::Cancelled { done } => {
                entry.status.state = JobState::Cancelled;
                entry.status.progress.done = done;
                entry.status.progress.current = None;
            }
            JobOutcome::Failed { page, message } => {
                entry.status.state = JobState::Failed;
                entry.status.progress.current = Some(page);
                entry.status.error = Some(format!(
                    "Page {} couldn’t be read, so no text was added. {message}",
                    page + 1
                ));
            }
        }
    }

    /// asks a run to stop. true when there was a run to stop.
    pub fn cancel(&self, id: u64) -> bool {
        let mut entries = self.entries();
        let Some(entry) = entries.get_mut(&id) else {
            return false;
        };
        if entry.status.state != JobState::Running {
            return false;
        }
        entry.cancel.cancel();
        entry.status.state = JobState::Cancelling;
        true
    }

    pub fn status(&self, id: u64) -> Option<JobStatus> {
        self.entries().get(&id).map(|entry| entry.status.clone())
    }

    /// the pages of a finished run, once: the run is forgotten afterwards.
    pub fn take_pages(&self, id: u64) -> Option<(u64, Vec<(u32, OcrPage)>)> {
        let mut entries = self.entries();
        if entries.get(&id)?.status.state != JobState::Finished {
            return None;
        }
        let entry = entries.remove(&id)?;
        Some((entry.status.document, entry.pages))
    }

    /// forgets a run that ended (cancelled, failed, or finished and not wanted).
    pub fn forget(&self, id: u64) {
        let mut entries = self.entries();
        if let Some(entry) = entries.get(&id) {
            if !matches!(entry.status.state, JobState::Running | JobState::Cancelling) {
                entries.remove(&id);
            }
        }
    }

    /// a document closed: its run is stopped and forgotten.
    pub fn forget_document(&self, document: u64) {
        let mut entries = self.entries();
        for entry in entries.values() {
            if entry.status.document == document {
                entry.cancel.cancel();
            }
        }
        entries.retain(|_, entry| entry.status.document != document);
    }
}
