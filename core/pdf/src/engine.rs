//! the only file that talks to pdfium.
//!
//! pdfium is not safe to call from several threads at once, so this
//! engine is owned by a single worker thread (see worker.rs) and never
//! shared directly.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::Path;

use pdfium_render::prelude::*;

use crate::geometry::{page_pixel_size, tile_rect, PageSize, PixelRect, TileKey};
use crate::outline::{OutlineItem, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS};
use crate::search::{find_matches, snippet, CharRange, SearchMatch, SearchQuery, TextRect};
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

    pub fn page_count(&self, id: DocumentId) -> Result<u32, PdfError> {
        Ok(self.document(id)?.page_sizes.len() as u32)
    }

    /// the document's table of contents, flattened in reading order.
    pub fn outline(&self, id: DocumentId) -> Result<Vec<OutlineItem>, PdfError> {
        let open = self.document(id)?;
        let mut outline = OutlineReader {
            page_count: open.page_sizes.len(),
            items: Vec::new(),
            seen: HashSet::new(),
        };
        if let Some(first) = open.document.bookmarks().root() {
            outline.read_siblings(first, 0);
        }
        Ok(outline.items)
    }

    /// searches the given pages and returns at most `limit` matches.
    ///
    /// each page's text is loaded, searched and dropped before the next
    /// page, so a long document never has all its text in memory at once.
    pub fn search_pages(
        &self,
        id: DocumentId,
        query: &SearchQuery,
        pages: Range<u32>,
        limit: usize,
    ) -> Result<Vec<SearchMatch>, PdfError> {
        let open = self.document(id)?;
        let mut matches = Vec::new();
        for page_number in pages {
            if matches.len() >= limit {
                break;
            }
            let Some(&page_size) = open.page_sizes.get(page_number as usize) else {
                break;
            };
            // a page that can't be loaded just has nothing to find
            let Ok(page) = open.document.pages().get(page_number as PdfPageIndex) else {
                continue;
            };
            let found = search_page(&page, page_number, page_size, query);
            let room = limit - matches.len();
            matches.extend(found.into_iter().take(room));
        }
        Ok(matches)
    }

    fn document(&self, id: DocumentId) -> Result<&OpenDocument, PdfError> {
        self.documents.get(&id).ok_or(PdfError::UnknownDocument)
    }

    /// draws one tile and returns it as png bytes.
    pub fn render_tile(&self, key: TileKey) -> Result<Vec<u8>, PdfError> {
        let open = self.document(key.document)?;
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

/// walks the outline tree, collecting entries that lead to a page.
struct OutlineReader<'a> {
    page_count: usize,
    items: Vec<OutlineItem>,
    /// bookmarks already visited, so a looping outline can't loop forever
    seen: HashSet<PdfBookmark<'a>>,
}

impl<'a> OutlineReader<'a> {
    fn read_siblings(&mut self, first: PdfBookmark<'a>, level: u32) {
        let mut next = Some(first);
        while let Some(bookmark) = next {
            let too_much = level > MAX_OUTLINE_DEPTH || self.items.len() >= MAX_OUTLINE_ITEMS;
            if too_much || !self.seen.insert(bookmark.clone()) {
                return;
            }
            let child_level = match self.page_of(&bookmark) {
                Some(page) => {
                    self.items.push(OutlineItem {
                        title: title_of(&bookmark),
                        page,
                        level,
                    });
                    level + 1
                }
                // an entry that leads nowhere is left out. its children
                // move up a level so the list has no gaps in its indenting
                None => level,
            };
            if let Some(child) = bookmark.first_child() {
                self.read_siblings(child, child_level);
            }
            next = bookmark.next_sibling();
        }
    }

    /// the page an entry leads to, either named directly or through a
    /// "go to" action (pdf writers use both).
    fn page_of(&self, bookmark: &PdfBookmark) -> Option<u32> {
        let index = match bookmark.destination() {
            Some(destination) => destination.page_index().ok()?,
            None => {
                let action = bookmark.action()?;
                let local = action.as_local_destination_action()?;
                local.destination().ok()?.page_index().ok()?
            }
        };
        let page = u32::try_from(index).ok()?;
        ((page as usize) < self.page_count).then_some(page)
    }
}

fn title_of(bookmark: &PdfBookmark) -> String {
    let title = bookmark.title().unwrap_or_default();
    let title = title.trim();
    if title.is_empty() {
        "Untitled".to_owned()
    } else {
        title.to_owned()
    }
}

/// finds the query on one page. the page's text is dropped on return.
fn search_page(
    page: &PdfPage,
    page_number: u32,
    page_size: PageSize,
    query: &SearchQuery,
) -> Vec<SearchMatch> {
    let Ok(text) = page.text() else {
        return Vec::new();
    };
    let chars: Vec<char> = text
        .chars()
        .iter()
        .map(|letter| letter.unicode_char().unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    find_matches(&chars, query)
        .into_iter()
        .map(|range| SearchMatch {
            page: page_number,
            rects: match_rects(page, &text, page_size, range),
            snippet: snippet(&chars, range),
        })
        .collect()
}

/// the rectangles a match covers, one per line, measured from the top left.
fn match_rects(
    page: &PdfPage,
    text: &PdfPageText,
    page_size: PageSize,
    range: CharRange,
) -> Vec<TextRect> {
    text.segments_subset(range.start, range.count)
        .iter()
        .filter_map(|segment| to_top_left(page, page_size, segment.bounds()))
        .collect()
}

/// how finely positions are measured when converting them: 1/64 point.
const POSITION_STEPS_PER_POINT: f32 = 64.0;

/// turns a rectangle in pdf coordinates (origin at the bottom left, and
/// shifted or rotated however the file says) into the viewer's
/// coordinates: points from the top left of the page as it is shown.
///
/// pdfium's own page-to-screen conversion is used, the same one it uses
/// when drawing tiles, so highlights land on the drawn text even on
/// rotated pages or pages whose box does not start at zero. that
/// conversion answers in whole pixels, so it is asked about a page 64
/// times larger than life and the answer is divided back down.
fn to_top_left(page: &PdfPage, page_size: PageSize, rect: PdfRect) -> Option<TextRect> {
    let config = PdfRenderConfig::new().set_fixed_size(
        (page_size.width_pt * POSITION_STEPS_PER_POINT).round() as Pixels,
        (page_size.height_pt * POSITION_STEPS_PER_POINT).round() as Pixels,
    );
    let corner_a = page
        .points_to_pixels(rect.left(), rect.top(), &config)
        .ok()?;
    let corner_b = page
        .points_to_pixels(rect.right(), rect.bottom(), &config)
        .ok()?;
    // after a rotation either corner may be the top left one
    let left = corner_a.0.min(corner_b.0) as f32;
    let top = corner_a.1.min(corner_b.1) as f32;
    let right = corner_a.0.max(corner_b.0) as f32;
    let bottom = corner_a.1.max(corner_b.1) as f32;
    Some(TextRect {
        x: left / POSITION_STEPS_PER_POINT,
        y: top / POSITION_STEPS_PER_POINT,
        width: (right - left) / POSITION_STEPS_PER_POINT,
        height: (bottom - top) / POSITION_STEPS_PER_POINT,
    })
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
    // page_sizes asks pdfium for the sizes only, without loading the pages.
    // the sizes already include each page's rotation
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
