//! optional document helpers (summaries, questions, find-by-meaning) through a provider the user chooses, off by default.
//!
//! this is the "AI tools" feature: summarise, list key points, answer a
//! question with checked quotes, and jump to where the document talks
//! about something. three promises shape the whole crate:
//!
//! - **off until chosen.** `Settings` start disabled with no provider, and
//!   nothing here contacts any server until the person turns the tools on
//!   and fills in a provider themselves.
//! - **told before it leaves.** every task is planned first; the plan's
//!   `Disclosure` says in plain english what would be sent and where.
//!   text that would leave this computer is only sent with the person's
//!   acknowledgement of exactly that disclosure, and the type system makes
//!   that the only way to send anything.
//! - **only what the document says.** answers are built from passages of
//!   the document, and every quote is checked against them.
//!
//! "find where" also works with no provider at all (`find_where_offline`).
//!
//! | module | job |
//! |---|---|
//! | `settings` | what the person chose; builds the provider |
//! | `provider` | the trait every provider implements, and the permit |
//! | `local_server` | a llama.cpp server, normally on this computer |
//! | `server_process` | starting and stopping that server |
//! | `remote_api` | an online service with the person's key |
//! | `secret` | the key type that never prints, and the key store hook |
//! | `address` | reading addresses and checking where they really lead |
//! | `disclosure` | what will be sent, as a sentence, and the yes to it |
//! | `document` | the text this crate is given |
//! | `chunking` | paragraphs into pieces and chunks |
//! | `tokens` | guessing token counts |
//! | `retrieval` | bm25, stemming and embedding similarity |
//! | `prompts` | every instruction, as plain text |
//! | `tasks` | summarise, key points, ask and find |
//! | `wire` | the http and json of the chat-completions format |

mod address;
mod cancel;
mod chunking;
mod disclosure;
mod document;
mod error;
mod local_server;
mod prompts;
mod provider;
mod remote_api;
mod retrieval;
mod secret;
mod server_process;
mod settings;
mod tasks;
mod tokens;
mod wire;

#[cfg(test)]
mod test_support;

pub use address::{is_loopback, Address, Resolver, SystemResolver};
pub use cancel::{CancelToken, Progress, Stage};
pub use disclosure::{
    describe_pages, describe_words, Acknowledgement, Disclosure, DisclosureId, QueryKind,
};
pub use document::{Document, Page, PageRange, Paragraph, Rect};
pub use error::AiError;
pub use local_server::LocalServer;
pub use prompts::DOESNT_SAY;
pub use provider::{
    ChatRequest, Completion, FinishReason, Permit, Provider, ProviderInfo, ProviderKind, Usage,
};
pub use remote_api::RemoteApi;
pub use retrieval::{stem, Bm25};
pub use secret::{ApiKey, KeyStore, MemoryKeyStore};
pub use server_process::{LaunchOptions, ServerProcess};
pub use settings::{
    LocalServerConfig, ProviderConfig, RemoteApiConfig, Settings, DEFAULT_KEY_NAME, THINKING_HELP,
};
pub use tasks::citations::{Answer, Citation};
pub use tasks::digest::{KeyPoint, Summary, SummaryLength};
pub use tasks::find::FoundPassage;
pub use tasks::{find_where_offline, AiTools, PlannedTask, TaskOutput, TaskRequest};
pub use tokens::TokenEstimator;
