//! signing in to google for drive uploads, the way google asks installed
//! apps to do it: the system browser shows google's own sign-in page, google
//! sends the browser back to a one-time address on this computer (a
//! "loopback" redirect), and the app swaps the code for a token, proving
//! with pkce that it started the sign-in.
//!
//! the scope is `drive.file`: Letters can only see and change files it
//! created itself, never anything else in the person's drive.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// only files Letters creates.
pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

/// google's addresses, or a test server's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    pub authorize: String,
    pub token: String,
    pub upload: String,
    pub files: String,
}

impl Endpoints {
    pub fn google() -> Endpoints {
        Endpoints {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            upload: "https://www.googleapis.com/upload/drive/v3/files".into(),
            files: "https://www.googleapis.com/drive/v3/files".into(),
        }
    }

    /// every address on one local test server.
    pub fn local(base: &str) -> Endpoints {
        Endpoints {
            authorize: format!("{base}/authorize"),
            token: format!("{base}/token"),
            upload: format!("{base}/upload/drive/v3/files"),
            files: format!("{base}/drive/v3/files"),
        }
    }
}

/// the app's registration with google (from Settings).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub client_id: String,
    /// google gives desktop apps a "secret" that is not secret; some
    /// registrations need it in the token request
    #[serde(default)]
    pub client_secret: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// seconds since 1970
    pub expires_at: u64,
}

impl Tokens {
    /// a token about to run out is refreshed a minute early, so an upload
    /// never starts with one that ends half way.
    pub fn needs_refresh(&self, now: u64) -> bool {
        now + 60 >= self.expires_at
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// percent-encodes a value for a url or a form.
pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    out.push(byte);
                    index += 2;
                }
            }
            other => out.push(other),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// the page the browser opens to sign in.
pub fn authorization_url(
    endpoints: &Endpoints,
    client: &Client,
    redirect: &str,
    challenge: &str,
    state: &str,
) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        endpoints.authorize,
        encode(&client.client_id),
        encode(redirect),
        encode(DRIVE_SCOPE),
        encode(challenge),
        encode(state)
    )
}

/// the one-time address on this computer google sends the browser back to.
pub struct Loopback {
    listener: TcpListener,
}

impl Loopback {
    pub fn bind() -> Result<Loopback, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("Letters couldn’t listen for the sign-in ({error})."))?;
        Ok(Loopback { listener })
    }

    pub fn redirect_uri(&self) -> String {
        let port = self
            .listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(0);
        format!("http://127.0.0.1:{port}")
    }

    /// waits for the browser to come back with the code, checking `state`.
    pub fn wait_for_code(&self, state: &str, timeout: Duration) -> Result<String, String> {
        self.listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(answer) = answer_browser(stream, state)? {
                        return answer;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() > timeout {
                        return Err("The sign-in was not finished in time. Try again.".into());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}

/// reads one browser request; None for requests that are not the answer (a favicon).
fn answer_browser(
    stream: TcpStream,
    state: &str,
) -> Result<Option<Result<String, String>>, String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let target = line.split_whitespace().nth(1).unwrap_or("");
    let Some((_, query)) = target.split_once('?') else {
        respond(stream, "404 Not Found", "");
        return Ok(None);
    };
    let parameters = parse_query(query);
    let value = |name: &str| {
        parameters
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let result = if value("state").as_deref() != Some(state) {
        Err("The sign-in answer didn’t match this sign-in, so it was ignored.".to_string())
    } else if let Some(code) = value("code") {
        Ok(code)
    } else {
        Err(format!(
            "Google didn’t sign you in ({}).",
            value("error").unwrap_or_else(|| "no reason given".into())
        ))
    };
    let page = if result.is_ok() {
        "You are signed in. You can close this tab and go back to Letters."
    } else {
        "The sign-in didn’t finish. You can close this tab and try again in Letters."
    };
    respond(stream, "200 OK", page);
    Ok(Some(result))
}

fn respond(mut stream: TcpStream, status: &str, text: &str) {
    let body = format!("<!doctype html><meta charset=utf-8><title>Letters</title><p style=\"font:16px system-ui;margin:3em\">{text}</p>");
    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect()
}

/// the form body of a token request.
pub fn token_request_body(client: &Client, fields: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(&str, &str)> = vec![("client_id", client.client_id.as_str())];
    if !client.client_secret.is_empty() {
        pairs.push(("client_secret", client.client_secret.as_str()));
    }
    pairs.extend_from_slice(fields);
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "an_hour")]
    expires_in: u64,
}

