//! compares two versions of a document and reports what changed in text, formatting and layout.
//!
//! how to use it:
//! 1. wrap each document in a `PageSource`. `LopdfSource` reads a pdf file;
//!    give it a renderer with `with_renderer` so pictures can be compared
//! 2. call `compare` (or `compare_documents` for the defaults). it returns
//!    a `ComparisonResult`: page pairs, their changes and a summary, ready
//!    to send to the ui as camelCase json
//! 3. for the overlay view, call `overlay_image` or `aligned_images` for
//!    the page pair on screen
//! 4. for the report, call `report_pdf`, `report_text` or `report_html`
//!
//! the rules every function follows:
//! - pages are counted from 0; sentences meant for people count from 1
//! - every rectangle is in points from the top left corner of the page as
//!   the person sees it, with y going down
//! - errors are sentences the ui can show as they are
//!
//! the modules, in the order the work happens:
//! - source, lopdf_source: what a document must provide, and a pdf reader
//! - words: letters into words, lines and blocks in reading order
//! - fingerprint, page_align: matching pages between the versions
//! - diff, text_diff, moves: text changes, and passages that moved
//! - formatting, layout: style and position changes of unchanged text
//! - pixel_diff, overlay: comparing and blending pictures of pages
//! - compare: the whole comparison, one page pair at a time
//! - result, sentences: what is found, and how it is worded
//! - report: the comparison report as pdf, text or html

mod compare;
mod diff;
mod error;
mod fingerprint;
mod formatting;
mod geometry;
mod hashing;
mod layout;
mod lopdf_source;
mod moves;
mod options;
mod overlay;
mod page_align;
mod pixel_diff;
mod report;
mod result;
mod sentences;
mod source;
mod standard_fonts;
mod text_diff;
mod words;

pub use compare::{compare, compare_documents};
pub use error::CompareError;
pub use geometry::{PageSize, Rect};
pub use lopdf_source::{LopdfSource, Renderer};
pub use options::{CompareOptions, Progress, Stage};
pub use overlay::{aligned_images, overlay_image, AlignedImages};
pub use report::{report_html, report_pdf, report_text, ReportOptions};
pub use result::{
    map_scroll, Category, Change, ChangeKind, ComparisonResult, Offset, PagePair, ScrollPoint,
    StyleChange, Summary, TextStyle,
};
pub use source::{Colour, Glyph, PageSource, RgbaImage};

/// one word of a page as the comparison reads it, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct PageWord {
    pub text: String,
    /// one rectangle, or two for a word hyphenated across lines
    pub rects: Vec<Rect>,
}

/// the words of a page in the order the comparison reads them. useful to
/// check reading order, and for the ui to show what was compared.
pub fn read_words(source: &dyn PageSource, page: u32) -> Result<Vec<PageWord>, CompareError> {
    let size = source.page_size(page)?;
    let glyphs = source.glyphs(page)?;
    let text = words::read_page_text(&glyphs, size, false);
    Ok(text
        .words
        .into_iter()
        .map(|word| PageWord {
            text: word.text,
            rects: word.rects,
        })
        .collect())
}
