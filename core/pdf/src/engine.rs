//! the only file that talks to pdfium.
//!
//! pdfium is not safe to call from several threads at once, so this
//! engine is owned by a single worker thread (see worker.rs) and never
//! shared directly.

use std::collections::HashMap;
use std::path::Path;

use pdfium_render::prelude::*;

use crate::geometry::{page_pixel_size, tile_rect, PageSize, PixelRect, TileKey};
use crate::{DocumentId, PdfError};

struct OpenDocument {
    document: PdfDocument<'static>,
    page_sizes: Vec<PageSize>,
}

pub struct PdfEngine {
    pdfium: &'static Pdfium,
    documents: HashMap<DocumentId, OpenDocument>,
    next_id: u64,
}

impl PdfEngine {
    /// loads the pdfium library from the given folder.
    pub fn load(library_dir: &Path) -> Result<Self, PdfError> {
        let library_path = Pdfium::pdfium_platform_library_name_at_path(library_dir);
        if !library_path.is_file() {
            return Err(PdfError::EngineUnavailable(format!(
                "{} is missing",
                library_path.display()
            )));
        }
        let pdfium = match Pdfium::bind_to_library(&library_path) {
            Ok(bindings) => Pdfium::new(bindings),
            // the library can only be loaded once per process. if it already
            // is, a new handle to the same loaded library is all we need
            Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
            Err(error) => {
                return Err(PdfError::EngineUnavailable(format!(
                    "{} ({error:?})",
                    library_path.display()
                )))
            }
        };
        // the engine lives for the whole run of the app, and open documents
        // borrow from it, so leaking this one small object keeps the
        // lifetimes simple without costing anything real
        let pdfium: &'static Pdfium = Box::leak(Box::new(pdfium));
        Ok(Self {
            pdfium,
            documents: HashMap::new(),
            next_id: 1,
        })
    }

    /// opens a pdf and returns its id and the size of every page.
    ///
    /// pdfium reads the file lazily, so this stays fast and small even
    /// for very long documents. page sizes are read up front because the
    /// viewer needs them to lay out the scroll area before anything is drawn.
    pub fn open(
        &mut self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<(DocumentId, Vec<PageSize>), PdfError> {
        let document = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(describe_open_error)?;
        let page_sizes = read_page_sizes(&document)?;

        let id = DocumentId(self.next_id);
        self.next_id += 1;
        self.documents.insert(
            id,
            OpenDocument {
                document,
                page_sizes: page_sizes.clone(),
            },
        );
        Ok((id, page_sizes))
    }

    pub fn close(&mut self, id: DocumentId) {
        self.documents.remove(&id);
    }

    pub fn open_document_count(&self) -> usize {
        self.documents.len()
    }

    /// draws one tile and returns it as png bytes.
    pub fn render_tile(&self, key: TileKey) -> Result<Vec<u8>, PdfError> {
        let open = self
            .documents
            .get(&key.document)
            .ok_or(PdfError::UnknownDocument)?;
        let page_size = *open
            .page_sizes
            .get(key.page as usize)
            .ok_or(PdfError::UnknownPage(key.page))?;
        let page_pixels = page_pixel_size(page_size, key.scale_milli);
        let rect = tile_rect(page_pixels, key.column, key.row).ok_or(PdfError::UnknownTile)?;

        let page = open
            .document
            .pages()
            .get(key.page as PdfPageIndex)
            .map_err(|_| PdfError::UnknownPage(key.page))?;
        let pixels = render_region(&page, page_pixels, rect)?;
        encode_png(&pixels, rect.width, rect.height)
    }
}

fn describe_open_error(error: PdfiumError) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            PdfError::PasswordRequired
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            PdfError::CannotOpen("It isn’t a PDF, or the file is damaged.".into())
        }
        PdfiumError::IoError(io_error) => PdfError::CannotOpen(io_error.to_string()),
        other => PdfError::CannotOpen(format!("{other:?}")),
    }
}

fn read_page_sizes(document: &PdfDocument) -> Result<Vec<PageSize>, PdfError> {
    // page_sizes asks pdfium for the sizes only, without loading the pages
    let rects = document
        .pages()
        .page_sizes()
        .map_err(|error| PdfError::CannotOpen(format!("{error:?}")))?;
    Ok(rects
        .iter()
        .map(|rect| PageSize {
            width_pt: rect.width().value,
            height_pt: rect.height().value,
        })
        .collect())
}

/// renders just the wanted rectangle of the page into a bitmap of that size.
///
/// the page is placed at a negative offset so the wanted region lands at
/// the bitmap's top left corner. pdfium clips to the bitmap, so the rest
/// of the page is never rasterised and never allocated.
fn render_region(
    page: &PdfPage,
    page_pixels: (u32, u32),
    rect: PixelRect,
) -> Result<Vec<u8>, PdfError> {
    let failed = |error: PdfiumError| PdfError::RenderFailed(format!("{error:?}"));

    let mut bitmap = PdfBitmap::empty(
        rect.width as Pixels,
        rect.height as Pixels,
        PdfBitmapFormat::BGRA,
    )
    .map_err(failed)?;

    let config = PdfRenderConfig::new()
        .set_fixed_size(page_pixels.0 as Pixels, page_pixels.1 as Pixels)
        .set_origin(-(rect.x as Pixels), -(rect.y as Pixels))
        .set_clear_color(PdfColor::WHITE)
        // ask pdfium for rgba directly so no second pass is needed to swap channels
        .set_reverse_byte_order(true);

    page.render_into_bitmap_with_config(&mut bitmap, &config)
        .map_err(failed)?;
    Ok(bitmap.as_raw_bytes())
}

/// compresses rgba pixels as a png.
///
/// png is lossless, which matters for crisp text. the fast setting is
/// used because tiles are made while the user is scrolling.
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PdfError> {
    let failed = |error: png::EncodingError| PdfError::RenderFailed(error.to_string());

    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(rgba).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(output)
}
