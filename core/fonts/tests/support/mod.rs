//! helpers shared by the engine tests: fixture paths, temporary folders
//! and a tiny local http server, so no test ever contacts the internet.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// the fonts tools/fetch-fonts fetched, when they are there.
pub fn vendor_fonts() -> Option<PathBuf> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/fonts");
    folder.join("carlito").is_dir().then_some(folder)
}

/// a fresh, empty folder for one test.
pub fn temporary(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("letters-fonts-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// the windows fonts folder (or the mac one), when it exists.
pub fn system_font(file: &str) -> Option<PathBuf> {
    let windows = std::env::var_os("WINDIR").map(|dir| PathBuf::from(dir).join("Fonts").join(file));
    let mac = Some(PathBuf::from("/System/Library/Fonts/Supplemental").join(file));
    [windows, mac]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

/// what the test server saw.
#[derive(Default)]
pub struct Seen {
    pub paths: Vec<String>,
    pub ranges: Vec<Option<String>>,
}

/// serves the given files over plain http on 127.0.0.1, honouring
/// "Range: bytes=N-" requests. returns the base address.
pub fn serve(files: HashMap<String, Vec<u8>>, seen: Arc<Mutex<Seen>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            let mut range = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("range:") {
                    range = Some(value.trim().to_string());
                }
            }
            {
                let mut seen = seen.lock().unwrap();
                seen.paths.push(path.clone());
                seen.ranges.push(range.clone());
            }
            let Some(body) = files.get(&path) else {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            };
            let start = range
                .as_deref()
                .and_then(|range| range.strip_prefix("bytes="))
                .and_then(|range| range.trim_end_matches('-').parse::<usize>().ok())
                .filter(|start| *start < body.len());
            let response = match start {
                Some(start) => {
                    let part = &body[start..];
                    let mut head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nConnection: close\r\n\r\n",
                        part.len(),
                        body.len() - 1,
                        body.len()
                    )
                    .into_bytes();
                    head.extend_from_slice(part);
                    head
                }
                None => {
                    let mut head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    head.extend_from_slice(body);
                    head
                }
            };
            let _ = stream.write_all(&response);
        }
    });
    address
}
