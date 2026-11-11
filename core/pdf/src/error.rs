use thiserror::Error;

/// everything that can go wrong when opening or rendering a pdf.
///
/// messages are written for the person using the app, because the ui
/// shows them as they are.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PdfError {
    #[error("the pdf engine could not be loaded: {0}")]
    EngineUnavailable(String),

    #[error("this file could not be opened: {0}")]
    CannotOpen(String),

    #[error("this pdf is locked with a password")]
    PasswordRequired,

    #[error("this document is no longer open")]
    UnknownDocument,

    #[error("page {0} does not exist in this document")]
    UnknownPage(u32),

    #[error("that part of the page does not exist")]
    UnknownTile,

    #[error("the page could not be drawn: {0}")]
    RenderFailed(String),

    #[error("the pdf engine stopped unexpectedly")]
    WorkerStopped,
}
