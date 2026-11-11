use thiserror::Error;

/// everything that can go wrong when opening or rendering a pdf.
///
/// the ui shows these messages as they are, so they are written for the
/// person using the app: a plain sentence, no blame, and where possible
/// what to do next.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PdfError {
    #[error("Letters can’t show PDFs because part of the app is missing. Install Letters again to fix this. ({0})")]
    EngineUnavailable(String),

    #[error("This file can’t be opened. {0}")]
    CannotOpen(String),

    #[error("This PDF is locked with a password.")]
    PasswordRequired,

    #[error("This document is no longer open.")]
    UnknownDocument,

    #[error("Page {0} doesn’t exist in this document.")]
    UnknownPage(u32),

    #[error("That part of the page doesn’t exist.")]
    UnknownTile,

    #[error("This page can’t be shown. Close the document and open it again. ({0})")]
    RenderFailed(String),

    /// the request waited so long that newer ones took its place.
    /// asking again is fine.
    #[error("This part of the page was skipped. It will be drawn when asked for again.")]
    Superseded,

    #[error("Letters stopped showing PDFs unexpectedly. Quit Letters and open it again.")]
    WorkerStopped,
}
