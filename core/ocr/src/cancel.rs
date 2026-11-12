//! stopping a long recognition, and telling the person how far it got.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

/// a switch the ui can flip from another thread to stop recognition.
///
/// clones share the same switch, so the ui keeps one and hands a clone to
/// the thread doing the work. the work checks it between pages, and while
/// waiting for tesseract, which is stopped straight away when it flips.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// how far a run over several pages has got, sent after each page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// zero based number of the page that was just finished
    pub page: usize,
    /// how many pages are finished, this one included
    pub done: usize,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_one_switch() {
        let flag = CancelFlag::new();
        let given_to_worker = flag.clone();
        assert!(!given_to_worker.is_cancelled());
        flag.cancel();
        assert!(given_to_worker.is_cancelled());
    }
}
