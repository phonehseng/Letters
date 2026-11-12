//! stand-ins used by the tests: a fake provider and a tiny http server.
//!
//! neither touches the network beyond 127.0.0.1.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cancel::CancelToken;
use crate::provider::{
    ChatRequest, Completion, FinishReason, Permit, Provider, ProviderInfo, ProviderKind, Usage,
};
use crate::retrieval::terms;
use crate::AiError;

type AnswerFn = Box<dyn Fn(&ChatRequest) -> String + Send + Sync>;

/// a provider that answers from a closure and remembers every request.
pub struct FakeProvider {
    pub info: ProviderInfo,
    answer: AnswerFn,
    pub requests: Mutex<Vec<ChatRequest>>,
    pub embedded: Mutex<Vec<String>>,
}

impl FakeProvider {
    pub fn local(context_tokens: usize) -> Self {
        FakeProvider {
            info: ProviderInfo {
                name: "Model on this computer".into(),
                kind: ProviderKind::Local,
                sends_content_off_device: false,
                host: "127.0.0.1".into(),
                context_tokens,
                supports_embeddings: false,
                thinking_tokens: 0,
            },
            answer: Box::new(|_| "An answer.".to_string()),
            requests: Mutex::new(Vec::new()),
            embedded: Mutex::new(Vec::new()),
        }
    }

    pub fn remote(context_tokens: usize) -> Self {
        let mut fake = Self::local(context_tokens);
        fake.info.name = "Online service".into();
        fake.info.kind = ProviderKind::Remote;
        fake.info.sends_content_off_device = true;
        fake.info.host = "api.example.com".into();
        fake
    }

    pub fn answering(
        mut self,
        answer: impl Fn(&ChatRequest) -> String + Send + Sync + 'static,
    ) -> Self {
        self.answer = Box::new(answer);
        self
    }

    pub fn with_embeddings(mut self) -> Self {
        self.info.supports_embeddings = true;
        self
    }

    pub fn sent(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Provider for FakeProvider {
    fn describe(&self) -> ProviderInfo {
        self.info.clone()
    }

    fn complete(
        &self,
        request: &ChatRequest,
        permit: &Permit,
        on_text: &mut dyn FnMut(&str),
        cancel: &CancelToken,
    ) -> Result<Completion, AiError> {
        permit.check(&self.info)?;
        cancel.check()?;
        self.requests.lock().unwrap().push(request.clone());
        let text = (self.answer)(request);
        // stream it in a few pieces, as a server would
        let words: Vec<&str> = text.split_inclusive(' ').collect();
        for piece in words.chunks(3) {
            cancel.check()?;
            on_text(&piece.concat());
        }
        Ok(Completion {
            text,
            usage: Some(Usage {
                prompt_tokens: request.len_chars() / 4,
                completion_tokens: 10,
            }),
            finish: FinishReason::Stop,
        })
    }

    fn embed(
        &self,
        texts: &[String],
        permit: &Permit,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<Vec<f32>>>, AiError> {
        if !self.info.supports_embeddings {
            return Ok(None);
        }
        permit.check(&self.info)?;
        cancel.check()?;
        self.embedded.lock().unwrap().extend(texts.iter().cloned());
        Ok(Some(
            texts.iter().map(|text| fake_embedding(text)).collect(),
        ))
    }
}

/// a bag of words in 64 slots, where a few words that mean the same
/// thing share a slot. enough to show "meaning" beating word matches.
pub fn fake_embedding(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0; 64];
    for term in terms(text) {
        let concept = match term.as_str() {
            "money" | "financ" | "budget" | "fund" => "money",
            other => other,
        };
        let mut hasher = DefaultHasher::new();
        concept.hash(&mut hasher);
        vector[(hasher.finish() % 64) as usize] += 1.0;
    }
    vector
}

/// a one-thread http server that answers each connection with the next
/// canned response, exactly as given, and then closes the connection.
pub struct TinyServer {
    pub port: u16,
    pub requests: Arc<Mutex<Vec<String>>>,
    handle: Option<JoinHandle<()>>,
}

/// what the server does with one connection.
pub enum Reply {
    /// send these bytes and close.
    Send(String),
    /// send these bytes, then wait before closing (for timeouts).
    SendThenStall(String, Duration),
}

impl TinyServer {
    pub fn start(replies: Vec<Reply>) -> TinyServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for reply in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                serve_one(stream, reply, &seen);
            }
        });
        TinyServer {
            port,
            requests,
            handle: Some(handle),
        }
    }

    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn seen(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for TinyServer {
    fn drop(&mut self) {
        // only join when every reply was used, or the thread would block
        // in accept; an unfinished one just ends with the test process
        if let Some(handle) = self.handle.take() {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

fn serve_one(mut stream: TcpStream, reply: Reply, seen: &Mutex<Vec<String>>) {
    let request = read_request(&stream);
    seen.lock().unwrap().push(request);
    let (bytes, stall) = match reply {
        Reply::Send(bytes) => (bytes, None),
        Reply::SendThenStall(bytes, wait) => (bytes, Some(wait)),
    };
    let _ = stream.write_all(bytes.as_bytes());
    let _ = stream.flush();
    if let Some(wait) = stall {
        thread::sleep(wait);
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// the request line, headers and body as one string.
fn read_request(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut head = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    head + &String::from_utf8_lossy(&body)
}

/// a complete event-stream response.
pub fn sse_response(events: &[String]) -> String {
    let mut text = String::from(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
    );
    for event in events {
        text.push_str("data: ");
        text.push_str(event);
        text.push_str("\n\n");
    }
    text
}

/// a json response with a status code.
pub fn json_response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// the stream of pieces for one streamed answer, ending with [DONE].
pub fn streamed_answer(pieces: &[&str]) -> Vec<String> {
    let mut events: Vec<String> = pieces
        .iter()
        .map(|piece| {
            serde_json::json!({"choices":[{"delta":{"content":piece},"finish_reason":null}]})
                .to_string()
        })
        .collect();
    events.push(serde_json::json!({"choices":[{"delta":{},"finish_reason":"stop"}]}).to_string());
    events.push("[DONE]".to_string());
    events
}
