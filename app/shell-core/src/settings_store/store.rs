//! reading and writing `app-settings.json`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::model::AppSettings;

/// the settings file, read once and kept in memory, written on each change.
pub struct SettingsStore {
    file: PathBuf,
    current: Mutex<AppSettings>,
}

impl SettingsStore {
    /// a missing or damaged file gives the defaults: losing a setting is
    /// never worth an error.
    pub fn load(file: PathBuf) -> SettingsStore {
        let current = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .map(AppSettings::from_json)
            .unwrap_or_default();
        SettingsStore {
            file,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> AppSettings {
        self.lock().clone()
    }

    /// keeps `settings` and writes the file (pretty printed, so a person
    /// can read it). returns what is now stored.
    pub fn set(&self, settings: AppSettings) -> Result<AppSettings, String> {
        let settings = AppSettings::from_json(
            serde_json::to_value(&settings).map_err(|error| error.to_string())?,
        );
        let text = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
        if let Some(folder) = self.file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        crate::writer::save::write_atomically(text.as_bytes(), &self.file)?;
        *self.lock() = settings.clone();
        Ok(settings)
    }

    /// changes one part of the settings.
    pub fn update(&self, change: impl FnOnce(&mut AppSettings)) -> Result<AppSettings, String> {
        let mut settings = self.get();
        change(&mut settings);
        self.set(settings)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AppSettings> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::keys::save_typed_key;
    use letters_ai::{MemoryKeyStore, ProviderConfig, RemoteApiConfig};

    fn file(name: &str) -> PathBuf {
        crate::test_pdf::temp_dir(name).join("app-settings.json")
    }

    #[test]
    fn settings_survive_a_restart() {
        let path = file("settings-store-restart");
        let store = SettingsStore::load(path.clone());
        store
            .update(|settings| settings.general.open_in_new_windows = true)
            .unwrap();
        let again = SettingsStore::load(path.clone());
        assert!(again.get().general.open_in_new_windows);
        // readable json, as promised
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\n  \"general\": {"), "{text}");
    }

    #[test]
    fn the_key_is_never_written_to_the_file() {
        let path = file("settings-store-secret");
        let store = SettingsStore::load(path.clone());
        let keys = MemoryKeyStore::new();
        let secret = "sk-very-secret-value-1234";
        save_typed_key(&keys, "letters-ai-remote", secret).unwrap();
        store
            .update(|settings| {
                settings.ai.enabled = true;
                settings.ai.provider = Some(ProviderConfig::RemoteApi(RemoteApiConfig {
                    address: "https://api.example.com/v1".into(),
                    model: "my-remote-model".into(),
                    ..RemoteApiConfig::default()
                }));
            })
            .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(secret));
        assert!(text.contains("\"keyName\": \"letters-ai-remote\""));
        // and the tools still find the key where it belongs
        let tools = letters_ai::AiTools::from_settings(&store.get().ai, &keys).unwrap();
        assert!(tools.provider_info().sends_content_off_device);
    }

    #[test]
    fn a_damaged_file_gives_the_defaults() {
        let path = file("settings-store-damaged");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(SettingsStore::load(path).get(), AppSettings::default());
    }
}
