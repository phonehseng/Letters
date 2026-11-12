//! edits page content directly: text in place and images.

mod add_text;
mod color_ops;
mod colors;
mod draft;
mod edit_run;
mod error;
mod fonts;
mod geometry;
mod image_data;
mod image_tools;
mod images;
mod interpret;
mod layout;
mod lexer;
mod model;
mod moving;
mod page_boxes;
mod page_space;
mod pages;
mod path_read;
mod paths;
mod pdf_values;
mod reflow;
mod rewrite;
mod same_font;
mod scan;
mod separators;
mod structure;
mod summary;
mod text_edit;
mod text_fonts;
mod text_write;
mod writer;

use lopdf::Document;

pub use add_text::{add_text, NewText, NewTextFont};
pub use colors::Color;
pub use error::EditError;
pub use fonts::completion::{with_font_donors, FontDonors};
pub use fonts::pdf_font::{FontFamily, FontKind};
pub use fonts::standard::StandardFont;
pub use geometry::{Point, Rect};
pub use image_data::ExtractedImage;
pub use image_tools::{
    crop_image, crop_image_pixels, flip_image, set_image_opacity, FlipDirection,
};
pub use images::{
    add_image, bring_image_to_front, delete_image, extract_image, move_image, replace_image,
    resize_image, rotate_image, send_image_to_back, ImageEditResult, ImageFit,
};
pub use model::{
    FontInfo, GroupReason, ImageObject, PageObjects, PaintInfo, PathGroup, PathObject, PathShape,
    StrokeInfo, TextAlignment, TextBlock, TextLine, TextRun,
};
pub use page_boxes::{
    add_page_space, page_boxes, page_margins, resize_page, set_page_boxes, set_page_margins,
    MarginFit, Margins, PageBoxes, PageChangeResult, PageMargins, PaperFit, Side,
};
pub use paths::{
    bring_paths_to_front, delete_paths, duplicate_paths, move_paths, resize_paths, rotate_paths,
    send_paths_to_back, set_path_points, set_path_style, NewPaint, PathStyle,
};
pub use reflow::{
    add_text_matching, edit_table_cell, reflow_paragraph, LimitReason, MatchedText, OverflowChoice,
    OverflowOption, OverflowReport, ReflowOptions, ReflowResult,
};
pub use same_font::extend_font;
pub use scan::{
    convert_scan_to_text, ScanConversion, ScanLine, ScanMode, ScanOptions, ScanParagraph, ScanStyle,
};
pub use structure::{
    content_edit_check, marked_content_balanced, marked_content_ids, EditCheck, SignatureInfo,
};
pub use summary::{ChangesSummary, ContentEditResult, EditConsent, FontExtension, Tightening};
pub use text_edit::{
    delete_chars, delete_text, insert_text, replace_text, set_text_style, TextEditOptions,
    TextEditResult, TextOverflow, TextStyleChange,
};
pub use text_fonts::{FallbackKind, FontNotice};

/// reads a page as editable objects: text blocks (with lines, runs and
/// fonts) and images. changes nothing.
///
/// the ids in the result are only valid until the next edit of the page;
/// every edit returns the page's objects again with fresh ids.
pub fn page_objects(doc: &Document, page: usize) -> Result<PageObjects, EditError> {
    Ok(layout::build(doc, page)?.objects())
}
