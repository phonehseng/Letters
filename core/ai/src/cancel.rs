//! stopping work part way, and reporting how far it has got.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::AiError;

/// a shared "stop" switch.
///
/// the ui keeps one clone and the running task another. pressing Stop
/// flips the switch, and the task notices at its next checkpoint: between
/// requests, between streamed pieces of an answer, and between batches of
/// embeddings.
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    stopped: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// returns an error once cancelled, so loops can use `?`.
    pub fn check(&self) -> Result<(), AiError> {
        if self.is_cancelled() {
            Err(AiError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// what a running task tells the ui while it works.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    /// a whole step finished, for a progress bar: "reading part 2 of 5".
    Step {
        done: usize,
        total: usize,
        stage: Stage,
    },
    /// the next few words of the final answer, so it can appear as it is
    /// written instead of all at once.
    Text(String),
}

/// which part of a task a progress step belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// finding the passages that matter (embedding the document's text).
    Searching,
    /// reading one part of a long document on its own.
    Reading,
    /// combining the notes from several parts into one.
    Combining,
    /// writing the final answer.
    Writing,
}
