//! what the Settings window stores, with a default for everything, so a
//! missing file, a missing section or a missing field all read as the
//! defaults. the ai tools start off.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// the layout of the file this version writes.
pub const CURRENT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub version: u32,
    pub general: General,
    pub files: FileDefaults,
    pub scanning: Scanning,
    /// the engine's own settings: the master switch (off) and the provider
    /// (none). the key itself is in the credential store
    pub ai: letters_ai::Settings,
    /// a model server Letters starts itself, if the person chose one
    pub ai_server: AiServerLaunch,
    /// anything a newer version of Letters wrote that this one doesn't
    /// know. it is kept, so opening Settings here never loses it
    #[serde(flatten)]
    pub unknown: Map<String, Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            version: CURRENT_VERSION,
            general: General::default(),
            files: FileDefaults::default(),
            scanning: Scanning::default(),
            ai: letters_ai::Settings::default(),
            ai_server: AiServerLaunch::default(),
            unknown: Map::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    /// "fitWidth", or a percentage such as "100"
    pub default_zoom: String,
    /// a document opened from Recents goes to a window of its own
    pub open_in_new_windows: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            default_zoom: "fitWidth".into(),
            open_in_new_windows: false,
        }
    }
}

/// the format each kind of file is saved or exported in unless chosen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileDefaults {
    /// "docx" or "odt" for new word processor documents
    pub document: String,
    /// "exactMatch" or "easyToEdit" for a pdf exported as a word document
    pub word_export_style: String,
    /// "png", "jpeg" or "tiff" for pages exported as pictures
    pub picture: String,
}

impl Default for FileDefaults {
    fn default() -> Self {
        FileDefaults {
            document: "docx".into(),
            word_export_style: "exactMatch".into(),
            picture: "png".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scanning {
    /// a text recognition language code, such as "eng"
    pub ocr_language: String,
}

impl Default for Scanning {
    fn default() -> Self {
        Scanning {
            ocr_language: "eng".into(),
        }
    }
}

/// the two files of a model server Letters can start on this computer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AiServerLaunch {
    pub executable: String,
    pub model_file: String,
}

impl AppSettings {
    /// reads a stored file of any version this one understands.
    ///
    /// version 0 is a file written before the version field existed: its
    /// sections are read as they are. a newer version's fields that this
    /// version doesn't know stay in `unknown` and are written back.
    pub fn from_json(value: Value) -> AppSettings {
        let mut settings: AppSettings = serde_json::from_value(value).unwrap_or_default();
        if settings.version < CURRENT_VERSION {
            settings.version = CURRENT_VERSION;
        }
        // a choice outside the lists the window offers falls back to the
        // default rather than leaving the window with nothing selected
        let files = FileDefaults::default();
        if !["docx", "odt"].contains(&settings.files.document.as_str()) {
            settings.files.document = files.document;
        }
        if !["exactMatch", "easyToEdit"].contains(&settings.files.word_export_style.as_str()) {
            settings.files.word_export_style = files.word_export_style;
        }
        if !["png", "jpeg", "tiff"].contains(&settings.files.picture.as_str()) {
            settings.files.picture = files.picture;
        }
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_starts_at_its_default_and_ai_starts_off() {
        let settings = AppSettings::from_json(serde_json::json!({}));
        assert_eq!(settings, AppSettings::default());
        assert!(!settings.ai.enabled);
        assert!(settings.ai.provider.is_none());
        assert_eq!(settings.general.default_zoom, "fitWidth");
    }

    #[test]
    fn an_old_file_is_read_and_brought_up_to_date() {
        let old = serde_json::json!({ "general": { "openInNewWindows": true } });
        let settings = AppSettings::from_json(old);
        assert_eq!(settings.version, CURRENT_VERSION);
        assert!(settings.general.open_in_new_windows);
        assert_eq!(settings.files.document, "docx");
    }

    #[test]
    fn a_newer_versions_fields_survive() {
        let newer = serde_json::json!({ "version": 7, "templates": { "folder": "C:/T" } });
        let settings = AppSettings::from_json(newer);
        assert_eq!(settings.version, 7);
        let written = serde_json::to_value(&settings).unwrap();
        assert_eq!(written["templates"]["folder"], "C:/T");
    }

    #[test]
    fn choices_outside_the_lists_fall_back() {
        let odd = serde_json::json!({ "files": { "document": "wpd", "picture": "jpeg" } });
        let settings = AppSettings::from_json(odd);
        assert_eq!(settings.files.document, "docx");
        assert_eq!(settings.files.picture, "jpeg");
    }

    #[test]
    fn a_damaged_section_means_the_defaults() {
        let damaged = serde_json::json!({ "general": "not a section" });
        assert_eq!(AppSettings::from_json(damaged), AppSettings::default());
    }
}
