//! saving to google drive.
//!
//! - `local`: Google Drive for desktop's folder, a plain save (no sign-in)
//! - `oauth`, `pkce`: signing in through the system browser
//! - `upload`: one file to drive, optionally as a google doc
//! - `tokens`: the sign-in kept in the system credential store
//!
//! direct upload needs an oauth client id registered by the app's
//! publisher; Letters reads it from Settings and never ships one.
//! nothing is uploaded unless the person chooses it each time.

pub mod local;
pub mod oauth;
pub mod pkce;
pub mod tokens;
pub mod upload;

use std::time::Duration;

use oauth::{Client, Endpoints, Loopback, Tokens};
use tokens::TokenStore;
use upload::{UploadRequest, Uploaded};

pub struct Drive<'a> {
    pub endpoints: Endpoints,
    pub client: Client,
    pub store: &'a dyn TokenStore,
}

impl Drive<'_> {
    pub fn signed_in(&self) -> bool {
        self.store.load().is_some()
    }

    /// signs in through the browser: `open` is given google's page to show.
    pub fn sign_in(
        &self,
        open: impl FnOnce(&str) -> Result<(), String>,
        timeout: Duration,
    ) -> Result<(), String> {
        if self.client.client_id.trim().is_empty() {
            return Err("Direct upload needs a Google OAuth client ID. Add one in Settings, or save to the Google Drive folder instead.".into());
        }
        let loopback = Loopback::bind()?;
        let redirect = loopback.redirect_uri();
        let verifier = pkce::new_verifier()?;
        let state = pkce::new_state()?;
        open(&oauth::authorization_url(
            &self.endpoints,
            &self.client,
            &redirect,
            &pkce::challenge(&verifier),
            &state,
        ))?;
        let code = loopback.wait_for_code(&state, timeout)?;
        let tokens =
            oauth::exchange_code(&self.endpoints, &self.client, &code, &verifier, &redirect)?;
        self.store.save(&tokens)
    }

    pub fn sign_out(&self) {
        self.store.clear();
    }

    /// a token that is good for the next few minutes, refreshed if needed.
    fn fresh_tokens(&self) -> Result<Tokens, String> {
        let tokens = self.store.load().ok_or("Sign in to Google Drive first.")?;
        if !tokens.needs_refresh(oauth::now()) {
            return Ok(tokens);
        }
        let refreshed = oauth::refresh(&self.endpoints, &self.client, &tokens)?;
        self.store.save(&refreshed)?;
        Ok(refreshed)
    }

    /// uploads one file, into a folder of that name when one is given.
    pub fn upload(
        &self,
        mut request: UploadRequest,
        folder_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Uploaded, String> {
        let tokens = self.fresh_tokens()?;
        if let Some(name) = folder_name.filter(|name| !name.trim().is_empty()) {
            request.folder_id = Some(upload::find_or_create_folder(
                &self.endpoints,
                &tokens.access_token,
                name.trim(),
            )?);
        }
        upload::upload(&self.endpoints, &tokens.access_token, &request, bytes)
    }
}

