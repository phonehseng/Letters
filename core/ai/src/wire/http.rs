//! sending requests and turning every failure into a sentence.
//!
//! three safety rules live here:
//! - **no redirects.** a server that answers "go over there instead" could
//!   send the document somewhere the disclosure didn't name, so a redirect
//!   stops the request.
//! - **no proxy for the local server.** a proxy set for the whole computer
//!   would carry "local" text off the machine.
//! - **nothing from the server goes into an error.** error bodies are read
//!   only to tell "unknown model" from "too long", then dropped, because a
//!   server can quote the key back in its message.

use std::io::BufReader;
use std::time::Duration;

use ureq::http::Response;
use ureq::Body;

use crate::secret::ApiKey;
use crate::wire::chat_format::{mentions_context, mentions_missing_model, read_error};
use crate::AiError;

/// how long to wait at each step of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    pub connect: Duration,
    /// the longest gap between two pieces of an answer. a small model on
    /// a laptop can think for a while before the first word, so this is
    /// generous.
    pub between_reads: Duration,
}

impl Timeouts {
    pub fn with_read_seconds(seconds: u64) -> Self {
        Timeouts {
            connect: Duration::from_secs(10),
            between_reads: Duration::from_secs(seconds.max(1)),
        }
    }
}

/// one http client with the rules above built in.
pub struct HttpClient {
    agent: ureq::Agent,
    /// the host as typed, for error messages.
    host: String,
    /// the model name, so "unknown model" errors can name it.
    model: String,
}

/// whether system-wide proxy settings may be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyUse {
    /// for the local server: always connect directly.
    Never,
    /// for an online service: follow the computer's proxy settings, as a
    /// school network may require. the connection is still encrypted all
    /// the way to the service, so the proxy can't read it.
    FromSystem,
}

impl HttpClient {
    pub fn new(host: &str, model: &str, timeouts: Timeouts, proxy: ProxyUse) -> Self {
        let mut config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(timeouts.connect))
            .timeout_per_read(Some(timeouts.between_reads))
            .timeout_send_body(Some(Duration::from_secs(60)))
            .user_agent("Letters");
        if proxy == ProxyUse::Never {
            config = config.proxy(None);
        }
        HttpClient {
            agent: config.build().into(),
            host: host.to_string(),
            model: model.to_string(),
        }
    }

    /// posts json and returns the answer's body as a buffered reader.
    pub fn post_json(
        &self,
        url: &str,
        key: Option<&ApiKey>,
        body: &str,
    ) -> Result<BufReader<ureq::BodyReader<'static>>, AiError> {
        let mut request = self
            .agent
            .post(url)
            .header("content-type", "application/json");
        if let Some(key) = key {
            request = request.header("authorization", format!("Bearer {}", key.expose()));
        }
        let response = request
            .send(body)
            .map_err(|error| self.transport_error(error))?;
        let response = self.check_status(response)?;
        Ok(BufReader::new(response.into_body().into_reader()))
    }

    /// posts json and reads the whole answer as text.
    pub fn post_json_for_text(
        &self,
        url: &str,
        key: Option<&ApiKey>,
        body: &str,
    ) -> Result<String, AiError> {
        let mut reader = self.post_json(url, key, body)?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut reader, &mut text).map_err(read_error)?;
        Ok(text)
    }

    /// a plain get, returning only the status code. for health checks.
    pub fn get_status(&self, url: &str) -> Result<u16, AiError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| self.transport_error(error))?;
        Ok(response.status().as_u16())
    }

    fn check_status(&self, mut response: Response<Body>) -> Result<Response<Body>, AiError> {
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(response);
        }
        if (300..400).contains(&status) {
            return Err(AiError::Redirected);
        }
        // a short read: enough to recognise the message, never kept
        let message = response
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .unwrap_or_default();
        Err(error_for_status(status, &message, &self.model))
    }

    fn transport_error(&self, error: ureq::Error) -> AiError {
        match error {
            ureq::Error::Timeout(_) => AiError::TimedOut,
            ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => AiError::Redirected,
            ureq::Error::StatusCode(status) => error_for_status(status, "", &self.model),
            ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => AiError::TimedOut,
            ureq::Error::Protocol(_) => AiError::MalformedResponse,
            _ => AiError::NotReachable(self.host.clone()),
        }
    }
}

/// what an http error code means for the person.
pub fn error_for_status(status: u16, message: &str, model: &str) -> AiError {
    match status {
        401 | 403 => AiError::KeyRejected,
        413 => AiError::OutOfContext,
        429 => AiError::RateLimited,
        _ if mentions_missing_model(message) => AiError::ModelMissing(model.to_string()),
        _ if mentions_context(message) => AiError::OutOfContext,
        404 => AiError::InvalidAddress(
            "there’s no chat-completions service at that address.".to_string(),
        ),
        _ => AiError::ServerError(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_become_sentences() {
        assert_eq!(error_for_status(401, "", "m"), AiError::KeyRejected);
        assert_eq!(error_for_status(403, "", "m"), AiError::KeyRejected);
        assert_eq!(error_for_status(429, "", "m"), AiError::RateLimited);
        assert_eq!(
            error_for_status(404, "model 'm' not found", "m"),
            AiError::ModelMissing("m".into())
        );
        assert_eq!(
            error_for_status(400, "exceeds the context size", "m"),
            AiError::OutOfContext
        );
        assert_eq!(
            error_for_status(503, "loading", "m"),
            AiError::ServerError(503)
        );
        assert!(matches!(
            error_for_status(404, "", "m"),
            AiError::InvalidAddress(_)
        ));
    }
}
