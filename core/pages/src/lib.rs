//! reorders, rotates, inserts, deletes, extracts, merges, splits and crops pages, and stamps page numbers, headers, footers and watermarks.
//!
//! every operation works on an open `lopdf::Document`. the app keeps a
//! working copy of each file and handles undo itself, so the rules are:
//! - each operation takes the document and one small settings struct
//!   (shaped so the ui can send it as json, with camelCase names)
//! - everything is checked before anything changes: an operation either
//!   succeeds completely or returns an error and leaves the document as it was
//! - page numbers in the api are 0-based positions in display order;
//!   error messages count from 1, the way people do
//!
//! the modules, by group of operations:
//! - `order`: reorder, move, delete
//! - `rotate`, `crop`: change how a page is shown
//! - `insert`: blank pages and pages from another document
//! - `combine`, `split`: make new documents
//! - `stamp`: page numbers, headers and footers, watermarks
//!
//! and the helpers underneath them:
//! - `tree`: the page tree, inherited settings, boxes and rotation
//! - `copy`: copying pages between documents without dragging extras along
//! - `links`, `outline`, `forms`, `cleanup`: keeping bookmarks, links and
//!   form fields valid
//! - `changes`: collecting changes so they are applied all at once

mod changes;
mod cleanup;
mod combine;
mod copy;
mod crop;
mod error;
mod forms;
mod insert;
mod links;
mod order;
mod outline;
mod rotate;
mod split;
mod stamp;
mod tree;

pub use combine::{extract, merge, ExtractParams, MergeParams};
pub use crop::{crop, uncrop, CropParams, CropRect, UncropParams};
pub use error::PagesError;
pub use insert::{insert_blank, insert_from, InsertBlankParams, InsertFromParams};
pub use order::{delete, move_pages, reorder, DeleteParams, MovePagesParams, ReorderParams};
pub use rotate::{rotate, RotateParams};
pub use split::{split, PageSpan, SplitParams, SplitPart};
pub use stamp::{
    add_header_footer, add_image_watermark, add_page_numbers, add_text_watermark, remove_stamps,
    HeaderFooterParams, HorizontalPlace, ImageWatermarkParams, NumberFormat, PageNumberParams,
    RemoveStampsParams, Rgb, StampFont, StampKind, StampLayer, StampPosition, TextWatermarkParams,
    VerticalEdge,
};
