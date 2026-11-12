//! where the google sign-in is kept between runs: the system's own
//! credential store (Windows Credential Manager, the macOS keychain),
//! never a plain file. behind a small trait so tests use memory instead.

use std::sync::Mutex;

use crate::drive::oauth::Tokens;

pub trait TokenStore: Send + Sync {
    fn load(&self) -> Option<Tokens>;
    fn save(&self, tokens: &Tokens) -> Result<(), String>;
    fn clear(&self);
}

/// the system credential store, under one entry for Letters.
pub struct SystemStore {
    service: String,
}

impl SystemStore {
    pub fn new(service: &str) -> SystemStore {
        SystemStore { service: service.into() }
    }

    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, "google-drive").map_err(|error| error.to_string())
    }
}

impl TokenStore for SystemStore {
    fn load(&self) -> Option<Tokens> {
        let secret = self.entry().ok()?.get_password().ok()?;
        serde_json::from_str(&secret).ok()
    }

    fn save(&self, tokens: &Tokens) -> Result<(), String> {
        let text = serde_json::to_string(tokens).map_err(|error| error.to_string())?;
        self.entry()?.set_password(&text).map_err(|error| format!("The sign-in couldn’t be kept in the system’s credential store ({error})."))
    }

    fn clear(&self) {
        if let Ok(entry) = self.entry() {
            let _ = entry.delete_credential();
        }
    }
}

/// for tests: kept in memory only.
#[derive(Default)]
pub struct MemoryStore {
    tokens: Mutex<Option<Tokens>>,
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Option<Tokens> {
        self.tokens.lock().unwrap().clone()
    }

    fn save(&self, tokens: &Tokens) -> Result<(), String> {
        *self.tokens.lock().unwrap() = Some(tokens.clone());
        Ok(())
    }

    fn clear(&self) {
        *self.tokens.lock().unwrap() = None;
    }
}
