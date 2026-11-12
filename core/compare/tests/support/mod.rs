//! helpers shared by the integration tests: pdfs written by hand, and a
//! renderer built on letters-pdf (the app's own pdfium engine).
//!
//! the pdfs are written byte by byte so the tests don't depend on any pdf
//! writing library being right. `PdfSpec::variant` writes the same pages
//! with different bytes: objects numbered and stored in another order,
//! text split into pieces and numbers written differently. a comparison
//! must see the two as identical.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use letters_compare::{LopdfSource, PageSource, Rect, RgbaImage};
use letters_pdf::{page_pixel_size, DocumentId, RenderWorker, TileKey, TILE_SIZE};

pub const LETTER_WIDTH: f32 = 612.0;
pub const LETTER_HEIGHT: f32 = 792.0;

/// where the pdfium library lives for tests (see core/pdf's tests).
pub fn pdfium_dir() -> PathBuf {
    let dir = match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    };
    assert!(
        dir.exists(),
        "pdfium was not found in {}. run tools/fetch-pdfium first.",
        dir.display()
    );
    dir
}

/// a fresh, empty folder for one test's files.
pub fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-compare-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// the standard fonts the test pdfs can use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    Helvetica,
    HelveticaBold,
    Times,
}

impl Face {
    fn resource(&self) -> &'static str {
        match self {
            Face::Helvetica => "F1",
            Face::HelveticaBold => "F2",
            Face::Times => "F3",
        }
    }
}

/// one line of text, placed by its baseline.
#[derive(Clone, Debug)]
pub struct Text {
    pub x: f32,
    /// the baseline's distance from the top of the page, in points
    pub baseline: f32,
    pub size: f32,
    pub text: String,
    pub face: Face,
    pub colour: [f32; 3],
}

pub fn text(x: f32, baseline: f32, size: f32, text: &str) -> Item {
    Item::Text(Text {
        x,
        baseline,
        size,
        text: text.to_owned(),
        face: Face::Helvetica,
        colour: [0.0, 0.0, 0.0],
    })
}

impl Item {
    pub fn face(mut self, face: Face) -> Item {
        if let Item::Text(line) = &mut self {
            line.face = face;
        }
        self
    }

    pub fn colour(mut self, colour: [f32; 3]) -> Item {
        if let Item::Text(line) = &mut self {
            line.colour = colour;
        }
        self
    }
}

/// a small picture, stored as rgb bytes row by row.
#[derive(Clone, Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Picture {
    /// a picture of one colour.
    pub fn plain(width: u32, height: u32, colour: [u8; 3]) -> Picture {
        Picture {
            width,
            height,
            rgb: colour.repeat((width * height) as usize),
        }
    }
}

/// something drawn on a page. positions are from the top left, y down.
#[derive(Clone, Debug)]
pub enum Item {
    Text(Text),
    /// a filled rectangle
    Box {
        x: f32,
        top: f32,
        width: f32,
        height: f32,
        colour: [f32; 3],
    },
    /// a picture stretched over a rectangle
    Image {
        x: f32,
        top: f32,
        width: f32,
        height: f32,
        picture: Picture,
    },
    /// drawing instructions written as they are, for unusual cases
    Raw(String),
}

pub fn filled_box(x: f32, top: f32, width: f32, height: f32, colour: [f32; 3]) -> Item {
    Item::Box {
        x,
        top,
        width,
        height,
        colour,
    }
}

#[derive(Clone, Debug)]
pub struct PageSpec {
    pub width: f32,
    pub height: f32,
    pub rotate: u32,
    /// left, bottom, right, top in pdf points
    pub crop: Option<[f32; 4]>,
    pub items: Vec<Item>,
}

/// a us letter page with these items.
pub fn page(items: Vec<Item>) -> PageSpec {
    PageSpec {
        width: LETTER_WIDTH,
        height: LETTER_HEIGHT,
        rotate: 0,
        crop: None,
        items,
    }
}

#[derive(Clone, Debug, Default)]
pub struct PdfSpec {
    pub pages: Vec<PageSpec>,
    pub variant: bool,
}

pub fn pdf(pages: Vec<PageSpec>) -> PdfSpec {
    PdfSpec {
        pages,
        variant: false,
    }
}

