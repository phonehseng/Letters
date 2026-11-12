//! reads where every glyph of a pdf sits, with pdfium.
//!
//! the same reader is used for word's pdfs, libreoffice's pdfs and our own,
//! so differences between them come from layout, not from the reader. pen
//! origins are what layout decides; ink boxes depend on outlines, so only
//! origins are used.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

/// one drawn character: its pen origin in points from the page's top left.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: char,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub font: String,
}

#[derive(Clone, Debug, Default)]
pub struct PdfPage {
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
}

/// pdfium is not thread safe, so the one bound library sits behind a lock.
pub struct SharedPdfium(pub Pdfium);

// safety: every use goes through the mutex below, one thread at a time
unsafe impl Send for SharedPdfium {}

/// the pdfium library is bound once per process; later calls reuse it.
pub fn pdfium() -> MutexGuard<'static, SharedPdfium> {
    static PDFIUM: OnceLock<Mutex<SharedPdfium>> = OnceLock::new();
    PDFIUM
        .get_or_init(|| {
            let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium");
            let library = Pdfium::pdfium_platform_library_name_at_path(&folder);
            let pdfium = match Pdfium::bind_to_library(&library) {
                Ok(bindings) => Pdfium::new(bindings),
                Err(_) => Pdfium::default(),
            };
            Mutex::new(SharedPdfium(pdfium))
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// every non-space glyph of every page, in content order.
pub fn read_pdf(path: &Path) -> Result<Vec<PdfPage>, String> {
    let library = pdfium();
    let document = library
        .0
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("{}: {error:?}", path.display()))?;
    let mut pages = Vec::new();
    for page in document.pages().iter() {
        let height = f64::from(page.height().value);
        let mut out = PdfPage {
            width: f64::from(page.width().value),
            height,
            glyphs: Vec::new(),
        };
        if let Ok(text) = page.text() {
            for character in text.chars().iter() {
                if character.is_generated().unwrap_or(false) {
                    continue;
                }
                let Some(ch) = character.unicode_char() else {
                    continue;
                };
                if ch.is_whitespace() || ch.is_control() {
                    continue;
                }
                let Ok((x, y)) = character.origin() else {
                    continue;
                };
                out.glyphs.push(Glyph {
                    ch,
                    x: f64::from(x.value),
                    y: height - f64::from(y.value),
                    size: f64::from(character.scaled_font_size().value),
                    font: character.font_name(),
                });
            }
        }
        pages.push(out);
    }
    Ok(pages)
}
