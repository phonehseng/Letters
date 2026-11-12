//! helpers shared by the integration tests: hand-written test pdfs,
//! saving and reloading, and rendering pages through letters-pdf.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use letters_pdf::{page_pixel_size, PageSize, RenderWorker, TileKey, TILE_SIZE};
use lopdf::Document;

/// where the pdfium library lives for tests.
///
/// set LETTERS_PDFIUM_DIR to override; otherwise the folder filled by
/// tools/fetch-pdfium is used.
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

/// one worker for the whole test run: pdfium only allows one at a time
/// anyway, and starting it once keeps the tests quick.
fn worker() -> &'static RenderWorker {
    static WORKER: OnceLock<RenderWorker> = OnceLock::new();
    WORKER.get_or_init(|| RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap())
}

/// a fresh, empty folder for this test run's files.
fn temp_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir()
            .join("letters-annotate-tests")
            .join(format!("run-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    })
    .clone()
}

/// a page for a test pdf.
#[derive(Clone)]
pub struct TestPage {
    pub media_box: [f32; 4],
    pub crop_box: Option<[f32; 4]>,
    pub rotate: i64,
    /// drawing instructions for the page; /F1 is helvetica
    pub content: String,
    /// annotation dictionaries already on the page, as pdf source text,
    /// as another app would have written them
    pub annotations: Vec<String>,
}

pub const PAGE_WIDTH: f32 = 612.0;
pub const PAGE_HEIGHT: f32 = 792.0;

impl TestPage {
    /// a blank us letter page.
    pub fn letter() -> Self {
        Self {
            media_box: [0.0, 0.0, PAGE_WIDTH, PAGE_HEIGHT],
            crop_box: None,
            rotate: 0,
            content: String::new(),
            annotations: Vec::new(),
        }
    }

    pub fn rotated(mut self, degrees: i64) -> Self {
        self.rotate = degrees;
        self
    }

    pub fn cropped(mut self, crop_box: [f32; 4]) -> Self {
        self.crop_box = Some(crop_box);
        self
    }

    pub fn with_media_box(mut self, media_box: [f32; 4]) -> Self {
        self.media_box = media_box;
        self
    }

    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_owned();
        self
    }

    pub fn with_annotation(mut self, annotation: &str) -> Self {
        self.annotations.push(annotation.to_owned());
        self
    }
}

/// a us letter page with a black line of helvetica text at a known spot:
/// 24 point text whose baseline is 100 points down the stored page, from
/// 72 points in. "MMMMMMMMMM" is dark across almost its whole width.
pub fn text_page() -> TestPage {
    TestPage::letter().with_content("BT /F1 24 Tf 72 692 Td (MMMMMMMMMM) Tj ET")
}

/// writes a pdf by hand so the tests don't depend on the library they
/// are testing. objects: 1 catalog, 2 page tree, 3 helvetica, then for
/// each page its page object, its content and its annotations.
///
/// with `form_field`, page 0 also gets a text field widget (object
/// numbers after all pages) listed in the catalog's /AcroForm.
pub fn pdf_bytes(pages: &[TestPage], form_field: bool) -> Vec<u8> {
    let mut objects: BTreeMap<usize, String> = BTreeMap::new();
    let mut next = 4;
    let mut kids = Vec::new();
    let mut page_numbers = Vec::new();
    for page in pages {
        let page_number = next;
        let content_number = next + 1;
        next += 2;
        let mut annotation_refs: Vec<String> = Vec::new();
        for annotation in &page.annotations {
            objects.insert(
                next,
                annotation.replace("{page}", &format!("{page_number} 0 R")),
            );
            annotation_refs.push(format!("{next} 0 R"));
            next += 1;
        }
        kids.push(format!("{page_number} 0 R"));
        page_numbers.push((page_number, annotation_refs, content_number));
        objects.insert(
            content_number,
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                page.content.len(),
                page.content
            ),
        );
    }
    let widget_number = next;
    for ((page_number, mut annotation_refs, content_number), page) in
        page_numbers.into_iter().zip(pages)
    {
        if form_field && page_number == 4 {
            annotation_refs.push(format!("{widget_number} 0 R"));
        }
        let crop = page
            .crop_box
            .map(|[a, b, c, d]| format!(" /CropBox [{a} {b} {c} {d}]"))
            .unwrap_or_default();
        let [a, b, c, d] = page.media_box;
        let annots = if annotation_refs.is_empty() {
            String::new()
        } else {
            format!(" /Annots [{}]", annotation_refs.join(" "))
        };
        objects.insert(
            page_number,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [{a} {b} {c} {d}]{crop} /Rotate {} /Contents {content_number} 0 R /Resources << /Font << /F1 3 0 R >> >>{annots} >>",
                page.rotate
            ),
        );
    }
    let acro_form = if form_field {
        objects.insert(
            widget_number,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /Rect [300 700 500 720] /P 4 0 R /F 4 /DA (/Helv 12 Tf 0 g) >>".into(),
        );
        format!(" /AcroForm << /Fields [{widget_number} 0 R] /DA (/Helv 0 Tf 0 g) >>")
    } else {
        String::new()
    };
    objects.insert(1, format!("<< /Type /Catalog /Pages 2 0 R{acro_form} >>"));
    objects.insert(
        2,
        format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages.len(),
            kids.join(" ")
        ),
    );
    objects.insert(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    );
    write_objects(&objects)
}

/// lays objects out with the cross reference table pdf needs.
fn write_objects(objects: &BTreeMap<usize, String>) -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (number, body) in objects {
        assert_eq!(
            *number,
            offsets.len() + 1,
            "object numbers must have no gaps"
        );
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
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

pub fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).unwrap()
}

