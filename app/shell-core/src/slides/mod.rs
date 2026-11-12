//! the shell's side of presentation mode (.pptx and .odp): the slide
//! editor's open decks, their undo history, their pictures, saving,
//! recovery copies and export, all through the slides engine (core/slides).
//!
//! - `deck`: one open deck, its edits, undo and redo, and saving
//! - `view`, `view_text`: what the editor is shown of a deck and a slide
//! - `pictures`: each slide drawn by the engine into a pdf the app shows
//! - `new_objects`: shapes, text boxes, tables and pictures Insert adds
//! - `find`: Find and Replace across slides, tables and notes
//! - `recovery`: copies of unsaved decks, offered after a crash
//! - `sessions`: every deck open in any window

pub mod deck;
pub mod find;
pub mod new_objects;
pub mod pictures;
pub mod recovery;
pub mod sessions;
#[cfg(test)]
mod test_decks;
pub mod view;
pub mod view_text;

/// the engine's operations and pdf layouts, for the shell's commands
pub use letters_slides::edit::Operation;
pub use letters_slides::export::PdfLayout;
