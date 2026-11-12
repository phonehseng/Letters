//! a stand-in for a llama.cpp server, for the process manager tests.
//!
//! it takes the same arguments Letters passes (`-m <model> --host <host>
//! --port <port>`), reads the "model" file to decide how to behave, and
//! answers `/health` and `/v1/chat/completions` with canned replies:
//!
//! - a model file containing `exit` makes it quit at once, like a server
//!   that can't load a damaged model;
//! - a model file containing `slow` makes it answer 503 ("loading") for
//!   the first second, like a real server loading a model.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value_after = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    let model = value_after("-m").expect("a model file is passed with -m");
    let host = value_after("--host").unwrap_or_else(|| "127.0.0.1".into());
    let port = value_after("--port").expect("a port is passed with --port");
    let behaviour = std::fs::read_to_string(&model).unwrap_or_default();
    if behaviour.contains("exit") {
        std::process::exit(3);
    }
    let loading_until = if behaviour.contains("slow") {
        Instant::now() + Duration::from_secs(1)
    } else {
        Instant::now()
    };
    let listener = TcpListener::bind(format!("{host}:{port}")).expect("the port is free");
    for stream in listener.incoming().flatten() {
        answer(stream, Instant::now() < loading_until);
    }
}

fn answer(mut stream: TcpStream, loading: bool) {
    let request_line = read_request(&stream);
    let reply = if request_line.starts_with("GET /health") {
        if loading {
            json(503, r#"{"error":{"message":"Loading model"}}"#)
        } else {
            json(200, r#"{"status":"ok"}"#)
        }
    } else if request_line.starts_with("POST /v1/chat/completions") {
        let piece =
            r#"{"choices":[{"delta":{"content":"Hello from the stub."},"finish_reason":"stop"}]}"#;
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {piece}\n\ndata: [DONE]\n\n"
        )
    } else {
        json(404, "{}")
    };
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}

/// reads the whole request and returns its first line.
fn read_request(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    let _ = reader.read_line(&mut first);
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    first
}

fn json(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
