//! where an online service's key is kept: the system's own credential
//! store (Windows Credential Manager, the macOS keychain), the same way
//! the Google Drive sign-in is kept (see drive/tokens.rs). never a file,
//! never a log line: the key travels as `letters_ai::ApiKey`, which
//! prints as `***`.

use letters_ai::{AiError, ApiKey, KeyStore};

/// the credential store, under one service name for Letters' ai keys.
pub struct SystemKeyStore {
    service: String,
}

impl SystemKeyStore {
    pub fn new(service: &str) -> SystemKeyStore {
        SystemKeyStore {
            service: service.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, AiError> {
        keyring::Entry::new(&self.service, name).map_err(|error| AiError::KeyStore(plain(&error)))
    }
}

impl KeyStore for SystemKeyStore {
    fn load(&self, name: &str) -> Result<Option<ApiKey>, AiError> {
        match self.entry(name)?.get_password() {
            Ok(secret) => Ok(Some(ApiKey::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AiError::KeyStore(plain(&error))),
        }
    }

    /// `ApiKey` keeps its text private to the ai engine, so a store
    /// outside that crate can't read a key back to save it. keys typed in
    /// Settings go through `save_text` instead, straight from the field to
    /// the store; the engine itself only ever loads.
    fn save(&self, _name: &str, _key: &ApiKey) -> Result<(), AiError> {
        Err(AiError::KeyStore("save the key from Settings".into()))
    }

    fn delete(&self, name: &str) -> Result<(), AiError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AiError::KeyStore(plain(&error))),
        }
    }
}

/// the store's own words, without anything that could hold the key.
fn plain(error: &keyring::Error) -> String {
    match error {
        keyring::Error::NoStorageAccess(_) => "the password store couldn’t be opened".into(),
        keyring::Error::PlatformFailure(_) => "the system refused".into(),
        _ => "it didn’t answer as expected".into(),
    }
}

/// somewhere a typed key can be written: the system store, or memory in
/// the tests.
pub trait TypedKeyStore: KeyStore {
    fn save_text(&self, name: &str, key: &str) -> Result<(), AiError>;
}

impl TypedKeyStore for SystemKeyStore {
    fn save_text(&self, name: &str, key: &str) -> Result<(), AiError> {
        self.entry(name)?
            .set_password(key)
            .map_err(|error| AiError::KeyStore(plain(&error)))
    }
}

impl TypedKeyStore for letters_ai::MemoryKeyStore {
    fn save_text(&self, name: &str, key: &str) -> Result<(), AiError> {
        self.save(name, &ApiKey::new(key))
    }
}

/// how a typed key reaches the store: trimmed the way `ApiKey` trims it,
/// and an empty field forgets the key. true when a key is now kept.
pub fn save_typed_key(store: &dyn TypedKeyStore, name: &str, typed: &str) -> Result<bool, AiError> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        store.delete(name)?;
        return Ok(false);
    }
    store.save_text(name, trimmed)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_ai::MemoryKeyStore;

    #[test]
    fn a_typed_key_is_trimmed_and_an_empty_one_forgets() {
        let store = MemoryKeyStore::new();
        assert!(save_typed_key(&store, "k", "  secret \n").unwrap());
        assert_eq!(store.load("k").unwrap(), Some(ApiKey::new("secret")));
        assert!(!save_typed_key(&store, "k", "   ").unwrap());
        assert_eq!(store.load("k").unwrap(), None);
    }
}
