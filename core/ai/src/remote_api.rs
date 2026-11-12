//! an online service that speaks the common chat-completions format,
//! reached with a key the person supplies.
//!
//! everything sent here leaves this computer, so `describe` always says
//! so. the connection must be https; plain http is only accepted for an
//! address on this computer (a local relay, or the tests), because over
//! plain http anyone on the network could read the key and the text.

use crate::address::{self, Address, Resolver, SystemResolver};
use crate::cancel::CancelToken;
use crate::provider::{ChatRequest, Completion, Permit, Provider, ProviderInfo, ProviderKind};
use crate::secret::ApiKey;
use crate::settings::RemoteApiConfig;
use crate::wire::chat_format::{chat_body, embeddings_body, parse_embeddings, read_stream};
use crate::wire::http::{HttpClient, ProxyUse, Timeouts};
use crate::AiError;

const CHAT_ENDPOINT: &str = "/chat/completions";
const EMBEDDINGS_ENDPOINT: &str = "/embeddings";
const EMBEDDING_BATCH: usize = 32;

pub struct RemoteApi {
    config: RemoteApiConfig,
    address: Address,
    key: ApiKey,
    http: HttpClient,
}

impl std::fmt::Debug for RemoteApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteApi")
            .field("address", &self.address)
            .field("model", &self.config.model)
            .field("key", &self.key)
            .finish()
    }
}

impl RemoteApi {
    pub fn new(config: &RemoteApiConfig, key: ApiKey) -> Result<Self, AiError> {
        Self::with_resolver(config, key, &SystemResolver)
    }

    pub fn with_resolver(
        config: &RemoteApiConfig,
        key: ApiKey,
        resolver: &dyn Resolver,
    ) -> Result<Self, AiError> {
        let address = Address::parse(&config.address)?;
        if config.model.trim().is_empty() {
            return Err(AiError::ModelNotChosen);
        }
        if key.is_empty() {
            return Err(AiError::KeyMissing);
        }
        let mut proxy = ProxyUse::FromSystem;
        if !address.secure {
            // only allowed when the address is this computer
            let resolved = address::resolve(&address, resolver)?;
            if !resolved.loopback {
                return Err(AiError::InsecureAddress);
            }
            proxy = ProxyUse::Never;
        }
        let http = HttpClient::new(
            &address.host,
            &config.model,
            Timeouts::with_read_seconds(config.read_timeout_seconds),
            proxy,
        );
        Ok(RemoteApi {
            config: config.clone(),
            address,
            key,
            http,
        })
    }

    fn embedding_model(&self) -> Option<&str> {
        self.config
            .embedding_model
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
    }
}