/// the content type of a file Letters uploads, by extension.
pub fn content_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "pdf" => "application/pdf",
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod fake_google;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use tokens::MemoryStore;

    /// plays the browser: follows google's page to the redirect with a code.
    fn browser(url: &str, approve: bool) -> Result<(), String> {
        let query = url.split_once('?').unwrap().1;
        let parameters = oauth::parse_query(query);
        let value = |name: &str| {
            parameters
                .iter()
                .find(|(key, _)| key == name)
                .unwrap()
                .1
                .clone()
        };
        let redirect = value("redirect_uri").replace("http://", "");
        let answer = if approve {
            format!(
                "/?state={}&code=code-for-{}",
                value("state"),
                value("code_challenge")
            )
        } else {
            format!("/?state={}&error=access_denied", value("state"))
        };
        std::thread::spawn(move || {
            let mut stream = TcpStream::connect(redirect).unwrap();
            write!(stream, "GET {answer} HTTP/1.1\r\n\r\n").unwrap();
            let mut page = String::new();
            let _ = stream.read_to_string(&mut page);
        });
        Ok(())
    }

    fn client() -> Client {
        Client {
            client_id: "test-client".into(),
            client_secret: String::new(),
        }
    }

    #[test]
    fn sign_in_upload_as_a_google_doc_into_a_folder_and_sign_out() {
        let server = fake_google::FakeGoogle::start();
        let store = MemoryStore::default();
        let drive = Drive {
            endpoints: Endpoints::local(&server.base),
            client: client(),
            store: &store,
        };
        assert!(!drive.signed_in());
        drive
            .sign_in(|url| browser(url, true), Duration::from_secs(5))
            .unwrap();
        assert!(drive.signed_in());
        // the token request proved the pkce secret (the fake checks it)
        assert_eq!(
            server
                .log()
                .iter()
                .filter(|line| line.starts_with("token authorization_code ok"))
                .count(),
            1
        );

        let request = UploadRequest {
            name: "Essay.docx".into(),
            content_type: content_type("Essay.docx").into(),
            as_google_doc: true,
            folder_id: None,
        };
        let uploaded = drive
            .upload(request, Some("Letters"), b"PK docx bytes")
            .unwrap();
        assert_eq!(
            uploaded.web_view_link.as_deref(),
            Some("https://docs.google.com/document/d/file-1/edit")
        );
        let log = server.log();
        assert!(
            log.iter().any(|line| line == "create folder Letters"),
            "{log:?}"
        );
        assert!(
            log.iter()
                .any(|line| line.contains("upload google-doc parents=folder-1 bytes=13")),
            "{log:?}"
        );

        // the folder is found the second time, not made again
        let pdf = UploadRequest {
            name: "Essay.pdf".into(),
            content_type: content_type("Essay.pdf").into(),
            as_google_doc: false,
            folder_id: None,
        };
        drive.upload(pdf, Some("Letters"), b"%PDF").unwrap();
        assert_eq!(
            server
                .log()
                .iter()
                .filter(|line| line.starts_with("create folder"))
                .count(),
            1
        );

        drive.sign_out();
        assert!(!drive.signed_in());
    }

    #[test]
    fn an_expired_token_is_refreshed_before_uploading() {
        let server = fake_google::FakeGoogle::start();
        let store = MemoryStore::default();
        store
            .save(&Tokens {
                access_token: "old".into(),
                refresh_token: Some("refresh-1".into()),
                expires_at: 0,
            })
            .unwrap();
        let drive = Drive {
            endpoints: Endpoints::local(&server.base),
            client: client(),
            store: &store,
        };
        let request = UploadRequest {
            name: "a.pdf".into(),
            content_type: "application/pdf".into(),
            as_google_doc: false,
            folder_id: None,
        };
        drive.upload(request, None, b"%PDF").unwrap();
        assert!(server
            .log()
            .iter()
            .any(|line| line == "token refresh_token ok"));
        let tokens = store.load().unwrap();
        assert_eq!(tokens.access_token, "access-refreshed");
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-1"));
    }

    #[test]
    fn a_refused_sign_in_and_a_missing_client_id_say_so() {
        let server = fake_google::FakeGoogle::start();
        let store = MemoryStore::default();
        let drive = Drive {
            endpoints: Endpoints::local(&server.base),
            client: client(),
            store: &store,
        };
        let error = drive
            .sign_in(|url| browser(url, false), Duration::from_secs(5))
            .unwrap_err();
        assert!(error.contains("access_denied"), "{error}");
        let no_id = Drive {
            endpoints: Endpoints::local(&server.base),
            client: Client::default(),
            store: &store,
        };
        assert!(no_id
            .sign_in(|_| Ok(()), Duration::from_secs(1))
            .unwrap_err()
            .contains("client ID"));
        assert!(drive
            .upload(
                UploadRequest {
                    name: "x".into(),
                    content_type: "x".into(),
                    as_google_doc: false,
                    folder_id: None
                },
                None,
                b""
            )
            .unwrap_err()
            .contains("Sign in"));
    }
}
