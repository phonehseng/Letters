//! helpers shared by the integration tests: pdfs written by hand, a tiny
//! TrueType font made from boxes, rendering through letters-pdf and
//! finding text with its search.

// each test file uses only some of these helpers
#![allow(dead_code)]

pub mod font;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use letters_pdf::{page_pixel_size, RenderWorker, TileKey, TILE_SIZE};
use lopdf::Document;

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

fn worker() -> &'static RenderWorker {
    static WORKER: OnceLock<RenderWorker> = OnceLock::new();
    WORKER.get_or_init(|| RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap())
}

fn temp_file(kind: &str) -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = DIR
        .get_or_init(|| {
            let dir = std::env::temp_dir()
                .join("letters-edit-tests")
                .join(format!("run-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            dir
        })
        .clone();
    dir.join(format!(
        "{kind}-{}.pdf",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// builds a pdf by hand, object by object, so the tests don't depend on
/// the library being tested to write correct files.
pub struct PdfBuilder {
    objects: Vec<Vec<u8>>,
    pages: Vec<usize>,
}

/// helvetica, WinAnsi, not embedded: object 3 in every built pdf.
pub const HELVETICA: usize = 3;

impl Default for PdfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfBuilder {
    pub fn new() -> Self {
        let mut builder = Self {
            objects: Vec::new(),
            pages: Vec::new(),
        };
        builder.add(b"catalog placeholder".to_vec());
        builder.add(b"pages placeholder".to_vec());
        builder.add(
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_vec(),
        );
        builder
    }

    /// adds an object and returns its number.
    pub fn add(&mut self, body: impl Into<Vec<u8>>) -> usize {
        self.objects.push(body.into());
        self.objects.len()
    }

    pub fn stream(&mut self, extra: &str, data: &[u8]) -> usize {
        let mut body = format!("<< {extra} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.add(body)
    }

    /// adds a page with the given content streams and resources (the
    /// inside of a resource dictionary, e.g. "/Font << /F1 3 0 R >>").
    pub fn page(&mut self, setup: &PageSetup, contents: &[&str], resources: &str) -> usize {
        let streams: Vec<usize> = contents
            .iter()
            .map(|content| self.stream("", content.as_bytes()))
            .collect();
        self.page_with_streams(setup, &streams, resources)
    }

    pub fn page_with_streams(
        &mut self,
        setup: &PageSetup,
        streams: &[usize],
        resources: &str,
    ) -> usize {
        let contents: String = streams
            .iter()
            .map(|stream| format!("{stream} 0 R "))
            .collect();
        let [a, b, c, d] = setup.media_box;
        let crop = setup
            .crop_box
            .map(|[a, b, c, d]| format!(" /CropBox [{a} {b} {c} {d}]"))
            .unwrap_or_default();
        let page = self.add(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{a} {b} {c} {d}]{crop} /Rotate {} /Contents [{contents}] /Resources << {resources} >> >>",
            setup.rotate
        ));
        self.pages.push(page);
        page
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
        let kids: String = self
            .pages
            .iter()
            .map(|page| format!("{page} 0 R "))
            .collect();
        self.objects[1] = format!(
            "<< /Type /Pages /Count {} /Kids [{kids}] >>",
            self.pages.len()
        )
        .into_bytes();
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in self.objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            bytes.extend_from_slice(body);
            bytes.extend_from_slice(b"\nendobj\n");
        }
        let table_start = bytes.len();
        let count = offsets.len() + 1;
        let mut table = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for offset in offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{table_start}\n%%EOF\n"
        ));
        bytes.extend_from_slice(table.as_bytes());
        bytes
    }
}

#[derive(Clone, Copy)]
pub struct PageSetup {
    pub media_box: [f32; 4],
    pub crop_box: Option<[f32; 4]>,
    pub rotate: i64,
}

impl PageSetup {
    pub fn letter() -> Self {
        Self {
            media_box: [0.0, 0.0, 612.0, 792.0],
            crop_box: None,
            rotate: 0,
        }
    }

    pub fn rotated(mut self, rotate: i64) -> Self {
        self.rotate = rotate;
        self
    }

    pub fn cropped(mut self, crop: [f32; 4]) -> Self {
        self.crop_box = Some(crop);
        self
    }
}

/// the four rotations and a cropped page with a box away from zero.
pub fn layouts() -> Vec<(&'static str, PageSetup)> {
    vec![
        ("upright", PageSetup::letter()),
        ("turned 90", PageSetup::letter().rotated(90)),
        ("turned 180", PageSetup::letter().rotated(180)),
        ("turned 270", PageSetup::letter().rotated(270)),
        (
            "cropped",
            PageSetup::letter().cropped([40.0, 60.0, 560.0, 760.0]),
        ),
    ]
}

