//! what the person chose in Settings.
//!
//! off by default, with no provider: nothing in this crate talks to any
//! server until the person turns the tools on *and* fills in a provider
//! themselves. there are no built-in addresses or model names.
//!
//! the api key is not part of the settings. they hold only the key's name
//! in the key store, so saving settings to a file can't save the key.

use serde::{Deserialize, Serialize};

use crate::local_server::LocalServer;
use crate::provider::Provider;
use crate::remote_api::RemoteApi;
use crate::secret::KeyStore;
use crate::AiError;

/// the whole ai tools section of Settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// the master switch. false until the person turns it on.
    pub enabled: bool,
    /// where the model runs. none until the person chooses.
    pub provider: Option<ProviderConfig>,
}

/// the two kinds of provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProviderConfig {
    LocalServer(LocalServerConfig),
    RemoteApi(RemoteApiConfig),
}

/// a llama.cpp server (or another server with the same endpoints) that
/// the person runs, usually on this computer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalServerConfig {
    /// for example `http://127.0.0.1:8080`.
    pub address: String,
    /// the model name to ask for. a llama.cpp server runs one model and
    /// ignores this, so it may be left empty.
    pub model: Option<String>,
    /// how many tokens the model reads at once (the server's context size).
    pub context_tokens: usize,
    /// whether the server was started with embeddings turned on.
    pub embeddings: bool,
    /// room for thinking: extra answer tokens for a model that writes out
    /// its reasoning before it answers (see `THINKING_HELP`).
    pub thinking_tokens: usize,
    /// the longest wait between two pieces of an answer, in seconds.
    pub read_timeout_seconds: u64,
}

/// what the settings screen can say about room for thinking.
///
/// some models reason at length before they answer. that reasoning uses
/// up answer tokens, and with too few the model stops before it writes
/// anything. 0 suits models that answer straight away; 1000 to 4000 suits
/// most that think first.
pub const THINKING_HELP: &str =
    "Some models think before they answer. Give them extra room here if answers come back empty.";

impl Default for LocalServerConfig {
    fn default() -> Self {
        LocalServerConfig {
            address: String::new(),
            model: None,
            context_tokens: 4096,
            embeddings: false,
            thinking_tokens: 0,
            read_timeout_seconds: 300,
        }
    }
}

/// an online service reached with the person's own key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RemoteApiConfig {
    /// the service's base address, for example `https://api.example.com/v1`.
    /// requests go to `<address>/chat/completions`.
    pub address: String,
    /// the model name, exactly as the service spells it.
    pub model: String,
    /// a model for embeddings. leaving it empty keeps "find where" and
    /// question passages chosen on this computer, so less text is sent.
    pub embedding_model: Option<String>,
    pub context_tokens: usize,
    /// room for thinking, as for the local server.
    pub thinking_tokens: usize,
    /// the name the key is saved under in the key store.
    pub key_name: String,
    pub read_timeout_seconds: u64,
}

/// the key store name used when the settings don't say otherwise.
pub const DEFAULT_KEY_NAME: &str = "letters-ai-remote";

impl Default for RemoteApiConfig {
    fn default() -> Self {
        RemoteApiConfig {
            address: String::new(),
            model: String::new(),
            embedding_model: None,
            context_tokens: 8192,
            thinking_tokens: 0,
            key_name: DEFAULT_KEY_NAME.to_string(),
            read_timeout_seconds: 120,
        }
    }
}

impl Settings {
    /// the provider the person set up, ready to use.
    ///
    /// refuses when the tools are off or no provider is chosen, so a
    /// caller can't get a provider any other way than through a choice
    /// the person made.
    pub fn build_provider(&self, keys: &dyn KeyStore) -> Result<Box<dyn Provider>, AiError> {
        if !self.enabled {
            return Err(AiError::Disabled);
        }
        match self.provider.as_ref().ok_or(AiError::NoProvider)? {
            ProviderConfig::LocalServer(config) => Ok(Box::new(LocalServer::new(config)?)),
            ProviderConfig::RemoteApi(config) => {
                let key = keys.load(&config.key_name)?.ok_or(AiError::KeyMissing)?;
                Ok(Box::new(RemoteApi::new(config, key)?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{ApiKey, MemoryKeyStore};

    #[test]
    fn off_and_empty_by_default() {
        let settings = Settings::default();
        assert!(!settings.enabled);
        assert!(settings.provider.is_none());
        // a settings file without the section reads the same way
        let read: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(read, settings);
    }

    #[test]
    fn nothing_runs_until_turned_on_and_chosen() {
        let keys = MemoryKeyStore::new();
        let mut settings = Settings {
            enabled: false,
            provider: Some(ProviderConfig::LocalServer(LocalServerConfig {
                address: "http://127.0.0.1:9".into(),
                ..Default::default()
            })),
        };
        assert_eq!(
            settings.build_provider(&keys).err(),
            Some(AiError::Disabled)
        );
        settings.provider = None;
        settings.enabled = true;
        assert_eq!(
            settings.build_provider(&keys).err(),
            Some(AiError::NoProvider)
        );
    }

    #[test]
    fn an_online_service_needs_its_key() {
        let keys = MemoryKeyStore::new();
        let config = RemoteApiConfig {
            address: "https://api.example.com/v1".into(),
            model: "my-remote-model".into(),
            ..Default::default()
        };
        let settings = Settings {
            enabled: true,
            provider: Some(ProviderConfig::RemoteApi(config)),
        };
        assert_eq!(
            settings.build_provider(&keys).err(),
            Some(AiError::KeyMissing)
        );
        keys.save(DEFAULT_KEY_NAME, &ApiKey::new("test-key"))
            .unwrap();
        let provider = settings.build_provider(&keys).unwrap();
        assert!(provider.describe().sends_content_off_device);
        assert_eq!(provider.describe().host, "api.example.com");
    }

    #[test]
    fn settings_never_hold_the_key() {
        let settings = Settings {
            enabled: true,
            provider: Some(ProviderConfig::RemoteApi(RemoteApiConfig::default())),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"keyName\":\"letters-ai-remote\""));
        assert!(json.contains("\"kind\":\"remoteApi\""));
    }
}
