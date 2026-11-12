//! what every provider (the thing that runs the model) looks like.
//!
//! a provider takes a chat request and streams back an answer, and may also
//! turn text into embeddings. there are two here: a llama.cpp server on
//! this computer (`LocalServer`) and a service reached with the person's own
//! key (`RemoteApi`). tests use a fake one.
//!
//! **why a permit.** sending text to a provider needs a `Permit`, and only
//! this crate can make one, after the person has seen and accepted the
//! disclosure for exactly that request (see `disclosure.rs`). requests can
//! only be built inside this crate too. so code outside it can't hand a
//! provider any text without going through `plan` and `run`.
//!
//! ```compile_fail
//! // a permit can't be made outside the crate
//! let permit = letters_ai::Permit { destination: String::new(), leaves_device: true };
//! ```
//!
//! ```compile_fail
//! // and neither can a request
//! let request = letters_ai::ChatRequest { system: String::new(), user: String::new(), max_tokens: 1 };
//! ```

use crate::cancel::CancelToken;
use crate::AiError;

/// where a provider runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// a model server the person runs themselves.
    Local,
    /// a service on the internet, reached with the person's key.
    Remote,
}

/// what the settings screen and the disclosure say about a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfo {
    /// a short name for the ui: "Model on this computer", "Online service".
    pub name: String,
    pub kind: ProviderKind,
    /// true when text sent to it can leave this computer. decided from
    /// resolved addresses, never from how the address looks.
    pub sends_content_off_device: bool,
    /// the host requests go to, as the person typed it.
    pub host: String,
    /// how many tokens the model can read at once, prompt and answer
    /// together.
    pub context_tokens: usize,
    /// whether `embed` will do anything.
    pub supports_embeddings: bool,
    /// extra answer tokens for models that write out their reasoning
    /// before the answer. 0 for models that answer directly.
    pub thinking_tokens: usize,
}

/// one chat request: the rules in the system role, the document and the
/// question in the user role.
///
/// the fields are private on purpose: a request can only be assembled by
/// `prompts.rs`, which keeps document text out of the system role.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatRequest {
    pub(crate) system: String,
    pub(crate) user: String,
    pub(crate) max_tokens: usize,
}

impl ChatRequest {
    /// the instructions. never contains document text.
    pub fn system(&self) -> &str {
        &self.system
    }

    /// the document text (between delimiters) and the task.
    pub fn user(&self) -> &str {
        &self.user
    }

    /// the most tokens the answer may use.
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// all the characters the request carries, for the disclosure and
    /// for learning the token ratio.
    pub fn len_chars(&self) -> usize {
        self.system.chars().count() + self.user.chars().count()
    }
}

/// tokens a request really used, when the server says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

/// why the model stopped writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishReason {
    /// it finished its answer.
    Stop,
    /// it reached `max_tokens` and was cut off.
    Length,
    /// the server gave another reason, or none.
    Other,
}

/// a finished answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub text: String,
    pub usage: Option<Usage>,
    pub finish: FinishReason,
}

/// proof that the person agreed to this provider receiving the text.
///
/// only `run` makes these, after checking the acknowledgement against the
/// disclosure. providers check that the permit names them before sending.
#[derive(Debug)]
pub struct Permit {
    pub(crate) destination: String,
    pub(crate) leaves_device: bool,
}

impl Permit {
    pub(crate) fn for_provider(info: &ProviderInfo) -> Permit {
        Permit {
            destination: info.host.clone(),
            leaves_device: info.sends_content_off_device,
        }
    }

    /// refuses a permit made for somewhere else.
    ///
    /// a permit for "stays on this computer" can never be used to send to
    /// a provider that sends content away, even with the same host name.
    pub fn check(&self, info: &ProviderInfo) -> Result<(), AiError> {
        let same_host = self.destination == info.host;
        let covers_leaving = self.leaves_device || !info.sends_content_off_device;
        if same_host && covers_leaving {
            Ok(())
        } else {
            Err(AiError::ConsentRequired)
        }
    }
}

/// a place a model runs.
///
/// implementations must call `permit.check(&self.describe())` before
/// sending anything, call `on_text` with each streamed piece, and stop
/// with `AiError::Cancelled` soon after `cancel` is set.
pub trait Provider: Send + Sync {
    fn describe(&self) -> ProviderInfo;

    fn complete(
        &self,
        request: &ChatRequest,
        permit: &Permit,
        on_text: &mut dyn FnMut(&str),
        cancel: &CancelToken,
    ) -> Result<Completion, AiError>;

    /// one embedding per text, in the same order, or `None` when this
    /// provider can't make embeddings.
    fn embed(
        &self,
        texts: &[String],
        permit: &Permit,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<Vec<f32>>>, AiError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(host: &str, leaves: bool) -> ProviderInfo {
        ProviderInfo {
            name: "test".into(),
            kind: if leaves {
                ProviderKind::Remote
            } else {
                ProviderKind::Local
            },
            sends_content_off_device: leaves,
            host: host.into(),
            context_tokens: 4096,
            supports_embeddings: false,
            thinking_tokens: 0,
        }
    }

    #[test]
    fn a_permit_only_fits_its_own_provider() {
        let local = Permit::for_provider(&info("127.0.0.1", false));
        assert!(local.check(&info("127.0.0.1", false)).is_ok());
        // same host but now it would leave the computer: refused
        assert_eq!(
            local.check(&info("127.0.0.1", true)),
            Err(AiError::ConsentRequired)
        );
        let remote = Permit::for_provider(&info("api.example.com", true));
        assert!(remote.check(&info("api.example.com", true)).is_ok());
        assert_eq!(
            remote.check(&info("other.example.com", true)),
            Err(AiError::ConsentRequired)
        );
    }
}
