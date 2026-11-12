use thiserror::Error;

/// everything that can go wrong with the ai tools.
///
/// the ui shows these messages as they are, so each one is a plain
/// sentence for the person using the app that says what to do next.
/// none of them ever holds the api key or text the server sent back,
/// because a server can echo a key inside its own error message.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum AiError {
    #[error("AI tools are turned off. Turn them on in Settings to use them.")]
    Disabled,

    #[error("Choose where AI tools should run in Settings first.")]
    NoProvider,

    #[error("The server address isn’t right: {0}")]
    InvalidAddress(String),

    #[error("This address doesn’t use a secure connection, so the key and the document could be read on the way. Use an address that starts with https://.")]
    InsecureAddress,

    #[error("Letters can’t reach {0}. Check the address, and that the server is running.")]
    NotReachable(String),

    #[error("Add the key for this service in Settings first.")]
    KeyMissing,

    #[error("The key couldn’t be read from this computer’s password store. ({0})")]
    KeyStore(String),

    #[error("The service didn’t accept the key. Check it in Settings, or make a new one with the service.")]
    KeyRejected,

    #[error("Type the name of the model to use in Settings first.")]
    ModelNotChosen,

    #[error("The server doesn’t have a model called “{0}”. Check the model name in Settings.")]
    ModelMissing(String),

    #[error("This is too long for the model to read at once. Choose fewer pages, or a model that can read more.")]
    OutOfContext,

    #[error(
        "The service is busy, or this key has used up its allowance. Try again in a little while."
    )]
    RateLimited,

    #[error("The server had a problem (error {0}). Try again in a little while.")]
    ServerError(u16),

    #[error("The server took too long to answer. Try again, or choose fewer pages.")]
    TimedOut,

    #[error("The server tried to send the request somewhere else, so Letters stopped. Check the address.")]
    Redirected,

    #[error("The answer stopped part way through. Try again.")]
    StreamInterrupted,

    #[error("The server answered in a way Letters doesn’t understand. Check that the address points to a server that speaks the chat-completions format.")]
    MalformedResponse,

    #[error("The model didn’t write an answer. If it is a model that thinks before answering, give it more room for thinking in Settings, or choose another model.")]
    NoAnswer,

    #[error("Stopped.")]
    Cancelled,

    #[error(
        "This would send document text off this computer, and that hasn’t been confirmed yet."
    )]
    ConsentRequired,

    #[error("That confirmation was for a different request. Ask again to see what will be sent.")]
    ConsentMismatch,

    #[error("The AI tools settings changed since this was prepared. Ask again to see what will be sent.")]
    ProviderChanged,

    #[error("These pages have no text to read. If they are scanned, run text recognition first.")]
    NothingToRead,

    #[error("Those pages aren’t in this document.")]
    InvalidPages,

    #[error("Type what to look for first.")]
    EmptyQuery,

    #[error("Letters can’t find the {0}. Choose it again in Settings.")]
    FileMissing(String),

    #[error("The model server couldn’t start. Check that the program and the model file are the right ones. ({0})")]
    ServerStartFailed(String),

    #[error("The model server stopped while it was starting. The model file may be damaged or too big for this computer.")]
    ServerExited,
}

impl AiError {
    /// true for problems that asking again later might fix.
    ///
    /// the ui uses it to decide between a "Try Again" button and a
    /// "Open Settings" button.
    pub fn is_temporary(&self) -> bool {
        matches!(
            self,
            AiError::NotReachable(_)
                | AiError::RateLimited
                | AiError::ServerError(_)
                | AiError::TimedOut
                | AiError::StreamInterrupted
        )
    }
}
