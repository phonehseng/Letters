//! OCR (optical character recognition) reads the letters in a picture of
//! a page, such as a scan or a photo, and turns them into real text you
//! can search, select and copy.
//!
//! this crate does it fully offline, in four parts:
//! - `preprocess`: cleaning up the picture first (turning it upright,
//!   straightening it, removing specks, telling ink from paper)
//! - `engine`: running tesseract, the program that reads the letters
//! - `result` and `structure`: the words found, placed on the page as the
//!   person sees it, and gathered into paragraphs in reading order
//! - `text_layer` and `images_to_pdf`: writing the words into a pdf as an
//!   invisible layer over the picture, and making pdfs from pictures
//!
//! `is_scanned` tells the app when a page is worth offering this for.
//!
//! the shell's typical use, for a scanned page of an open document:
//!
//! ```ignore
//! let engine = Engine::locate(&tesseract_folder)?;
//! // the page drawn by the pdf renderer at 300 dpi, as rgba pixels
//! let picture = PageImage::from_rgba(width, height, &rgba, 300.0)?;
//! let words = recognize(&engine, &picture, &OcrOptions::default(), &cancel)?;
//! state.edit(id, |document| add_text_layer(document, page, &words))?;
//! ```

mod cancel;
mod engine;
mod error;
mod geometry;
mod hocr;
mod images_to_pdf;
mod languages;
mod metadata;
mod page_image;
mod pdf_page;
pub mod preprocess;
mod recognize;
mod result;
mod scanned;
mod structure;
mod text_layer;

pub use cancel::{CancelFlag, Progress};
pub use engine::Engine;
pub use error::OcrError;
pub use geometry::{Point, Rect};
pub use images_to_pdf::{images_to_pdf, ImageSource, ImagesToPdfOptions, PaperSize};
pub use languages::{language_name, BUNDLED_LANGUAGES};
pub use page_image::{PageImage, DEFAULT_DPI, MAX_MEGAPIXELS};
pub use preprocess::{AppliedStep, PreprocessOptions};
pub use recognize::{recognize, recognize_pages, OcrOptions};
pub use result::{Baseline, OcrPage, OcrWord};
pub use scanned::{is_scanned, page_makeup, PageMakeup};
pub use structure::{structured_text, StructuredText, TextParagraph};
pub use text_layer::{add_text_layer, has_text_layer, remove_text_layer, LAYER_TAG};
