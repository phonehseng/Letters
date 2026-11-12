//! a `PageSource` that reads pdf files with lopdf.
//!
//! the letters come from interpreting each page's content stream (see
//! interpreter.rs); pictures of pages come from a function the caller
//! gives, because drawing pdfs is the pdf engine's job. the shell passes
//! one that asks the app's renderer; tests pass one built on letters-pdf.

mod cmap;
mod fonts;
mod interpreter;
mod page_frame;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::geometry::PageSize;
use crate::hashing::Fnv64;
use crate::source::{Glyph, PageSource, RgbaImage};
use crate::CompareError;
pub(crate) use fonts::strip_subset_tag;
use interpreter::{FontCache, Interpreter};
use page_frame::{inherited, PageFrame};

/// draws page `page` (0-based) at `scale` pixels per point, as the person sees it.
pub type Renderer = Box<dyn Fn(u32, f32) -> Result<RgbaImage, String> + Send + Sync>;

/// how deep the drawing fingerprint follows references between objects.
/// deep enough for fonts and pictures inside forms, shallow enough to stay quick.
const MAX_FINGERPRINT_DEPTH: usize = 6;

/// a pdf document read with lopdf.
pub struct LopdfSource {
    document: Document,
    pages: Vec<ObjectId>,
    frames: Vec<PageFrame>,
    fonts: FontCache,
    renderer: Option<Renderer>,
}

impl LopdfSource {
    /// reads a pdf file. a locked (password-protected) file is refused
    /// with `CompareError::Locked`, asking for it to be unlocked first.
    pub fn open(path: &Path) -> Result<Self, CompareError> {
        let document = Document::load(path).map_err(read_error)?;
        Self::from_document(document)
    }

    /// reads a pdf held in memory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CompareError> {
        let document = Document::load_mem(bytes).map_err(read_error)?;
        Self::from_document(document)
    }

    /// uses a document that is already open.
    pub fn from_document(document: Document) -> Result<Self, CompareError> {
        if document.is_encrypted() {
            return Err(CompareError::Locked);
        }
        let pages: Vec<ObjectId> = document.get_pages().into_values().collect();
        let frames = pages
            .iter()
            .map(|&page| PageFrame::read(&document, page))
            .collect();
        Ok(Self {
            document,
            pages,
            frames,
            fonts: Mutex::new(HashMap::new()),
            renderer: None,
        })
    }

    /// gives the source a way to draw its pages, needed to compare
    /// pictures, show overlays and put thumbnails in the report.
    pub fn with_renderer(
        mut self,
        renderer: impl Fn(u32, f32) -> Result<RgbaImage, String> + Send + Sync + 'static,
    ) -> Self {
        self.renderer = Some(Box::new(renderer));
        self
    }

    fn page_id(&self, page: u32) -> Result<ObjectId, CompareError> {
        self.pages
            .get(page as usize)
            .copied()
            .ok_or(CompareError::NoSuchPage(page + 1))
    }

    fn resources(&self, page: ObjectId) -> Option<&Dictionary> {
        let object = inherited(&self.document, page, b"Resources")?;
        let (_, object) = self.document.dereference(object).ok()?;
        object.as_dict().ok()
    }
}

/// turns a lopdf loading error into a sentence for the person.
fn read_error(error: lopdf::Error) -> CompareError {
    match error {
        lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => CompareError::Locked,
        other => CompareError::CannotRead(other.to_string()),
    }
}

impl PageSource for LopdfSource {
    fn page_count(&self) -> u32 {
        self.pages.len() as u32
    }

    fn page_size(&self, page: u32) -> Result<PageSize, CompareError> {
        self.page_id(page)?;
        Ok(self.frames[page as usize].visible_size())
    }

    fn glyphs(&self, page: u32) -> Result<Vec<Glyph>, CompareError> {
        let id = self.page_id(page)?;
        let content = self.document.get_page_content(id);
        let interpreter = Interpreter::new(&self.document, self.frames[page as usize], &self.fonts);
        Ok(interpreter.read_page(&content, self.resources(id)))
    }

    fn render(&self, page: u32, scale: f32) -> Result<RgbaImage, CompareError> {
        self.page_id(page)?;
        let renderer = self.renderer.as_ref().ok_or(CompareError::NoRenderer)?;
        renderer(page, scale).map_err(|reason| CompareError::RenderFailed {
            page: page + 1,
            reason,
        })
    }

    fn can_render(&self) -> bool {
        self.renderer.is_some()
    }

    /// hashes everything that decides what the page looks like: its
    /// visible area, its content stream, and the fonts, pictures and forms
    /// it uses (followed by value, so two files that store the same things
    /// under different object numbers still get the same fingerprint).
    fn drawing_fingerprint(&self, page: u32) -> Option<u64> {
        let id = self.page_id(page).ok()?;
        let mut hash = Fnv64::new();
        for part in self.frames[page as usize].fingerprint_parts() {
            hash.write(&part.to_le_bytes());
        }
        hash.write(&self.document.get_page_content(id));
        if let Some(resources) = self.resources(id) {
            hash_dictionary(&self.document, resources, &mut hash, 0);
        }
        Some(hash.finish())
    }
}

fn hash_object(document: &Document, object: &Object, hash: &mut Fnv64, depth: usize) {
    if depth > MAX_FINGERPRINT_DEPTH {
        return;
    }
    match object {
        Object::Null => hash.write(b"n"),
        Object::Boolean(value) => hash.write(if *value { b"t" } else { b"f" }),
        Object::Integer(value) => hash.write(&value.to_le_bytes()),
        Object::Real(value) => hash.write(&value.to_le_bytes()),
        Object::Name(name) => {
            hash.write(b"/");
            hash.write(name);
        }
        Object::String(bytes, _) => {
            hash.write(b"(");
            hash.write(bytes);
        }
        Object::Array(items) => {
            hash.write(b"[");
            for item in items {
                hash_object(document, item, hash, depth + 1);
            }
        }
        Object::Dictionary(dict) => hash_dictionary(document, dict, hash, depth + 1),
        Object::Stream(stream) => {
            hash_dictionary(document, &stream.dict, hash, depth + 1);
            hash.write(&stream.content);
        }
        Object::Reference(id) => {
            if let Ok(target) = document.get_object(*id) {
                hash_object(document, target, hash, depth + 1);
            }
        }
    }
}

fn hash_dictionary(document: &Document, dict: &Dictionary, hash: &mut Fnv64, depth: usize) {
    // keys are hashed in sorted order, since writers list them in any order.
    // /Parent leads back up the page tree and /Length only describes the bytes
    let mut entries: Vec<(&Vec<u8>, &Object)> = dict
        .iter()
        .filter(|(key, _)| key.as_slice() != b"Parent" && key.as_slice() != b"Length")
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    hash.write(b"<<");
    for (key, value) in entries {
        hash.write(key);
        hash_object(document, value, hash, depth);
    }
}
