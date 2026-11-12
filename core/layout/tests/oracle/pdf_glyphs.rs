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
    /// x of every thin vertical rule (table column edges, page borders,
    /// column separators), points from the left. (the round trip test
    /// shares this reader and reads glyphs only)
    #[allow(dead_code)]
    pub vertical_rules: Vec<f64>,
    /// every picture: left, top, width, height in points from the top left
    #[allow(dead_code)]
    pub pictures: Vec<[f64; 4]>,
}

/// a path this narrow and at least this tall counts as a vertical rule.
const RULE_WIDTH: f64 = 2.6;
const RULE_HEIGHT: f64 = 3.0;

/// the vertical rules and pictures of one page, from its page objects.
fn page_objects(page: &pdfium_render::prelude::PdfPage<'_>, height: f64, out: &mut PdfPageData) {
    for object in page.objects().iter() {
        let Ok(bounds) = object.bounds() else {
            continue;
        };
        let (left, right) = (
            f64::from(bounds.left().value),
            f64::from(bounds.right().value),
        );
        let (bottom, top) = (
            f64::from(bounds.bottom().value),
            f64::from(bounds.top().value),
        );
        let (width, tall) = ((right - left).abs(), (top - bottom).abs());
        match object.object_type() {
            PdfPageObjectType::Path if width <= RULE_WIDTH && tall >= RULE_HEIGHT => {
                out.vertical_rules.push((left + right) / 2.0);
            }
            PdfPageObjectType::Image => {
                out.pictures.push([left, height - top, width, tall]);
            }
            _ => {}
        }
    }
}

/// what `page_objects` collects for one page.
#[derive(Default)]
struct PdfPageData {
    vertical_rules: Vec<f64>,
    pictures: Vec<[f64; 4]>,
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
        let mut objects = PdfPageData::default();
        page_objects(&page, height, &mut objects);
        let mut out = PdfPage {
            width: f64::from(page.width().value),
            height,
            glyphs: Vec::new(),
            vertical_rules: objects.vertical_rules,
            pictures: objects.pictures,
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
