//! the shell's side of word-processor mode (.docx and .odt).
//!
//! - `routing`: which screen a file opens in, and the format a name saves in
//! - `document`: one open document, and the model json the ui edits
//! - `sessions`: every open document, opening, saving, autosave, export
//! - `save`: the safe save, and what odt can't keep
//! - `recovery`: recovery copies offered after a crash
//! - `settings`: the person's typing switches and spellcheck
//! - `engine`: the office engine's presets and ruler sentences
//! - `export`: pdf export and print-ready pdfs through the engine's bridge
//! - `layout`: word's own lines and pages, through the engine's bridge
//! - `engine_layout`: letters' own lines and pages (core/layout), behind a setting
//! - `objects`: text blocks, free pictures, stacking, aligning, word's fixes
//! - `clipboard`: copy with picture bytes, paste of pictures and html
//! - `thumbnail`: the picture Recents shows for a word document

pub mod clipboard;
pub mod document;
pub mod engine;
pub mod engine_layout;
pub mod export;
pub mod layout;
pub mod objects;
#[cfg(test)]
mod proof_documents;
pub mod recovery;
pub mod routing;
pub mod save;
pub mod sessions;
pub mod settings;
pub mod thumbnail;

/// every format save as offers, with the engine's sentence on what each keeps
pub use letters_office::formats::{format_capabilities, FormatCapability};
/// the two formats save writes, for the shell's commands
pub use letters_office::FileKind;
