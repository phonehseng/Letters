//! the person's settings that outlive a window, kept in one small file.
//!
//! today there is one: the name written on comments and marks as their
//! author. it starts as the name the person signs in to the computer
//! with, and can be changed in the Annotate tools.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// a name longer than this is cut. pdf readers show the author in a
/// small label, and a whole paragraph there helps nobody.
pub const MAX_AUTHOR_NAME_CHARS: usize = 64;

/// what an annotation says when even the computer has no user name.
const FALLBACK_AUTHOR: &str = "Letters user";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    /// none means "use the computer's user name"
    #[serde(default)]
    author_name: Option<String>,
}

pub struct Settings {
    store: PathBuf,
    stored: StoredSettings,
}

impl Settings {
    /// reads the saved settings. a missing or damaged file just means
    /// the defaults: losing a setting is never worth an error.
    pub fn load(store: PathBuf) -> Self {
        let stored = fs::read_to_string(&store)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { store, stored }
    }

    /// the author name for new comments: the one the person chose, or
    /// else `system_name` (the computer's user name).
    pub fn author_name(&self, system_name: Option<&str>) -> String {
        self.stored
            .author_name
            .clone()
            .or_else(|| system_name.and_then(clean_author_name))
            .unwrap_or_else(|| FALLBACK_AUTHOR.to_owned())
    }

    /// remembers the name. a blank name goes back to the computer's user
    /// name. returns the name now in use.
    pub fn set_author_name(&mut self, name: &str, system_name: Option<&str>) -> String {
        self.stored.author_name = clean_author_name(name);
        self.save();
        self.author_name(system_name)
    }

    fn save(&self) {
        if let Some(folder) = self.store.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.stored) {
            let _ = fs::write(&self.store, text);
        }
    }
}

/// tidies a typed name: no space at either end, no line breaks or runs
/// of spaces inside, and not too long. none when nothing is left.
pub fn clean_author_name(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    let joined: String = words
        .join(" ")
        .chars()
        .take(MAX_AUTHOR_NAME_CHARS)
        .collect();
    let trimmed = joined.trim_end();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// the name the person signs in to this computer with, if the system
/// says. windows calls it USERNAME, macos and linux USER.
pub fn system_user_name() -> Option<String> {
    ["USERNAME", "USER"]
        .iter()
        .find_map(|variable| std::env::var(variable).ok())
        .and_then(|name| clean_author_name(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("letters-shell-tests")
            .join(format!("{test_name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn the_author_starts_as_the_computer_s_user_name() {
        let settings = Settings::load(temp_store("author-default"));
        assert_eq!(settings.author_name(Some("sam")), "sam");
    }

    #[test]
    fn without_any_name_the_author_is_still_named() {
        let settings = Settings::load(temp_store("author-none"));
        assert_eq!(settings.author_name(None), FALLBACK_AUTHOR);
        assert_eq!(settings.author_name(Some("   ")), FALLBACK_AUTHOR);
    }

    #[test]
    fn a_chosen_name_wins_and_survives_a_restart() {
        let store = temp_store("author-chosen");
        let mut settings = Settings::load(store.clone());
        assert_eq!(
            settings.set_author_name("  Sam   Lee ", Some("sam")),
            "Sam Lee"
        );
        assert_eq!(Settings::load(store).author_name(Some("sam")), "Sam Lee");
    }

    #[test]
    fn a_blank_name_goes_back_to_the_computer_s_name() {
        let mut settings = Settings::load(temp_store("author-blank"));
        settings.set_author_name("Sam Lee", Some("sam"));
        assert_eq!(settings.set_author_name("  ", Some("sam")), "sam");
    }

    #[test]
    fn names_are_tidied_and_cut() {
        assert_eq!(clean_author_name("Sam\nLee\t"), Some("Sam Lee".to_owned()));
        assert_eq!(clean_author_name(""), None);
        let long = "a".repeat(200);
        assert_eq!(
            clean_author_name(&long).unwrap().chars().count(),
            MAX_AUTHOR_NAME_CHARS
        );
    }

    #[test]
    fn a_damaged_settings_file_means_the_defaults() {
        let store = temp_store("settings-damaged");
        fs::create_dir_all(store.parent().unwrap()).unwrap();
        fs::write(&store, "not json").unwrap();
        assert_eq!(Settings::load(store).author_name(Some("sam")), "sam");
    }
}
