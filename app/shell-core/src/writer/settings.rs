//! the person's typing settings for the writer: the engine's typing
//! switches (smart quotes, autocorrect, capitals) and spellcheck. one file
//! per person, so the status bar and the inspector always agree, in every
//! window and after a restart.

use std::fs;
use std::path::PathBuf;

use letters_office::typing::TypingSettings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterSettings {
    #[serde(default)]
    pub typing: TypingSettings,
    #[serde(default = "on")]
    pub spellcheck: bool,
    /// "free" (every Letters feature) or "strict" (only what Word keeps exactly)
    #[serde(default = "free")]
    pub mode: String,
    /// the google oauth client for direct drive uploads; empty means
    /// only the Google Drive folder is offered
    #[serde(default)]
    pub drive_client_id: String,
    #[serde(default)]
    pub drive_client_secret: String,
}

fn free() -> String {
    "free".into()
}

fn on() -> bool {
    true
}

impl Default for WriterSettings {
    fn default() -> Self {
        WriterSettings {
            typing: TypingSettings::default(),
            spellcheck: true,
            mode: free(),
            drive_client_id: String::new(),
            drive_client_secret: String::new(),
        }
    }
}

pub struct WriterSettingsStore {
    file: PathBuf,
}

impl WriterSettingsStore {
    pub fn new(file: PathBuf) -> WriterSettingsStore {
        WriterSettingsStore { file }
    }

    /// the saved settings; a missing or damaged file gives the defaults
    /// (everything on), which is never worth an error.
    pub fn load(&self) -> WriterSettings {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, settings: &WriterSettings) {
        if let Some(folder) = self.file.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(text) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(&self.file, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_restart_and_start_all_on() {
        let dir = std::env::temp_dir()
            .join("letters-writer-tests")
            .join(format!("settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = WriterSettingsStore::new(dir.join("writer-settings.json"));
        let mut settings = store.load();
        assert!(settings.spellcheck && settings.typing.smart_quotes);
        settings.spellcheck = false;
        settings.typing.smart_quotes = false;
        store.save(&settings);
        let again = WriterSettingsStore::new(dir.join("writer-settings.json")).load();
        assert_eq!(again, settings);
    }
}
