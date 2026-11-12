//! the one error type every command returns to the ui.

use std::fmt::Display;

use letters_pdf::PdfError;
use letters_shell_core::session::{ApplyError, SessionError};
use serde::Serialize;

/// an error the ui can both show and react to.
///
/// `kind` lets the ui tell "ask for a password" apart from "show a message".
/// `message` is a sentence written for the person, safe to show as it is.
#[derive(Debug, Serialize)]
pub struct CommandError {
    kind: &'static str,
    message: String,
}

impl CommandError {
    /// an error the ui shows as a message, and nothing more.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            kind: "failed",
            message: message.into(),
        }
    }

    pub fn engine_unavailable(reason: String) -> Self {
        Self {
            kind: "engine-unavailable",
            message: reason,
        }
    }

    pub fn not_open() -> Self {
        Self::from(PdfError::UnknownDocument)
    }
}

impl From<PdfError> for CommandError {
    fn from(error: PdfError) -> Self {
        let kind = match error {
            PdfError::PasswordRequired => "password-required",
            PdfError::EngineUnavailable(_) => "engine-unavailable",
            _ => "failed",
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

impl From<SessionError> for CommandError {
    fn from(error: SessionError) -> Self {
        Self::failed(error.to_string())
    }
}

/// an edit's own refusal and a session problem both become a message.
impl<E: Display> From<ApplyError<E>> for CommandError {
    fn from(error: ApplyError<E>) -> Self {
        Self::failed(error.to_string())
    }
}
