//! the shell's side of word-processor mode (.docx and .odt).
//!
//! - `routing`: which screen a file opens in, and the format a name saves in
//! - `document`: one open document, and the model json the ui edits
//! - `sessions`: every open document, opening, saving, autosave, export
//! - `save`: the safe save, and what odt can't keep
//! - `recovery`: recovery copies offered after a crash
//! - `settings`: the person's typing switches and spellcheck
//! - `engine`: the office engine's presets and ruler sentences
//! - `export`: pdf export through word or libreoffice
//! - `thumbnail`: the picture Recents shows for a word document

pub mod document;
pub mod engine;
pub mod export;
pub mod layout;
pub mod recovery;
pub mod routing;
pub mod save;
pub mod sessions;
pub mod settings;
pub mod thumbnail;

/// the two formats save writes, for the shell's commands
pub use letters_office::FileKind;