pub fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).unwrap()
}

pub fn save(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

pub fn reload(doc: &mut Document) -> Document {
    load(&save(doc))
}

/// every place `text` appears on a page, found by pdfium's text search:
/// proof that new text is really there and readable, not just drawn.
/// each match gives one rectangle around all of it.
pub fn find_text(bytes: &[u8], page: u32, text: &str) -> Vec<letters_edit::Rect> {
    let path = temp_file("search");
    fs::write(&path, bytes).unwrap();
    let worker = worker();
    let info = worker.open(&path, None).unwrap();
    let results = worker.search(info.id, text).unwrap();
    worker.close(info.id);
    results
        .matches
        .into_iter()
        .filter(|found| found.page == page)
        .map(|found| {
            let left = found.rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
            let top = found.rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
            let right = found
                .rects
                .iter()
                .map(|r| r.x + r.width)
                .fold(f32::MIN, f32::max);
            let bottom = found
                .rects
                .iter()
                .map(|r| r.y + r.height)
                .fold(f32::MIN, f32::max);
            letters_edit::Rect::new(left, top, right - left, bottom - top)
        })
        .collect()
}

/// a rendered page at one pixel per point, as shown.
pub struct Image {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: f32, y: f32) -> [u8; 3] {
        let (x, y) = (x as u32, y as u32);
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is off the page"
        );
        let start = ((y * self.width + x) * 4) as usize;
        [self.rgba[start], self.rgba[start + 1], self.rgba[start + 2]]
    }

    /// how many pixels differ inside a region (left, top, right, bottom).
    pub fn differences_in(&self, other: &Image, region: (u32, u32, u32, u32)) -> usize {
        let (left, top, right, bottom) = region;
        (top..bottom.min(self.height))
            .flat_map(|y| (left..right.min(self.width)).map(move |x| (x, y)))
            .filter(|(x, y)| self.pixel(*x as f32, *y as f32) != other.pixel(*x as f32, *y as f32))
            .count()
    }

    /// the pixels in a region that pass a test.
    pub fn count(&self, region: (u32, u32, u32, u32), test: impl Fn([u8; 3]) -> bool) -> usize {
        let (left, top, right, bottom) = region;
        (top..bottom.min(self.height))
            .flat_map(|y| (left..right.min(self.width)).map(move |x| (x, y)))
            .filter(|(x, y)| test(self.pixel(*x as f32, *y as f32)))
            .count()
    }

    /// the leftmost column in a band of rows with any dark pixel.
    pub fn first_dark_column(&self, top: u32, bottom: u32) -> Option<u32> {
        (0..self.width).find(|x| {
            (top..bottom.min(self.height)).any(|y| is_dark(self.pixel(*x as f32, y as f32)))
        })
    }
}

pub fn is_dark(pixel: [u8; 3]) -> bool {
    pixel.iter().all(|channel| *channel < 110)
}

pub const WHITE: [u8; 3] = [255, 255, 255];

pub fn render(bytes: &[u8], page: u32) -> Image {
    let path = temp_file("render");
    fs::write(&path, bytes).unwrap();
    let worker = worker();
    let info = worker.open(&path, None).unwrap();
    let size = info.page_sizes[page as usize];
    let (width, height) = page_pixel_size(size, 1000);
    let mut rgba = vec![0; (width * height * 4) as usize];
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document: info.id,
                page,
                scale_milli: 1000,
                column,
                row,
            };
            let png = worker.tile(key).unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(&png[..]))
                .read_info()
                .unwrap();
            let mut tile = vec![0; reader.output_buffer_size().unwrap()];
            let frame = reader.next_frame(&mut tile).unwrap();
            for y in 0..frame.height {
                let from = (y * frame.width * 4) as usize;
                let to = (((row * TILE_SIZE + y) * width + column * TILE_SIZE) * 4) as usize;
                let length = (frame.width * 4) as usize;
                rgba[to..to + length].copy_from_slice(&tile[from..from + length]);
            }
        }
    }
    worker.close(info.id);
    Image {
        width,
        height,
        rgba,
    }
}

/// all the text of a page as letters-edit reads it, block by block.
pub fn page_text(doc: &Document, page: usize) -> Vec<String> {
    letters_edit::page_objects(doc, page)
        .unwrap()
        .text_blocks
        .into_iter()
        .map(|block| block.text)
        .collect()
}

/// the content streams of a page, decoded and joined.
pub fn page_content(doc: &Document, page: usize) -> String {
    let page_id = doc.get_pages()[&(page as u32 + 1)];
    String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned()
}
