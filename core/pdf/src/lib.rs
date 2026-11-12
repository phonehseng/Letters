//! opens pdf files and renders their pages as small png tiles.
//!
//! the whole design is about staying light on memory:
//! - a document is read lazily from disk, never loaded whole
//! - a page is only rendered when someone asks for one of its tiles
//! - finished tiles live in one cache with a fixed byte budget
//! - one worker thread owns the pdf library, so every window shares
//!   the same engine and the same cache
//!
//! it also reads a document's outline (its table of contents), searches
//! its text, and says where each character of a page is drawn.

mod cache;
mod engine;
mod error;
mod geometry;
mod outline;
mod page_text;
mod queue;
mod search;
mod viewport;
mod worker;

pub use cache::TileCache;
pub use error::PdfError;
pub use geometry::{page_pixel_size, tile_rect, PageSize, PixelRect, TileKey, TILE_SIZE};
pub use outline::OutlineItem;
pub use page_text::{PageText, TextChar, PAGE_TEXT_CACHE_PAGES};
pub use search::{SearchMatch, SearchResults, TextRect, MAX_SEARCH_MATCHES};
pub use viewport::Viewport;
pub use worker::{DocumentInfo, RenderWorker, WorkerStats};

/// identifies one open document inside the worker.
///
/// the ui only ever holds this number, never the document itself,
/// so closing a window can't leave a dangling pointer behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);
