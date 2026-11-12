//! code files: the parts of the code editor screen that need no window.
//!
//! - text_file: reading and saving plain text, keeping its encoding and line endings
//! - recovery: copies of unsaved code, offered back after a crash
//! - tools: finding compilers and vs code already installed
//! - runner and checks: syntax checks by those compilers, never running the program
//! - open_elsewhere: open in vs code, show in the file manager
//! - export: a document written as latex, rst, asciidoc, json or csv
//! - document_code and sources: finding code inside pdfs and word documents

pub mod checks;
pub mod document_code;
pub mod export;
pub mod open_elsewhere;
pub mod recovery;
pub mod runner;
pub mod sources;
pub mod text_file;
pub mod tools;
pub mod untitled;
