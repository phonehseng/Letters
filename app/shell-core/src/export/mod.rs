//! conversion and export: the parts of the Export dialog that need no
//! window.
//!
//! | file | job |
//! |---|---|
//! | `request.rs` | what the dialog asks for (`ExportJob`) and gets back (`ExportOutcome`) |
//! | `choices.rs` | what this computer can do, from the convert engine's capabilities |
//! | `run.rs` | one export through the right engine, with progress and cancel |
//! | `easy_edit/` | "easy to edit" word and opendocument files from the pdf's structure |
//! | `exact.rs` | the adapter to the "exact match" word converter |
//! | `renderer.rs` | whole pages drawn by the app's pdf engine, for the converters |
//! | `summary.rs` | the result line, in plain words, from each engine's report |
//! | `office.rs` | office files to pdf through a program on this computer |

pub mod choices;
pub mod easy_edit;
pub mod exact;
pub mod office;
pub mod renderer;
pub mod request;
pub mod run;
pub mod summary;
#[cfg(test)]
pub mod test_pdfs;
