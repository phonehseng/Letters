//! indexes every document the user chooses and searches them all at once, down to the page.
//!
//! - `normalize`, `stem`, `fuzzy`: words as the index sees them (folded,
//!   stemmed, and how far apart two spellings are)
//! - `query`: the search syntax ("phrases", -exclude, OR, prefixes)
//! - `postings`, `store`: the word lists and the files on disk
//! - `index`: adding, replacing and dropping documents, checkpoints
//! - `search`: ranking, grouping by document, snippets
//! - `extract`: the text of pdfs (through the app's engine), word
//!   documents and plain text, as passages
//! - `fingerprint`: telling a changed file from an unchanged one
//! - `jump`: the page finder ("p 12", "xii", "50%", "#methods")
//!
//! everything stays on this computer: the index is a folder of files in
//! the app's data folder and nothing is ever sent anywhere.
//!
//! the inverted index is written here rather than taken from a search
//! library: the app needs only a few hundred lines of it, it builds on
//! any toolchain, and it adds no weight to the program.

pub mod extract;
pub mod fingerprint;
pub mod fuzzy;
pub mod index;
pub mod jump;
pub mod model;
pub mod normalize;
pub mod postings;
pub mod query;
pub mod results;
pub mod search;
pub mod stem;
pub mod store;
// parts of the search, split by job: widening words, and cutting snippets
mod expand;
mod snippet;

pub use extract::{ExtractError, OutlineEntry, PdfText};
pub use fingerprint::fingerprint;
pub use index::{Index, IndexStats};
pub use jump::{resolve_jump, JumpCandidate, JumpDocument, JumpEntry, JumpKind};
pub use model::{DocKind, DocNote, DocRecord, ExtractedDocument, Fingerprint, Passage};
pub use results::{Correction, DocumentHit, Filters, PassageHit, SearchRequest, SearchResults};

/// what can go wrong with the index itself.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("The search index couldn\u{2019}t be read or written ({0}).")]
    Io(#[from] std::io::Error),
    #[error("The search index is damaged. Rebuild it in Settings.")]
    Damaged,
}
