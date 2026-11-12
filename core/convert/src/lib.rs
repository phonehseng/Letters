//! converts between pdf and other formats: images, text, html,
//! spreadsheets, slides and office documents.
//!
//! the parts, in the order a pdf goes through them:
//! - `source`: loading the pdf and choosing pages
//! - `render`: the page drawing trait the app supplies
//! - `images`: pages to png, jpeg and tiff
//! - `content`: reading the vector drawing of a page (lines, boxes,
//!   pictures), and making copies without text for backgrounds
//! - `structure`: finding paragraphs, headings, lists, tables, figures,
//!   repeated headers and footers, in reading order (the model the "easy
//!   to edit" writers use, documented in `structure::model`)
//! - `text`, `html`, `xlsx`, `pptx`: the writers
//! - `images_to_pdf`: pictures to pdf, through letters-ocr
//! - `office`: office documents to pdf through programs on this computer
//! - `capabilities`: what can be converted to what, in plain words
//!
//! every conversion takes a `CancelFlag` (checked between pages) and a
//! progress callback, and every error is a sentence for the person using
//! the app (`ConvertError`).

mod capabilities;
mod content;
mod error;
pub mod geometry;
mod html;
mod images;
mod images_to_pdf;
mod naming;
mod office;
mod package;
mod pptx;
mod render;
mod source;
pub mod structure;
#[cfg(test)]
mod test_pdf;
mod text;
mod xlsx;

pub use capabilities::{
    capabilities, Capability, ExportStyle, Format, EASY_TO_EDIT_EXPLAINED, EXACT_MATCH_EXPLAINED,
};
pub use content::Rgb;
pub use error::ConvertError;
pub use geometry::Rect;
pub use html::{pdf_to_html, HtmlExportOptions, HtmlMode, HtmlReport};
pub use images::{pdf_to_images, ColorMode, ImageExportOptions, ImageFormat, TiffCompression};
pub use images_to_pdf::images_to_pdf_file;
pub use letters_ocr::{CancelFlag, Engine, ImagesToPdfOptions, OcrOptions, PaperSize, Progress};
pub use office::{
    find_office_converters, office_to_pdf, OfficeConversion, OfficeConverter, OfficeConverters,
    OfficeFormat, OfficeKind, OfficeOptions,
};
pub use pptx::{pdf_to_pptx, PptxExportOptions, PptxReport, SlideReport};
pub use render::{scale_for_dpi, PageRenderer, RgbaImage, MAX_MEGAPIXELS, MAX_SIDE_PIXELS};
pub use source::{PageSelection, PageSpace, PdfSource};
pub use structure::model::*;
pub use structure::{analyze_structure, StructureOptions};
pub use text::{pdf_to_text, write_text_file, PageSeparator, TextExportOptions, TextMode};
pub use xlsx::{pdf_to_xlsx, SheetLayout, XlsxExportOptions, XlsxReport};
