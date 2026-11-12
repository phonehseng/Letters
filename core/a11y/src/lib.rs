//! checks a pdf for accessibility problems and fixes them.
//!
//! the checker (`check`) reads an open `lopdf::Document` and answers with
//! a `Report`: every finding explained in plain English (what it is, why
//! it matters to someone using a screen reader, what the fix does to the
//! file), where it is, and which fix applies. the fixes are small pure
//! functions on `&mut Document` that return what they changed, so the app
//! can run them through its usual edit pipeline with undo.
//!
//! conventions:
//! - pages are 0-based; messages count from 1, the way people do
//! - rectangles are in points, measured on the page as it is shown (after
//!   /CropBox and /Rotate) from its top left corner, with y going down
//! - structs cross to the ui as json with camelCase names
//! - an encrypted document is refused with a message asking to unlock it
//!
//! the words the ui reuses to explain the basics are the `*_EXPLAINED`
//! constants below.
//!
//! modules, by job:
//! - `rules`: the checker, one module per family of rules, and the
//!   catalog of every rule's wording
//! - `fixes`: the wizard's engine, one module per kind of fix
//! - `autotag`: building tags for an untagged document
//! - `content`: reads page drawing instructions (text, pictures, shapes,
//!   marked content); `rewrite` changes them
//! - `structure`: reads the tags; `tagwriter` adds and changes them
//! - `geometry`, `layout`, `colour`, `fonts`, `numtree`, `xmp`, `objects`:
//!   shared helpers
//! - `pdfua`: marking a document as pdf/ua once it passes
//! - `export`: the report as plain text and as accessible html

mod autotag;
mod colour;
mod content;
mod error;
mod export;
mod fixes;
mod fonts;
mod geometry;
mod layout;
mod numtree;
mod objects;
mod pdfua;
mod report;
mod rewrite;
mod rules;
mod structure;
mod tagwriter;
mod xmp;

pub use autotag::{auto_tag, AutoTagOptions, AutoTagResult, Confidence, TagCounts, TagDecision};
pub use colour::{contrast_ratio, nearest_passing_colour, required_ratio, Rgb};
pub use error::A11yError;
pub use export::{report_html, report_text};
pub use fixes::{
    add_bookmarks_from_headings, add_link_text, apply_text_colour, auto_reading_order,
    fix_heading_levels, mark_as_artifact, mark_as_tagged, propose_heading_levels, set_alt_text,
    set_annotation_contents, set_field_tooltip, set_heading_level, set_language, set_reading_order,
    set_tab_order, set_table_headers, set_title, show_title, tag_annotations, tag_untagged_content,
    HeadingChange, TableHeaders,
};
pub use geometry::Rect;
pub use objects::ObjectRef;
pub use pdfua::{mark_pdfua, PDFUA_HUMAN_CHECKS};
pub use report::{
    ContentRef, ContrastMeasurement, Finding, FixKind, FixResult, Location, Report, RuleOutcome,
    RuleStatus, Score, Severity, Summary, Target,
};
pub use rules::catalog::{Checked, FixMode, RuleInfo};
pub use rules::{check, check_with_options, CheckOptions, RegionRenderer, RgbaImage};

/// every rule the checker knows, with its wording, for the ui's
/// "what does Letters check" list.
pub fn rule_catalog() -> &'static [RuleInfo] {
    rules::catalog::RULES
}

/// what tags are, in one sentence for the ui.
pub const TAGS_EXPLAINED: &str = "Tags are a hidden outline of a PDF that tells screen readers what each part is (a heading, a paragraph, a list, a picture) and in what order to read them.";

/// what alt text is, in one sentence for the ui.
pub const ALT_TEXT_EXPLAINED: &str = "Alt text is a short written description of a picture that a screen reader reads aloud in its place, so people who can’t see the picture still get what it shows.";

/// what reading order is, in one sentence for the ui.
pub const READING_ORDER_EXPLAINED: &str = "Reading order is the order in which a screen reader reads the parts of a page, which comes from the tags, not from where things sit on the page.";

/// what pdf/ua is, in one sentence for the ui.
pub const PDF_UA_EXPLAINED: &str = "PDF/UA (ISO 14289) is the international standard for accessible PDFs: it lists what a file must contain so that screen readers and other assistive technology can use it.";
