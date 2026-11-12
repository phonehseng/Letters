//! finds, searches, previews and installs fonts: system, custom, bundled
//! and an online library of open fonts.
//!
//! see README.md for the sources, the licences of every font offered,
//! what leaves the computer and when, and the limits.
//!
//! the app uses `FontLibrary` (library.rs). the pieces under it are pure
//! and tested on their own: reading files safely (container, sfnt, woff,
//! woff2), the facts of a face (facts, coverage), scanning with a cache
//! (scan), the index of families (index), search (search, fuzzy), look-
//! alikes (similar), substitutes and fallbacks (substitute), previews
//! (outline, raster, preview), embedding (subset), added fonts (custom),
//! the online catalogue (catalogue) and downloads (download).

pub mod catalogue;
pub mod container;
pub mod coverage;
pub mod custom;
pub mod download;
pub mod error;
pub mod facts;
pub mod fuzzy;
pub mod index;
pub mod library;
pub mod model;
pub mod outline;
pub mod preview;
pub mod raster;
pub mod scan;
pub mod search;
pub mod sfnt;
pub mod similar;
pub mod subset;
pub mod substitute;
mod woff;
mod woff2;

pub use catalogue::{Catalogue, CatalogueFamily, CatalogueFile};
pub use custom::{AddReport, Rejected};
pub use download::{DownloadProgress, Downloader, LIBRARY_ADDRESS, LIBRARY_HOST};
pub use error::FontError;
pub use index::{Family, FontIndex, Style};
pub use library::{
    DownloadState, FamilyDetails, FamilySummary, FontLibrary, LibraryPaths, LibraryStatus, Settings,
};
pub use model::{
    Axis, Category, Embedding, EmbeddingPermission, FaceFacts, FaceRecord, Measurements, Outlines,
    Source,
};
pub use preview::{preview_bitmap, preview_svg, Preview};
pub use raster::AlphaBitmap;
pub use scan::{ScanCache, ScanProgress};
pub use search::{Personal, Place, SearchQuery, SearchResults};
pub use similar::Similar;
pub use subset::{embedding_data, EmbedData, EmbedKind};
pub use substitute::{fallback_chain, fallback_for, MissingFont, Resolution, Substitution};
