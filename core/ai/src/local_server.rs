//! a llama.cpp server the person runs, usually on this computer.
//!
//! the server's chat-completions, embeddings and health endpoints are
//! used. whether text stays on this computer is decided once, when the
//! provider is made, by looking the address up: only when every address
//! it has is this computer does the disclosure say "stays on this
//! computer". requests then go to the exact ip that was checked.

use std::net::IpAddr;

use crate::address::{self, Address, Resolver, SystemResolver};
use crate::cancel::CancelToken;
use crate::provider::{ChatRequest, Completion, Permit, Provider, ProviderInfo, ProviderKind};
use crate::settings::LocalServerConfig;
use crate::wire::chat_format::{chat_body, embeddings_body, parse_embeddings, read_stream};
use crate::wire::http::{HttpClient, ProxyUse, Timeouts};
use crate::AiError;

const CHAT_ENDPOINT: &str = "/v1/chat/completions";
const EMBEDDINGS_ENDPOINT: &str = "/v1/embeddings";
const HEALTH_ENDPOINT: &str = "/health";

/// how many texts go into one embeddings request. small batches keep
/// each request quick, so Stop is noticed soon.
const EMBEDDING_BATCH: usize = 16;

pub struct LocalServer {
    config: LocalServerConfig,
    address: Address,
    /// the checked address that requests go to.
    ip: IpAddr,
    on_this_computer: bool,
    http: HttpClient,
}

impl LocalServer {
    pub fn new(config: &LocalServerConfig) -> Result<Self, AiError> {
        Self::with_resolver(config, &SystemResolver)
    }

    /// the same, with a stand-in for dns (for tests).
    pub fn with_resolver(
        config: &LocalServerConfig,
        resolver: &dyn Resolver,
    ) -> Result<Self, AiError> {
        let mut address = Address::parse(&config.address)?;
        // people often paste the address with the /v1 already on it
        if address.path.ends_with("/v1") {
            address.path.truncate(address.path.len() - 3);
        }
        let resolved = address::resolve(&address, resolver)?;
        let ip = choose_ip(&resolved.addresses);
        let proxy = if resolved.loopback {
            ProxyUse::Never
        } else {
            ProxyUse::FromSystem
        };
        let http = HttpClient::new(
            &address.host,
            config.model.as_deref().unwrap_or("the loaded model"),
            Timeouts::with_read_seconds(config.read_timeout_seconds),
            proxy,
        );
        Ok(LocalServer {
            config: config.clone(),
            address,
            ip,
            on_this_computer: resolved.loopback,
            http,
        })
    }

    /// true once the server has loaded its model and is ready.
    pub fn is_ready(&self) -> bool {
        matches!(self.http.get_status(&self.url(HEALTH_ENDPOINT)), Ok(200))
    }

    fn url(&self, endpoint: &str) -> String {
        self.address.url_for_ip(self.ip, endpoint)
    }
}

/// prefers a loopback address when a name has several.
fn choose_ip(addresses: &[IpAddr]) -> IpAddr {
    *addresses
        .iter()
        .find(|ip| address::is_loopback(ip))
        .unwrap_or(&addresses[0])
}