/// lines of text laid out top to bottom from `first_baseline`, `leading` apart.
pub fn lines(x: f32, first_baseline: f32, size: f32, leading: f32, texts: &[&str]) -> Vec<Item> {
    texts
        .iter()
        .enumerate()
        .map(|(index, line)| text(x, first_baseline + index as f32 * leading, size, line))
        .collect()
}

/// a page of distinct sentences, so pages can be told apart: page `seed`
/// gets words no other seed uses.
pub fn numbered_page(seed: usize, line_count: usize) -> PageSpec {
    let texts: Vec<String> = (0..line_count)
        .map(|line| {
            (0..8)
                .map(|word| format!("w{seed}x{line}y{word}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    page(lines(72.0, 90.0, 11.0, 15.0, &texts))
}

/// writes the pdf and returns its bytes.
pub fn pdf_bytes(spec: &PdfSpec) -> Vec<u8> {
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();
    let page_count = spec.pages.len();
    // object numbers: 1 catalog, 2 pages, 3..5 fonts, then per page: page,
    // contents, and one per picture. the variant numbers pages backwards.
    let mut next = 6;
    let mut page_numbers = Vec::new();
    let order: Vec<usize> = if spec.variant {
        (0..page_count).rev().collect()
    } else {
        (0..page_count).collect()
    };
    let mut numbering = vec![(0, 0, Vec::new()); page_count];
    for &index in &order {
        let pictures = spec.pages[index]
            .items
            .iter()
            .filter(|item| matches!(item, Item::Image { .. }))
            .count();
        let page_number = next;
        let content_number = next + 1;
        let picture_numbers: Vec<usize> = (0..pictures).map(|offset| next + 2 + offset).collect();
        next += 2 + pictures;
        numbering[index] = (page_number, content_number, picture_numbers);
    }
    for (page_number, _, _) in &numbering {
        page_numbers.push(*page_number);
    }

    objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    let kids: String = page_numbers
        .iter()
        .map(|number| format!("{number} 0 R "))
        .collect();
    objects.push((
        2,
        format!("<< /Type /Pages /Count {page_count} /Kids [ {kids}] >>").into_bytes(),
    ));
    for (number, name) in [(3, "Helvetica"), (4, "Helvetica-Bold"), (5, "Times-Roman")] {
        objects.push((
            number,
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>"
            )
            .into_bytes(),
        ));
    }
    for (index, spec_page) in spec.pages.iter().enumerate() {
        let (page_number, content_number, picture_numbers) = &numbering[index];
        let pictures: String = picture_numbers
            .iter()
            .enumerate()
            .map(|(offset, number)| format!("/Im{offset} {number} 0 R "))
            .collect();
        let crop = spec_page
            .crop
            .map(|[a, b, c, d]| format!(" /CropBox [{a} {b} {c} {d}]"))
            .unwrap_or_default();
        objects.push((
            *page_number,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}]{crop} /Rotate {} /Contents {content_number} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R >> /XObject << {pictures}>> >> >>",
                spec_page.width, spec_page.height, spec_page.rotate
            )
            .into_bytes(),
        ));
        let content = page_content(spec_page, spec.variant);
        let mut body = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        body.extend_from_slice(&content);
        body.extend_from_slice(b"\nendstream");
        objects.push((*content_number, body));
        let images = spec_page.items.iter().filter_map(|item| match item {
            Item::Image { picture, .. } => Some(picture),
            _ => None,
        });
        for (number, picture) in picture_numbers.iter().zip(images) {
            let mut body = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
                picture.width,
                picture.height,
                picture.rgb.len()
            )
            .into_bytes();
            body.extend_from_slice(&picture.rgb);
            body.extend_from_slice(b"\nendstream");
            objects.push((*number, body));
        }
    }
    if spec.variant {
        objects.reverse();
    } else {
        objects.sort_by_key(|(number, _)| *number);
    }
    write_objects(objects)
}

