//! the api key, kept out of logs, errors and settings files.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::AiError;

/// an api key the person typed in.
///
/// it prints as `***` in every format, has no serde support (so it can't
/// end up in a settings file by accident) and is only read back as text at
/// the one place that builds the authorization header.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// wraps a key, trimming the spaces and line breaks that copying from
    /// a web page often adds.
    pub fn new(key: impl Into<String>) -> Self {
        ApiKey(key.into().trim().to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// the key itself. only the http layer calls this.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// where keys live between runs.
///
/// the shell implements this with the operating system's credential store
/// (the windows credential manager, the macos keychain) so a key is never
/// written to a plain file. keys are looked up by a name the settings hold,
/// such as "letters-ai-remote".
pub trait KeyStore: Send + Sync {
    fn load(&self, name: &str) -> Result<Option<ApiKey>, AiError>;
    fn save(&self, name: &str, key: &ApiKey) -> Result<(), AiError>;
    fn delete(&self, name: &str) -> Result<(), AiError>;
}

/// a key store that forgets everything when the app quits.
///
/// useful for tests, and for a "don't remember this key" choice.
#[derive(Default)]
pub struct MemoryKeyStore {
    keys: Mutex<HashMap<String, ApiKey>>,
}

impl MemoryKeyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyStore for MemoryKeyStore {
    fn load(&self, name: &str) -> Result<Option<ApiKey>, AiError> {
        Ok(self.lock().get(name).cloned())
    }

    fn save(&self, name: &str, key: &ApiKey) -> Result<(), AiError> {
        self.lock().insert(name.to_string(), key.clone());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), AiError> {
        self.lock().remove(name);
        Ok(())
    }
}

impl MemoryKeyStore {
    // a poisoned lock only means another thread panicked mid-insert;
    // the map itself is still usable
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, ApiKey>> {
        self.keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_never_prints() {
        let key = ApiKey::new("  secret-123\n");
        assert_eq!(format!("{key}"), "***");
        assert_eq!(format!("{key:?}"), "***");
        assert_eq!(format!("{:?}", Some(key.clone())), "Some(***)");
        assert_eq!(key.expose(), "secret-123");
    }

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryKeyStore::new();
        assert_eq!(store.load("a").unwrap(), None);
        store.save("a", &ApiKey::new("k")).unwrap();
        assert_eq!(store.load("a").unwrap(), Some(ApiKey::new("k")));
        store.delete("a").unwrap();
        assert_eq!(store.load("a").unwrap(), None);
    }
}