impl Provider for RemoteApi {
    fn describe(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Online service".to_string(),
            kind: ProviderKind::Remote,
            sends_content_off_device: true,
            host: self.address.host.clone(),
            context_tokens: self.config.context_tokens,
            supports_embeddings: self.embedding_model().is_some(),
            thinking_tokens: self.config.thinking_tokens,
        }
    }

    fn complete(
        &self,
        request: &ChatRequest,
        permit: &Permit,
        on_text: &mut dyn FnMut(&str),
        cancel: &CancelToken,
    ) -> Result<Completion, AiError> {
        permit.check(&self.describe())?;
        cancel.check()?;
        let body = chat_body(request, Some(self.config.model.trim()), true);
        let reader =
            self.http
                .post_json(&self.address.url(CHAT_ENDPOINT), Some(&self.key), &body)?;
        read_stream(reader, on_text, cancel)
    }

    fn embed(
        &self,
        texts: &[String],
        permit: &Permit,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<Vec<f32>>>, AiError> {
        let Some(model) = self.embedding_model() else {
            return Ok(None);
        };
        permit.check(&self.describe())?;
        let mut vectors = Vec::with_capacity(texts.len());
        for batch in texts.chunks(EMBEDDING_BATCH) {
            cancel.check()?;
            let body = embeddings_body(batch, Some(model));
            let answer = self.http.post_json_for_text(
                &self.address.url(EMBEDDINGS_ENDPOINT),
                Some(&self.key),
                &body,
            )?;
            vectors.extend(parse_embeddings(&answer, batch.len())?);
        }
        Ok(Some(vectors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_response, sse_response, streamed_answer, Reply, TinyServer};
    use std::net::IpAddr;

    const KEY: &str = "test-key-1234567890";

    /// says every name is somewhere on the internet.
    struct Elsewhere;

    impl Resolver for Elsewhere {
        fn resolve(&self, _host: &str, _port: u16) -> std::io::Result<Vec<IpAddr>> {
            Ok(vec!["203.0.113.7".parse().unwrap()])
        }
    }

    fn config(address: String) -> RemoteApiConfig {
        RemoteApiConfig {
            address,
            model: "my-remote-model".into(),
            read_timeout_seconds: 5,
            ..Default::default()
        }
    }

    fn api(tiny: &TinyServer) -> RemoteApi {
        RemoteApi::new(&config(format!("{}/v1", tiny.address())), ApiKey::new(KEY)).unwrap()
    }

    fn ask(api: &RemoteApi) -> Result<Completion, AiError> {
        let request = crate::prompts::build_request(
            "Summarise.",
            &[("[page 1]".into(), "Some text.".into())],
            None,
            50,
        );
        let permit = Permit::for_provider(&api.describe());
        api.complete(&request, &permit, &mut |_| {}, &CancelToken::new())
    }

    #[test]
    fn sends_the_key_and_model_and_streams_back() {
        let tiny = TinyServer::start(vec![Reply::Send(sse_response(&streamed_answer(&[
            "Fine", ".",
        ])))]);
        let api = api(&tiny);
        assert_eq!(ask(&api).unwrap().text, "Fine.");
        let raw = &tiny.seen()[0];
        assert!(raw.starts_with("POST /v1/chat/completions "), "{raw}");
        assert!(raw.to_ascii_lowercase().contains(&format!(
            "authorization: bearer {}",
            KEY.to_ascii_lowercase()
        )));
        assert!(raw.contains("\"model\":\"my-remote-model\""));
    }

    #[test]
    fn it_always_says_content_leaves() {
        let tiny = TinyServer::start(vec![]);
        let info = api(&tiny).describe();
        assert!(info.sends_content_off_device);
        assert_eq!(info.kind, ProviderKind::Remote);
        assert_eq!(info.host, "127.0.0.1");
    }

    #[test]
    fn a_rejected_key_never_shows_up_in_the_error() {
        // a server that quotes the key back in its message
        let echo = format!(r#"{{"error":{{"message":"Incorrect key provided: {KEY}"}}}}"#);
        let tiny = TinyServer::start(vec![Reply::Send(json_response(401, &echo))]);
        let error = ask(&api(&tiny)).unwrap_err();
        assert_eq!(error, AiError::KeyRejected);
        assert!(!format!("{error}").contains(KEY));
        assert!(!format!("{error:?}").contains(KEY));
    }

    #[test]
    fn the_key_is_hidden_when_the_provider_is_printed() {
        let tiny = TinyServer::start(vec![]);
        let printed = format!("{:?}", api(&tiny));
        assert!(!printed.contains(KEY));
        assert!(printed.contains("***"));
    }

    #[test]
    fn errors_are_sentences() {
        let tiny = TinyServer::start(vec![
            Reply::Send(json_response(
                404,
                r#"{"error":{"message":"The model `my-remote-model` does not exist"}}"#,
            )),
            Reply::Send(json_response(429, "{}")),
            Reply::Send(json_response(500, "oops")),
        ]);
        let api = api(&tiny);
        assert_eq!(
            ask(&api),
            Err(AiError::ModelMissing("my-remote-model".into()))
        );
        assert_eq!(ask(&api), Err(AiError::RateLimited));
        assert_eq!(ask(&api), Err(AiError::ServerError(500)));
    }

    #[test]
    fn an_interrupted_stream_is_reported() {
        let cut = sse_response(&streamed_answer(&["Par"])[..1]);
        let tiny = TinyServer::start(vec![Reply::Send(cut)]);
        assert_eq!(ask(&api(&tiny)), Err(AiError::StreamInterrupted));
    }

    #[test]
    fn plain_http_is_only_allowed_on_this_computer() {
        let error = RemoteApi::with_resolver(
            &config("http://api.example.com/v1".into()),
            ApiKey::new(KEY),
            &Elsewhere,
        )
        .unwrap_err();
        assert_eq!(error, AiError::InsecureAddress);
        // https needs no lookup to be allowed
        assert!(RemoteApi::with_resolver(
            &config("https://api.example.com/v1".into()),
            ApiKey::new(KEY),
            &Elsewhere
        )
        .is_ok());
    }

    #[test]
    fn a_key_and_a_model_are_required() {
        let address = "https://api.example.com/v1".to_string();
        assert_eq!(
            RemoteApi::new(&config(address.clone()), ApiKey::new("  ")).unwrap_err(),
            AiError::KeyMissing
        );
        let no_model = RemoteApiConfig {
            model: " ".into(),
            ..config(address)
        };
        assert_eq!(
            RemoteApi::new(&no_model, ApiKey::new(KEY)).unwrap_err(),
            AiError::ModelNotChosen
        );
    }

    #[test]
    fn embeddings_need_an_embedding_model() {
        let body = r#"{"data":[{"index":0,"embedding":[0.5,0.5]}]}"#;
        let tiny = TinyServer::start(vec![Reply::Send(json_response(200, body))]);
        let plain = api(&tiny);
        let permit = Permit::for_provider(&plain.describe());
        assert_eq!(
            plain.embed(&["a".into()], &permit, &CancelToken::new()),
            Ok(None)
        );
        let with_model = RemoteApiConfig {
            embedding_model: Some("my-embedding-model".into()),
            ..config(format!("{}/v1", tiny.address()))
        };
        let api = RemoteApi::new(&with_model, ApiKey::new(KEY)).unwrap();
        assert!(api.describe().supports_embeddings);
        let permit = Permit::for_provider(&api.describe());
        let vectors = api
            .embed(&["a".into()], &permit, &CancelToken::new())
            .unwrap()
            .unwrap();
        assert_eq!(vectors, vec![vec![0.5, 0.5]]);
        let raw = &tiny.seen()[0];
        assert!(raw.starts_with("POST /v1/embeddings "));
        assert!(raw.contains("my-embedding-model"));
    }

    #[test]
    fn a_permit_for_another_host_is_refused_before_sending() {
        let tiny = TinyServer::start(vec![]);
        let api = api(&tiny);
        let mut other = api.describe();
        other.host = "other.example.com".into();
        let request = crate::prompts::build_request("x", &[], None, 5);
        let result = api.complete(
            &request,
            &Permit::for_provider(&other),
            &mut |_| {},
            &CancelToken::new(),
        );
        assert_eq!(result, Err(AiError::ConsentRequired));
        assert!(tiny.seen().is_empty());
    }
}
