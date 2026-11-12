//! edits page content directly: text in place and images.

mod add_text;
mod colors;
mod draft;
mod error;
mod fonts;
mod geometry;
mod image_data;
mod images;
mod interpret;
mod layout;
mod lexer;
mod model;
mod page_space;
mod pages;
mod pdf_values;
mod rewrite;
mod text_edit;
mod text_fonts;
mod text_write;
mod writer;

use lopdf::Document;

pub use add_text::{add_text, NewText, NewTextFont};
pub use colors::Color;
pub use error::EditError;
pub use fonts::pdf_font::{FontFamily, FontKind};
pub use fonts::standard::StandardFont;
pub use geometry::{Point, Rect};
pub use image_data::ExtractedImage;
pub use images::{
    add_image, bring_image_to_front, delete_image, extract_image, move_image, replace_image,
    resize_image, rotate_image, send_image_to_back, ImageEditResult, ImageFit,
};
pub use model::{FontInfo, ImageObject, PageObjects, TextAlignment, TextBlock, TextLine, TextRun};
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