impl Provider for LocalServer {
    fn describe(&self) -> ProviderInfo {
        let name = if self.on_this_computer {
            "Model on this computer"
        } else {
            "Model server on another computer"
        };
        ProviderInfo {
            name: name.to_string(),
            kind: ProviderKind::Local,
            sends_content_off_device: !self.on_this_computer,
            host: self.address.host.clone(),
            context_tokens: self.config.context_tokens,
            supports_embeddings: self.config.embeddings,
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
        let body = chat_body(request, self.config.model.as_deref(), true);
        let reader = self.http.post_json(&self.url(CHAT_ENDPOINT), None, &body)?;
        read_stream(reader, on_text, cancel)
    }

    fn embed(
        &self,
        texts: &[String],
        permit: &Permit,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<Vec<f32>>>, AiError> {
        if !self.config.embeddings {
            return Ok(None);
        }
        permit.check(&self.describe())?;
        let mut vectors = Vec::with_capacity(texts.len());
        for batch in texts.chunks(EMBEDDING_BATCH) {
            cancel.check()?;
            let body = embeddings_body(batch, self.config.model.as_deref());
            let answer =
                self.http
                    .post_json_for_text(&self.url(EMBEDDINGS_ENDPOINT), None, &body)?;
            vectors.extend(parse_embeddings(&answer, batch.len())?);
        }
        Ok(Some(vectors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_response, sse_response, streamed_answer, Reply, TinyServer};
    use std::collections::HashMap;
    use std::time::Duration;

    struct FakeResolver(HashMap<&'static str, Vec<IpAddr>>);

    impl Resolver for FakeResolver {
        fn resolve(&self, host: &str, _port: u16) -> std::io::Result<Vec<IpAddr>> {
            self.0
                .get(host)
                .cloned()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "unknown"))
        }
    }

    fn server_at(address: String) -> LocalServer {
        let config = LocalServerConfig {
            address,
            embeddings: true,
            read_timeout_seconds: 5,
            ..Default::default()
        };
        LocalServer::new(&config).unwrap()
    }

    fn request() -> ChatRequest {
        crate::prompts::build_request(
            "Summarise.",
            &[("[page 1]".into(), "Some text.".into())],
            None,
            50,
        )
    }

    fn ask(server: &LocalServer) -> (Result<Completion, AiError>, String) {
        let permit = Permit::for_provider(&server.describe());
        let mut seen = String::new();
        let result = server.complete(
            &request(),
            &permit,
            &mut |t| seen.push_str(t),
            &CancelToken::new(),
        );
        (result, seen)
    }

    #[test]
    fn streams_an_answer_from_the_chat_endpoint() {
        let tiny = TinyServer::start(vec![Reply::Send(sse_response(&streamed_answer(&[
            "Hello", " there.",
        ])))]);
        let server = server_at(tiny.address());
        let (result, seen) = ask(&server);
        assert_eq!(result.unwrap().text, "Hello there.");
        assert_eq!(seen, "Hello there.");
        let raw = &tiny.seen()[0];
        assert!(raw.starts_with("POST /v1/chat/completions "), "{raw}");
        assert!(!raw.to_ascii_lowercase().contains("authorization"));
        assert!(raw.contains("\"stream\":true"));
    }

    #[test]
    fn a_pasted_v1_is_not_doubled() {
        let tiny = TinyServer::start(vec![Reply::Send(sse_response(&streamed_answer(&["ok"])))]);
        let server = server_at(format!("{}/v1/", tiny.address()));
        assert!(ask(&server).0.is_ok());
        assert!(tiny.seen()[0].starts_with("POST /v1/chat/completions "));
    }

    #[test]
    fn a_whole_json_answer_is_accepted() {
        let body =
            r#"{"choices":[{"message":{"content":"Whole answer."},"finish_reason":"stop"}]}"#;
        let tiny = TinyServer::start(vec![Reply::Send(json_response(200, body))]);
        assert_eq!(
            ask(&server_at(tiny.address())).0.unwrap().text,
            "Whole answer."
        );
    }

    #[test]
    fn malformed_and_interrupted_streams_are_errors() {
        let garbage = sse_response(&["{not json".to_string()]);
        let mut cut = sse_response(&streamed_answer(&["Half"])[..1]);
        cut.push_str("data: {\"choices\":[{\"de");
        let tiny = TinyServer::start(vec![Reply::Send(garbage), Reply::Send(cut)]);
        let server = server_at(tiny.address());
        assert_eq!(ask(&server).0, Err(AiError::MalformedResponse));
        let (result, seen) = ask(&server);
        assert_eq!(result, Err(AiError::StreamInterrupted));
        assert_eq!(seen, "Half");
    }

    #[test]
    fn a_stalled_stream_times_out() {
        let head = sse_response(&streamed_answer(&["Slow"])[..1]);
        let tiny = TinyServer::start(vec![Reply::SendThenStall(head, Duration::from_secs(4))]);
        let config = LocalServerConfig {
            address: tiny.address(),
            read_timeout_seconds: 1,
            ..Default::default()
        };
        let server = LocalServer::new(&config).unwrap();
        assert_eq!(ask(&server).0, Err(AiError::TimedOut));
    }

    #[test]
    fn server_errors_become_sentences() {
        let tiny = TinyServer::start(vec![
            Reply::Send(json_response(
                400,
                r#"{"error":{"message":"the request exceeds the available context size"}}"#,
            )),
            Reply::Send(json_response(
                503,
                r#"{"error":{"message":"Loading model"}}"#,
            )),
            Reply::Send(
                "HTTP/1.1 307 X\r\nLocation: http://elsewhere.test/\r\nContent-Length: 0\r\n\r\n"
                    .to_string(),
            ),
        ]);
        let server = server_at(tiny.address());
        assert_eq!(ask(&server).0, Err(AiError::OutOfContext));
        assert_eq!(ask(&server).0, Err(AiError::ServerError(503)));
        assert_eq!(ask(&server).0, Err(AiError::Redirected));
    }

    #[test]
    fn a_server_that_is_not_running_is_not_reachable() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let server = server_at(format!("http://127.0.0.1:{port}"));
        assert_eq!(
            ask(&server).0,
            Err(AiError::NotReachable("127.0.0.1".into()))
        );
        assert!(!server.is_ready());
    }

    #[test]
    fn embeddings_come_back_in_order() {
        let body =
            r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[0.0,1.0]}]}"#;
        let tiny = TinyServer::start(vec![Reply::Send(json_response(200, body))]);
        let server = server_at(tiny.address());
        let permit = Permit::for_provider(&server.describe());
        let vectors = server
            .embed(&["a".into(), "b".into()], &permit, &CancelToken::new())
            .unwrap()
            .unwrap();
        assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert!(tiny.seen()[0].starts_with("POST /v1/embeddings "));
    }

    #[test]
    fn without_embeddings_nothing_is_sent() {
        let config = LocalServerConfig {
            address: "http://127.0.0.1:9".into(),
            ..Default::default()
        };
        let server = LocalServer::new(&config).unwrap();
        let permit = Permit::for_provider(&server.describe());
        assert_eq!(
            server.embed(&["a".into()], &permit, &CancelToken::new()),
            Ok(None)
        );
    }

    #[test]
    fn health_is_checked() {
        let tiny = TinyServer::start(vec![Reply::Send(json_response(200, r#"{"status":"ok"}"#))]);
        assert!(server_at(tiny.address()).is_ready());
        assert!(tiny.seen()[0].starts_with("GET /health "));
    }

    #[test]
    fn where_it_runs_follows_the_resolved_address() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let resolver = FakeResolver(HashMap::from([
            ("localhost", vec![ip("10.0.0.5")]),
            ("my-box", vec![ip("::1")]),
        ]));
        let make = |address: &str| {
            let config = LocalServerConfig {
                address: address.into(),
                ..Default::default()
            };
            LocalServer::with_resolver(&config, &resolver)
                .unwrap()
                .describe()
        };
        let elsewhere = make("http://localhost:8080");
        assert!(elsewhere.sends_content_off_device);
        assert_eq!(elsewhere.name, "Model server on another computer");
        assert!(!make("http://my-box:8080").sends_content_off_device);
        assert!(
            !server_at("http://127.0.0.1:8080".into())
                .describe()
                .sends_content_off_device
        );
        assert!(
            !server_at("http://[::1]:8080".into())
                .describe()
                .sends_content_off_device
        );
    }

    #[test]
    fn a_permit_for_staying_cannot_send_away() {
        let resolver = FakeResolver(HashMap::from([(
            "my-box",
            vec!["192.168.1.20".parse().unwrap()],
        )]));
        let config = LocalServerConfig {
            address: "http://my-box:8080".into(),
            ..Default::default()
        };
        let server = LocalServer::with_resolver(&config, &resolver).unwrap();
        let mut info = server.describe();
        info.sends_content_off_device = false;
        let stays = Permit::for_provider(&info);
        let result = server.complete(&request(), &stays, &mut |_| {}, &CancelToken::new());
        assert_eq!(result, Err(AiError::ConsentRequired));
    }
}
