//! the Settings window's choices, kept in one readable json file in the
//! app's data folder (`app-settings.json`).
//!
//! some settings already had a home before the Settings window existed,
//! and stay there so nothing is stored twice: the name on comments
//! (`settings.rs`), the writer's typing switches and the Google Drive
//! client id (`writer/settings.rs`), the sign-in and any ai key (the
//! system's credential store). this file holds the rest.
//!
//! secrets never go in it: the ai settings hold only the *name* the key
//! is stored under, and the type of the key can't be written as json.

mod model;
mod store;

pub use model::{AiServerLaunch, AppSettings, FileDefaults, General, Scanning, CURRENT_VERSION};
pub use store::SettingsStore;
