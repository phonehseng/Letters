//! reading a pdf for conversion: glyphs, paths and pictures per page.

pub mod content;
pub mod encoding;
pub mod fonts;
pub mod images;
pub mod lexer;
pub mod matrix;
pub mod objects;
mod tables;
pub mod to_unicode;

use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};

pub use content::{Glyph, ImageItem, PageContent, PathItem, Segment};

use crate::ExactError;
use objects::{get, get_dict, numbers, resolve, stream_bytes};

/// an open pdf.
pub struct PdfFile {
    pub document: Document,
    pub bytes_len: u64,
}

impl PdfFile {
    /// opens a pdf, refusing locked ones (recipe: encrypted input gives the
    /// "unlock first" error).
    pub fn open(path: &Path) -> Result<Self, ExactError> {
        let bytes = std::fs::read(path).map_err(|e| ExactError::CannotOpen(format!("({e})")))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ExactError> {
        if !bytes.starts_with(b"%PDF") && !bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
            return Err(ExactError::CannotOpen("It isn’t a PDF.".into()));
        }
        let document = match Document::load_mem(bytes) {
            Ok(document) => document,
            Err(error) => {
                let text = error.to_string().to_lowercase();
                if text.contains("encrypt") || text.contains("password") || text.contains("decrypt")
                {
                    return Err(ExactError::Locked);
                }
                return Err(ExactError::CannotOpen(format!(
                    "It may be damaged. ({error})"
                )));
            }
        };
        if document.is_encrypted()
            || document.trailer.has(b"Encrypt")
            || document.encryption_state.is_some()
        {
            return Err(ExactError::Locked);
        }
        Ok(Self {
            document,
            bytes_len: bytes.len() as u64,
        })
    }

    /// the page objects in order.
    pub fn page_ids(&self) -> Vec<ObjectId> {
        self.document.get_pages().into_values().collect()
    }

    /// reads every page.
    pub fn read_pages(&self) -> Vec<PageContent> {
        let mut interpreter = content::Interpreter::new(&self.document);
        let mut pages = Vec::new();
        for (index, page_id) in self.page_ids().into_iter().enumerate() {
            pages.push(self.read_page(&mut interpreter, index, page_id));
        }
        pages
    }

    fn read_page(
        &self,
        interpreter: &mut content::Interpreter,
        index: usize,
        page_id: ObjectId,
    ) -> PageContent {
        let document = &self.document;
        let page = document.get_dictionary(page_id).ok();
        let media_box = page
            .and_then(|p| inherited(document, p, b"MediaBox"))
            .and_then(|object| match object {
                Object::Array(items) => Some(numbers(document, items)),
                _ => None,
            })
            .filter(|values| values.len() == 4)
            .unwrap_or_else(|| vec![0.0, 0.0, 612.0, 792.0]);
        let rotate = page
            .and_then(|p| inherited(document, p, b"Rotate"))
            .and_then(objects::number)
            .unwrap_or(0.0) as i64;
        let mut content = PageContent {
            index,
            width: media_box[2] - media_box[0],
            height: media_box[3] - media_box[1],
            rotate,
            ..PageContent::default()
        };
        if rotate.rem_euclid(360) != 0 {
            content.problems.push(format!(
                "page rotated by {rotate} degrees, coordinates not rotated"
            ));
        }
        let resources = page.and_then(|p| match inherited(document, p, b"Resources") {
            Some(Object::Dictionary(dict)) => Some(dict),
            _ => None,
        });
        let stream = page_content_bytes(document, page, &mut content.problems);
        interpreter.read_page(
            &mut content,
            &stream,
            resources,
            (media_box[0], media_box[1]),
        );
        content
    }
}

/// a page attribute, looked up through the page tree's parents.
fn inherited<'a>(document: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..32 {
        if let Some(value) = get(document, node, key) {
            return Some(value);
        }
        node = get_dict(document, node, b"Parent")?;
    }
    None
}

/// the page's content streams, joined with a space between them.
fn page_content_bytes(
    document: &Document,
    page: Option<&Dictionary>,
    problems: &mut Vec<String>,
) -> Vec<u8> {
    let Some(page) = page else {
        return Vec::new();
    };
    let streams: Vec<&Object> = match page.get(b"Contents").ok().map(|o| resolve(document, o)) {
        Some(Object::Array(items)) => items.iter().map(|i| resolve(document, i)).collect(),
        Some(other) => vec![other],
        None => Vec::new(),
    };
    let mut bytes = Vec::new();
    for object in streams {
        if let Object::Stream(stream) = object {
            match stream_bytes(stream) {
                Ok(part) => {
                    bytes.extend(part);
                    bytes.push(b'\n');
                }
                Err(reason) => {
                    problems.push(format!("a content stream couldn’t be read ({reason})"))
                }
            }
        }
    }
    bytes
}
