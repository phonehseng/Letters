//! the pdf being converted: loading it, its pages and their sizes, and
//! which pages the person chose.

use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::geometry::Matrix;
use crate::ConvertError;

/// an open pdf, ready to be converted.
///
/// it keeps the path as well as the parsed document, because pages are
/// drawn by a renderer that reads the file itself.
pub struct PdfSource {
    path: PathBuf,
    document: Document,
    page_ids: Vec<ObjectId>,
}

impl PdfSource {
    /// loads a pdf from disk. a locked pdf gives the "unlock it first"
    /// message and a pdf without pages gives "nothing to convert".
    pub fn open(path: &Path) -> Result<Self, ConvertError> {
        let document = Document::load(path).map_err(|error| {
            let text = error.to_string();
            if looks_like_encryption(&text) {
                ConvertError::Locked
            } else {
                ConvertError::CannotOpen(format!("It may be damaged or not a PDF. ({text})"))
            }
        })?;
        Self::from_document(path, document)
    }

    /// wraps a document that is already in memory. `path` must be a file
    /// with the same content, for the renderer to draw from.
    pub fn from_document(path: &Path, document: Document) -> Result<Self, ConvertError> {
        if document.is_encrypted() {
            return Err(ConvertError::Locked);
        }
        let page_ids: Vec<ObjectId> = document.get_pages().into_values().collect();
        if page_ids.is_empty() {
            return Err(ConvertError::EmptyDocument);
        }
        Ok(Self {
            path: path.to_path_buf(),
            document,
            page_ids,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn page_count(&self) -> usize {
        self.page_ids.len()
    }

    pub(crate) fn page_id(&self, page: usize) -> Result<ObjectId, ConvertError> {
        self.page_ids
            .get(page)
            .copied()
            .ok_or(ConvertError::UnknownPage(page))
    }

    /// how a page is shown: its size in points and the way from pdf
    /// space to the shown page.
    pub fn page_space(&self, page: usize) -> Result<PageSpace, ConvertError> {
        PageSpace::of_page(&self.document, self.page_id(page)?)
    }

    /// the document title from its information dictionary, if it has one.
    pub fn title(&self) -> Option<String> {
        let info = self.document.trailer.get(b"Info").ok()?;
        let info = resolve(&self.document, info).as_dict().ok()?;
        let title = resolve(&self.document, info.get(b"Title").ok()?);
        let text = match title {
            Object::String(bytes, _) => decode_text_string(bytes),
            _ => return None,
        };
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

fn looks_like_encryption(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("encrypt") || lower.contains("decrypt") || lower.contains("password")
}

/// pdf text strings are utf-16 with a byte order mark, or pdfdocencoding
/// (close enough to latin-1 for a title).
pub(crate) fn decode_text_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|byte| char::from(*byte)).collect()
    }
}

/// follows a reference to the object it names (or returns the object).
pub(crate) fn resolve<'a>(doc: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

/// a page attribute that may be set on the page or on any parent node.
pub(crate) fn inherited<'a>(
    doc: &'a Document,
    page: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Object> {
    let mut node = page;
    // a damaged page tree could loop, so the climb is limited
    for _ in 0..32 {
        if let Ok(value) = node.get(key) {
            return Some(resolve(doc, value));
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

pub(crate) fn numbers(object: &Object) -> Vec<f32> {
    match object {
        Object::Array(items) => items.iter().filter_map(number).collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

/// a page as it is shown: its size in points and the matrix from pdf
/// user space to the shown page (top left origin, y going down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSpace {
    pub width: f32,
    pub height: f32,
    pub to_shown: Matrix,
}

impl PageSpace {
    pub(crate) fn of_page(doc: &Document, page_id: ObjectId) -> Result<Self, ConvertError> {
        let page = doc
            .get_dictionary(page_id)
            .map_err(|_| ConvertError::Damaged("a page is missing".into()))?;
        let media = inherited(doc, page, b"MediaBox")
            .map(numbers)
            .filter(|box_| box_.len() == 4)
            .unwrap_or_else(|| vec![0.0, 0.0, 612.0, 792.0]);
        let crop = inherited(doc, page, b"CropBox")
            .map(numbers)
            .filter(|box_| box_.len() == 4)
            .unwrap_or_else(|| media.clone());
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(number)
            .unwrap_or(0.0) as i32;
        Ok(Self::from_boxes(&crop, rotate))
    }

    /// works out the shown page from its crop box and /Rotate.
    pub(crate) fn from_boxes(crop: &[f32], rotate: i32) -> Self {
        let (x0, x1) = (crop[0].min(crop[2]), crop[0].max(crop[2]));
        let (y0, y1) = (crop[1].min(crop[3]), crop[1].max(crop[3]));
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        // first flip pdf space (y up, origin bottom left of the crop box)
        // into an upright page with y going down
        let upright = Matrix::new(1.0, 0.0, 0.0, -1.0, -x0, y1);
        // then turn it the way /Rotate turns the page, clockwise on screen
        let (turn, width, height) = match rotate.rem_euclid(360) {
            90 => (Matrix::new(0.0, 1.0, -1.0, 0.0, h, 0.0), h, w),
            180 => (Matrix::new(-1.0, 0.0, 0.0, -1.0, w, h), w, h),
            270 => (Matrix::new(0.0, -1.0, 1.0, 0.0, 0.0, w), h, w),
            _ => (Matrix::IDENTITY, w, h),
        };
        Self {
            width,
            height,
            to_shown: upright.then(&turn),
        }
    }
}

/// which pages to convert.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageSelection {
    #[default]
    All,
    /// zero based page numbers, in any order; repeats are ignored
    Pages(Vec<usize>),
}

impl PageSelection {
    /// the chosen pages in order, checked against the document.
    pub fn resolve(&self, page_count: usize) -> Result<Vec<usize>, ConvertError> {
        match self {
            PageSelection::All => Ok((0..page_count).collect()),
            PageSelection::Pages(pages) => {
                if pages.is_empty() {
                    return Err(ConvertError::InvalidSetting(
                        "Choose at least one page to convert.".into(),
                    ));
                }
                if let Some(missing) = pages.iter().find(|page| **page >= page_count) {
                    return Err(ConvertError::UnknownPage(*missing));
                }
                let mut sorted = pages.clone();
                sorted.sort_unstable();
                sorted.dedup();
                Ok(sorted)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upright_pages_flip_y() {
        let space = PageSpace::from_boxes(&[0.0, 0.0, 612.0, 792.0], 0);
        assert_eq!((space.width, space.height), (612.0, 792.0));
        assert_eq!(space.to_shown.apply(72.0, 720.0), (72.0, 72.0));
    }

    #[test]
    fn turned_pages_swap_sides() {
        let space = PageSpace::from_boxes(&[0.0, 0.0, 612.0, 792.0], 90);
        assert_eq!((space.width, space.height), (792.0, 612.0));
        // the top left of the unturned page ends up at the top right
        assert_eq!(space.to_shown.apply(0.0, 792.0), (792.0, 0.0));
        let space = PageSpace::from_boxes(&[0.0, 0.0, 612.0, 792.0], 270);
        assert_eq!(space.to_shown.apply(0.0, 792.0), (0.0, 612.0));
    }

    #[test]
    fn crop_boxes_move_the_origin() {
        let space = PageSpace::from_boxes(&[100.0, 100.0, 300.0, 400.0], 0);
        assert_eq!(space.to_shown.apply(100.0, 400.0), (0.0, 0.0));
    }

    #[test]
    fn page_choices_are_checked_and_sorted() {
        let pages = PageSelection::Pages(vec![2, 0, 2]);
        assert_eq!(pages.resolve(3).unwrap(), vec![0, 2]);
        assert_eq!(
            PageSelection::Pages(vec![5]).resolve(3),
            Err(ConvertError::UnknownPage(5))
        );
        assert!(PageSelection::Pages(vec![]).resolve(3).is_err());
    }
}
