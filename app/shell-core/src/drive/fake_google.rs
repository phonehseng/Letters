//! a stand-in for google's sign-in and drive endpoints, on this computer,
//! so the whole drive flow is tested without ever contacting google.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::drive::oauth::parse_query;
use crate::drive::pkce::challenge;

#[derive(Default)]
struct Memory {
    log: Vec<String>,
    folder: Option<String>,
}

pub struct FakeGoogle {
    pub base: String,
    memory: Arc<Mutex<Memory>>,
}

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl FakeGoogle {
    pub fn start() -> FakeGoogle {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let memory = Arc::new(Mutex::new(Memory::default()));
        let shared = memory.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let memory = shared.clone();
                std::thread::spawn(move || serve(stream, &memory));
            }
        });
        FakeGoogle { base, memory }
    }

    pub fn log(&self) -> Vec<String> {
        self.memory.lock().unwrap().log.clone()
    }
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let length = headers.iter().find(|(name, _)| name == "content-length").and_then(|(_, value)| value.parse().ok()).unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request { method, target, headers, body })
}

fn reply(mut stream: TcpStream, status: &str, json: &str) {
    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len());
}

fn serve(stream: TcpStream, memory: &Mutex<Memory>) {
    let Some(request) = read_request(&stream) else { return };
    let path = request.target.split('?').next().unwrap_or("").to_string();
    let header = |name: &str| request.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()).unwrap_or_default();
    match (request.method.as_str(), path.as_str()) {
        ("POST", "/token") => {
            let form = parse_query(&String::from_utf8_lossy(&request.body));
            let field = |name: &str| form.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()).unwrap_or_default();
            match field("grant_type").as_str() {
                "authorization_code" if field("code") == format!("code-for-{}", challenge(&field("code_verifier"))) => {
                    memory.lock().unwrap().log.push("token authorization_code ok".into());
                    reply(stream, "200 OK", r#"{"access_token":"access-1","refresh_token":"refresh-1","expires_in":3600}"#);
                }
                "refresh_token" if field("refresh_token") == "refresh-1" => {
                    memory.lock().unwrap().log.push("token refresh_token ok".into());
                    reply(stream, "200 OK", r#"{"access_token":"access-refreshed","expires_in":3600}"#);
                }
                _ => reply(stream, "400 Bad Request", r#"{"error":"invalid_grant"}"#),
            }
        }
        _ if !header("authorization").starts_with("Bearer access-") => reply(stream, "401 Unauthorized", "{}"),
        ("GET", "/drive/v3/files") => {
            let folder = memory.lock().unwrap().folder.clone();
            match folder {
                Some(name) => reply(stream, "200 OK", &format!(r#"{{"files":[{{"id":"folder-1","name":"{name}"}}]}}"#)),
                None => reply(stream, "200 OK", r#"{"files":[]}"#),
            }
        }
        ("POST", "/drive/v3/files") => {
            let metadata: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
            let name = metadata["name"].as_str().unwrap_or("").to_string();
            let mut memory = memory.lock().unwrap();
            memory.log.push(format!("create folder {name}"));
            memory.folder = Some(name);
            reply(stream, "200 OK", r#"{"id":"folder-1","name":"folder"}"#);
        }
        ("POST", "/upload/drive/v3/files") => {
            let boundary = header("content-type").split("boundary=").nth(1).unwrap_or("").to_string();
            let body = String::from_utf8_lossy(&request.body).into_owned();
            let parts: Vec<&str> = body.split(&format!("--{boundary}")).collect();
            let metadata: serde_json::Value = parts.get(1).and_then(|part| part.split("\r\n\r\n").nth(1)).and_then(|json| serde_json::from_str(json.trim()).ok()).unwrap_or_default();
            let media = parts.get(2).and_then(|part| part.split_once("\r\n\r\n")).map(|(_, bytes)| bytes.trim_end_matches("\r\n").len()).unwrap_or(0);
            let google_doc = metadata["mimeType"] == "application/vnd.google-apps.document";
            let parents = metadata["parents"][0].as_str().unwrap_or("none").to_string();
            memory.lock().unwrap().log.push(format!("upload {} parents={parents} bytes={media}", if google_doc { "google-doc" } else { "plain" }));
            let link = if google_doc { "https://docs.google.com/document/d/file-1/edit" } else { "https://drive.google.com/file/d/file-1/view" };
            reply(stream, "200 OK", &format!(r#"{{"id":"file-1","name":"{}","webViewLink":"{link}"}}"#, metadata["name"].as_str().unwrap_or("")));
        }
        _ => reply(stream, "404 Not Found", "{}"),
    }
}
