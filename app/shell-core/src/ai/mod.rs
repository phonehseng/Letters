//! the optional ai tools' parts that need no window.
//!
//! off by default: nothing here is used until the person turns the tools
//! on in Settings and fills in a provider. "Find a Topic" works without
//! them, on this computer, with the engine's offline finder.
//!
//! | file | job |
//! |---|---|
//! | `document_text.rs` | the pdf's structure as the paragraphs-with-places the engine reads |
//! | `desk.rs` | plan, show, confirm, run: plans wait here for the person's Send |
//! | `consent.rs` | "Don’t ask again for this provider and document", one pair at a time |
//! | `keys.rs` | keys in the system's credential store, never in a file |
//! | `connection.rs` | Test Connection, with a fixed message and no document text |

pub mod connection;
pub mod consent;
pub mod desk;
pub mod document_text;
pub mod keys;
#[cfg(test)]
mod tests;