pub fn save(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// saves and loads again, as the app does between sessions.
pub fn reload(doc: &mut Document) -> Document {
    load(&save(doc))
}

/// the annotation dictionaries on a page, after following references.
pub fn page_annotations(doc: &Document, page: usize) -> Vec<lopdf::Dictionary> {
    let page_id = doc.get_pages()[&(page as u32 + 1)];
    doc.get_page_annotations(page_id)
        .unwrap()
        .into_iter()
        .cloned()
        .collect()
}

/// the annotation whose /NM is `id`.
pub fn annotation_by_name(doc: &Document, page: usize, id: &str) -> lopdf::Dictionary {
    page_annotations(doc, page)
        .into_iter()
        .find(|annotation| {
            annotation
                .get(b"NM")
                .ok()
                .and_then(|name| name.as_str().ok())
                .is_some_and(|name| name == id.as_bytes())
        })
        .unwrap_or_else(|| panic!("no annotation named {id}"))
}

pub fn name_of(dict: &lopdf::Dictionary, key: &[u8]) -> String {
    String::from_utf8_lossy(dict.get(key).unwrap().as_name().unwrap()).into_owned()
}

pub fn numbers_of(dict: &lopdf::Dictionary, key: &[u8]) -> Vec<f32> {
    dict.get(key)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_float().unwrap())
        .collect()
}

pub fn has_normal_appearance(doc: &Document, annotation: &lopdf::Dictionary) -> bool {
    let Ok(states) = annotation.get(b"AP").and_then(|ap| ap.as_dict()) else {
        return false;
    };
    let Ok(normal) = states.get(b"N").and_then(|normal| normal.as_reference()) else {
        return false;
    };
    doc.get_object(normal)
        .and_then(|object| object.as_stream())
        .is_ok()
}

/// a rendered page at one pixel per point, top left first, as shown.
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

    /// how many pixels differ by more than `tolerance` in any channel.
    pub fn differences(&self, other: &Image, tolerance: u8) -> usize {
        assert_eq!((self.width, self.height), (other.width, other.height));
        let (mine, theirs) = (self.rgba.as_chunks::<4>().0, other.rgba.as_chunks::<4>().0);
        mine.iter()
            .zip(theirs)
            .filter(|(a, b)| {
                a.iter()
                    .zip(b.iter())
                    .take(3)
                    .any(|(x, y)| x.abs_diff(*y) > tolerance)
            })
            .count()
    }
}

/// renders one page of a pdf through letters-pdf and pdfium, which
/// draws annotations from their appearance streams.
pub fn render(bytes: &[u8], page: u32) -> Image {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let path = temp_dir().join(format!(
        "render-{}.pdf",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, bytes).unwrap();
    let worker = worker();
    let info = worker.open(&path, None).unwrap();
    let size: PageSize = info.page_sizes[page as usize];
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
            let tile = decode_png(&worker.tile(key).unwrap());
            copy_tile(&mut rgba, width, &tile, column * TILE_SIZE, row * TILE_SIZE);
        }
    }
    worker.close(info.id);
    Image {
        width,
        height,
        rgba,
    }
}

/// where some text shows on a page, the way the app finds a selection:
/// one rectangle per line, in visible points from the top left.
pub fn find_text(bytes: &[u8], page: u32, text: &str) -> Vec<letters_annotate::Rect> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let path = temp_dir().join(format!(
        "search-{}.pdf",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, bytes).unwrap();
    let worker = worker();
    let info = worker.open(&path, None).unwrap();
    let results = worker.search(info.id, text).unwrap();
    worker.close(info.id);
    results
        .matches
        .into_iter()
        .filter(|found| found.page == page)
        .flat_map(|found| found.rects)
        .map(|rect| letters_annotate::Rect::new(rect.x, rect.y, rect.width, rect.height))
        .collect()
}

struct Tile {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode_png(bytes: &[u8]) -> Tile {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    Tile {
        width: info.width,
        height: info.height,
        rgba,
    }
}

fn copy_tile(page: &mut [u8], page_width: u32, tile: &Tile, left: u32, top: u32) {
    for y in 0..tile.height {
        let from = (y * tile.width * 4) as usize;
        let to = (((top + y) * page_width + left) * 4) as usize;
        let length = (tile.width * 4) as usize;
        page[to..to + length].copy_from_slice(&tile.rgba[from..from + length]);
    }
}

pub const WHITE: [u8; 3] = [255, 255, 255];

/// true when every channel is within `tolerance` of the expected colour.
pub fn near(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

/// asserts a colour with a readable message.
#[track_caller]
pub fn assert_near(actual: [u8; 3], expected: [u8; 3], tolerance: u8, what: &str) {
    assert!(
        near(actual, expected, tolerance),
        "{what}: expected about {expected:?}, found {actual:?}"
    );
}

pub fn is_dark(pixel: [u8; 3]) -> bool {
    pixel.iter().all(|channel| *channel < 90)
}

/// the four rotations every geometry test runs on, plus a cropped page
/// whose crop box starts away from zero and a page whose media box does.
pub fn page_variants() -> Vec<(&'static str, TestPage)> {
    vec![
        ("upright", text_page()),
        ("turned 90", text_page().rotated(90)),
        ("turned 180", text_page().rotated(180)),
        ("turned 270", text_page().rotated(270)),
        ("cropped", text_page().cropped([50.0, 80.0, 562.0, 742.0])),
        (
            "cropped and turned",
            text_page().cropped([50.0, 80.0, 562.0, 742.0]).rotated(90),
        ),
        (
            "shifted origin",
            TestPage::letter()
                .with_media_box([-100.0, 200.0, 512.0, 992.0])
                .with_content("BT /F1 24 Tf -28 892 Td (MMMMMMMMMM) Tj ET"),
        ),
    ]
}