/// the drawing instructions of a page.
fn page_content(spec: &PageSpec, variant: bool) -> Vec<u8> {
    let mut out = String::new();
    let mut picture = 0;
    for item in &spec.items {
        match item {
            Item::Text(line) => {
                let y = spec.height - line.baseline;
                let [r, g, b] = line.colour;
                if variant {
                    // the same drawing, written another way: the text in
                    // pieces, a text matrix instead of a move, padded numbers
                    let pieces: Vec<String> = line
                        .text
                        .split(' ')
                        .map(|word| format!("({})", escape(word)))
                        .collect();
                    out.push_str(&format!(
                        "BT\n{r:.3} {g:.3} {b:.3} rg\n/{} {:.3} Tf\n1 0 0 1 {:.3} {:.3} Tm\n[{}] TJ\nET\n",
                        line.face.resource(),
                        line.size,
                        line.x,
                        y,
                        pieces.join(" ( ) ")
                    ));
                } else {
                    out.push_str(&format!(
                        "BT {r} {g} {b} rg /{} {} Tf {} {} Td ({}) Tj ET\n",
                        line.face.resource(),
                        line.size,
                        line.x,
                        y,
                        escape(&line.text)
                    ));
                }
            }
            Item::Box {
                x,
                top,
                width,
                height,
                colour: [r, g, b],
            } => {
                out.push_str(&format!(
                    "q {r} {g} {b} rg {x} {} {width} {height} re f Q\n",
                    spec.height - top - height
                ));
            }
            Item::Image {
                x,
                top,
                width,
                height,
                ..
            } => {
                out.push_str(&format!(
                    "q {width} 0 0 {height} {x} {} cm /Im{picture} Do Q\n",
                    spec.height - top - height
                ));
                picture += 1;
            }
            Item::Raw(instructions) => {
                out.push_str(instructions);
                out.push('\n');
            }
        }
    }
    out.into_bytes()
}

/// a pdf from objects written by hand, numbered from 1, object 1 the catalog.
pub fn raw_pdf(objects: Vec<(usize, String)>) -> Vec<u8> {
    write_objects(
        objects
            .into_iter()
            .map(|(number, body)| (number, body.into_bytes()))
            .collect(),
    )
}

/// a stream object body with its length filled in.
pub fn stream_body(dictionary_entries: &str, content: &str) -> String {
    format!(
        "<< {dictionary_entries} /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// lays objects out in the given order and adds the table of their positions.
fn write_objects(objects: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let count = objects.iter().map(|(number, _)| *number).max().unwrap_or(0) + 1;
    let mut offsets = vec![0usize; count];
    for (number, body) in objects {
        offsets[number] = bytes.len();
        bytes.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        bytes.extend_from_slice(&body);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let table_start = bytes.len();
    let mut table = format!("xref\n0 {count}\n0000000000 65535 f \n");
    for offset in &offsets[1..] {
        table.push_str(&format!("{offset:010} 00000 n \n"));
    }
    table.push_str(&format!(
        "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{table_start}\n%%EOF\n"
    ));
    bytes.extend_from_slice(table.as_bytes());
    bytes
}

/// a written test document, open in the pdf engine too.
pub struct TestDoc {
    pub path: PathBuf,
    pub id: DocumentId,
    pub worker: RenderWorker,
    page_sizes: Vec<letters_pdf::PageSize>,
}

impl TestDoc {
    pub fn write(worker: &RenderWorker, dir: &Path, name: &str, spec: &PdfSpec) -> TestDoc {
        Self::write_bytes(worker, dir, name, &pdf_bytes(spec))
    }

    pub fn write_bytes(worker: &RenderWorker, dir: &Path, name: &str, bytes: &[u8]) -> TestDoc {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        let info = worker.open(&path, None).unwrap();
        TestDoc {
            path,
            id: info.id,
            worker: worker.clone(),
            page_sizes: info.page_sizes,
        }
    }

    /// the document as the comparison reads it, able to draw its pages.
    pub fn source(&self) -> LopdfSource {
        let worker = self.worker.clone();
        let id = self.id;
        let sizes = self.page_sizes.clone();
        LopdfSource::open(&self.path)
            .unwrap()
            .with_renderer(move |page, scale| render_page(&worker, id, &sizes, page, scale))
    }

    /// where pdfium says the first match of `word` is on `page`.
    pub fn word_rect(&self, page: u32, word: &str) -> Rect {
        let results = self.worker.search(self.id, word).unwrap();
        let found = results
            .matches
            .iter()
            .find(|found| found.page == page)
            .unwrap_or_else(|| panic!("pdfium didn't find {word:?} on page {page}"));
        let rect = found.rects[0];
        Rect::new(rect.x, rect.y, rect.width, rect.height)
    }
}

pub fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), 64 * 1024 * 1024).unwrap()
}