fn an_hour() -> u64 {
    3600
}

/// reads google's token answer into tokens, keeping an old refresh token
/// when a refresh answer has none (google sends it only the first time).
pub fn parse_token_answer(
    json: &str,
    previous_refresh: Option<String>,
    now: u64,
) -> Result<Tokens, String> {
    let answer: TokenAnswer = serde_json::from_str(json)
        .map_err(|error| format!("Google’s answer couldn’t be read ({error})."))?;
    Ok(Tokens {
        access_token: answer.access_token,
        refresh_token: answer.refresh_token.or(previous_refresh),
        expires_at: now + answer.expires_in,
    })
}

fn post_form(url: &str, body: &str) -> Result<String, String> {
    let mut response = ureq::post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.as_bytes())
        .map_err(|error| format!("Google couldn’t be reached or said no ({error})."))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

pub fn exchange_code(
    endpoints: &Endpoints,
    client: &Client,
    code: &str,
    verifier: &str,
    redirect: &str,
) -> Result<Tokens, String> {
    let body = token_request_body(
        client,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
        ],
    );
    parse_token_answer(&post_form(&endpoints.token, &body)?, None, now())
}

pub fn refresh(endpoints: &Endpoints, client: &Client, tokens: &Tokens) -> Result<Tokens, String> {
    let refresh_token = tokens
        .refresh_token
        .clone()
        .ok_or("Sign in to Google Drive again.")?;
    let body = token_request_body(
        client,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token),
        ],
    );
    parse_token_answer(
        &post_form(&endpoints.token, &body)?,
        Some(refresh_token),
        now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_address_asks_only_for_files_letters_makes() {
        let client = Client {
            client_id: "id 1".into(),
            client_secret: String::new(),
        };
        let url = authorization_url(
            &Endpoints::google(),
            &client,
            "http://127.0.0.1:5000",
            "abc",
            "xyz",
        );
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id=id%201"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file&"));
        assert!(url.contains("code_challenge=abc&code_challenge_method=S256&state=xyz"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000"));
    }

    #[test]
    fn tokens_are_refreshed_a_minute_early_and_keep_their_refresh_token() {
        let tokens = parse_token_answer(
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600}"#,
            None,
            1000,
        )
        .unwrap();
        assert_eq!(tokens.expires_at, 4600);
        assert!(!tokens.needs_refresh(4500));
        assert!(tokens.needs_refresh(4541));
        let refreshed = parse_token_answer(
            r#"{"access_token":"b","expires_in":10}"#,
            tokens.refresh_token.clone(),
            5000,
        )
        .unwrap();
        assert_eq!(refreshed.refresh_token.as_deref(), Some("r"));
    }

    #[test]
    fn token_requests_are_form_encoded_with_the_secret_only_when_set() {
        let client = Client {
            client_id: "c".into(),
            client_secret: String::new(),
        };
        assert_eq!(
            token_request_body(&client, &[("code", "a b/c")]),
            "client_id=c&code=a%20b%2Fc"
        );
        let with_secret = Client {
            client_id: "c".into(),
            client_secret: "s".into(),
        };
        assert!(token_request_body(&with_secret, &[]).ends_with("client_secret=s"));
    }

    #[test]
    fn the_loopback_takes_the_code_and_checks_the_state() {
        let loopback = Loopback::bind().unwrap();
        let address = loopback.redirect_uri().replace("http://", "");
        let browser = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(&address).unwrap();
            write!(
                stream,
                "GET /?state=s1&code=4%2Fabc HTTP/1.1\r\nHost: x\r\n\r\n"
            )
            .unwrap();
            let mut page = String::new();
            let _ = std::io::Read::read_to_string(&mut stream, &mut page);
            page
        });
        assert_eq!(
            loopback
                .wait_for_code("s1", Duration::from_secs(5))
                .unwrap(),
            "4/abc"
        );
        assert!(browser.join().unwrap().contains("You are signed in"));

        let loopback = Loopback::bind().unwrap();
        let address = loopback.redirect_uri().replace("http://", "");
        std::thread::spawn(move || {
            let mut stream = TcpStream::connect(&address).unwrap();
            write!(stream, "GET /?state=other&code=x HTTP/1.1\r\n\r\n").unwrap();
        });
        assert!(loopback
            .wait_for_code("s1", Duration::from_secs(5))
            .is_err());
    }
}
