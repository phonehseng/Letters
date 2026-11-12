//! talking to model servers over http.
//!
//! both providers speak the same json: the common chat-completions format
//! that llama.cpp's server and many online services understand, with
//! answers streamed as server-sent events. so the format lives here once.
//!
//! - `chat_format.rs`: the json of requests, answers and errors
//! - `sse.rs`: reading a server-sent event stream line by line
//! - `http.rs`: sending requests, timeouts, and turning failures into
//!   sentences

pub mod chat_format;
pub mod http;
pub mod sse;