/// writes two documents and opens both in one engine.
pub fn write_pair(test: &str, old: &PdfSpec, new: &PdfSpec) -> (TestDoc, TestDoc) {
    write_pair_with_cache(test, old, new, 64 * 1024 * 1024)
}

/// the same, with a tile cache of the given size (the cache's memory is
/// the engine's, not the comparison's, so memory tests keep it small).
pub fn write_pair_with_cache(
    test: &str,
    old: &PdfSpec,
    new: &PdfSpec,
    cache_bytes: usize,
) -> (TestDoc, TestDoc) {
    let dir = temp_dir(test);
    let worker = RenderWorker::start(&pdfium_dir(), cache_bytes).unwrap();
    let old = TestDoc::write(&worker, &dir, "old.pdf", old);
    let new = TestDoc::write(&worker, &dir, "new.pdf", new);
    (old, new)
}

/// draws a whole page by asking the engine for each of its tiles.
pub fn render_page(
    worker: &RenderWorker,
    id: DocumentId,
    sizes: &[letters_pdf::PageSize],
    page: u32,
    scale: f32,
) -> Result<RgbaImage, String> {
    let size = *sizes.get(page as usize).ok_or("no such page")?;
    let scale_milli = (scale * 1000.0).round() as u32;
    let (width, height) = page_pixel_size(size, scale_milli);
    let mut image = RgbaImage::white(width, height);
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document: id,
                page,
                scale_milli,
                column,
                row,
            };
            let png = worker.tile(key).map_err(|error| error.to_string())?;
            let tile = decode_png(&png);
            for y in 0..tile.height {
                for x in 0..tile.width {
                    let (page_x, page_y) = (column * TILE_SIZE + x, row * TILE_SIZE + y);
                    if page_x < width && page_y < height {
                        image.set_pixel(page_x, page_y, tile.pixel(x, y));
                    }
                }
            }
        }
    }
    Ok(image)
}

pub fn decode_png(bytes: &[u8]) -> RgbaImage {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    RgbaImage {
        width: info.width,
        height: info.height,
        pixels,
    }
}

/// true when some rectangle in `rects` sits on `expected` within `tolerance`
/// points on every side.
///
/// pdfium's boxes hug the ink of the letters, while the comparison's boxes
/// run from the font's ascent to its descent, so a word without letters
/// below the line (like "cat") has a pdfium box that stops at the baseline.
/// the depth below the baseline is therefore allowed on the bottom edge, on
/// whichever side of the page that is once it is turned.
pub fn lands_on(rects: &[Rect], expected: Rect, tolerance: f32) -> bool {
    rects.iter().any(|rect| {
        let below_line = 0.25
            * rect
                .height
                .min(rect.width)
                .max(expected.height.min(expected.width));
        let near = |a: f32, b: f32| (a - b).abs() <= tolerance;
        let near_or_beyond =
            |ours: f32, theirs: f32| (ours - theirs).abs() <= tolerance + below_line;
        near_or_beyond(rect.x, expected.x)
            && near_or_beyond(rect.y, expected.y)
            && near_or_beyond(rect.right(), expected.right())
            && near_or_beyond(rect.bottom(), expected.bottom())
            && (near(rect.x, expected.x) || near(rect.right(), expected.right()))
            && (near(rect.y, expected.y) || near(rect.bottom(), expected.bottom()))
    })
}

/// true when some rectangle in `rects` holds the middle of `expected`.
pub fn covers(rects: &[Rect], expected: Rect) -> bool {
    rects.iter().any(|rect| {
        rect.x <= expected.center_x()
            && rect.right() >= expected.center_x()
            && rect.y <= expected.center_y()
            && rect.bottom() >= expected.center_y()
    })
}

/// the page count a source reports, for short assertions.
pub fn pages_of(source: &LopdfSource) -> u32 {
    source.page_count()
}
