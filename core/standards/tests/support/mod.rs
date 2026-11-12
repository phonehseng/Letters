//! helpers shared by the standards tests: small documents built with
//! lopdf, rendering through letters-pdf, qpdf's structural check through
//! pikepdf, and a font from letters-fonts' test fixtures.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use letters_pdf::{RenderWorker, TileKey, TILE_SIZE};
use letters_standards::{FontFile, FontRequest, FontSource, PageRenderer, RenderedPage};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};

/// a one page letter-sized document drawing `content` with `resources`.
pub fn one_page(content: &str, resources: Dictionary) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(resources),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
        }),
    );
    let catalog =
        doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    doc
}

/// red and blue boxes and nothing else: no fonts, rgb colour.
pub fn boxes() -> Document {
    one_page(
        "1 0 0 rg 72 600 200 100 re f 0 0 1 RG 4 w 300 300 150 150 re S",
        Dictionary::new(),
    )
}

/// a page of text in helvetica, not embedded.
pub fn helvetica_text() -> Document {
    let font = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" };
    let resources = dictionary! { "Font" => dictionary! { "F1" => font } };
    one_page(
        "BT /F1 24 Tf 72 700 Td (Hello, archive) Tj ET 0 0 1 rg 72 600 100 50 re f",
        resources,
    )
}

/// saves with a classic cross-reference table, the way most files are.
pub fn save(doc: &mut Document) -> Vec<u8> {
    doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-standards-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// the font bundled as a letters-fonts test fixture (open font licence).
pub fn atkinson() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fonts/tests/fixtures/AtkinsonHyperlegible-Regular.ttf");
    std::fs::read(path).unwrap()
}

/// the same font with its licence set to "restricted" (fsType 2), which
/// forbids embedding.
pub fn restricted_font() -> Vec<u8> {
    let mut data = atkinson();
    let tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for table in 0..tables {
        let entry = 12 + table * 16;
        if &data[entry..entry + 4] == b"OS/2" {
            let offset =
                u32::from_be_bytes(data[entry + 8..entry + 12].try_into().unwrap()) as usize;
            data[offset + 8..offset + 10].copy_from_slice(&2u16.to_be_bytes());
        }
    }
    data
}

/// a font source that offers one font file for every request.
pub struct OneFont {
    pub data: Vec<u8>,
    pub family: &'static str,
    pub requests: std::cell::RefCell<Vec<FontRequest>>,
}

impl OneFont {
    pub fn new(data: Vec<u8>, family: &'static str) -> Self {
        Self {
            data,
            family,
            requests: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl FontSource for OneFont {
    fn find(&self, request: &FontRequest) -> Option<FontFile> {
        self.requests.borrow_mut().push(request.clone());
        Some(FontFile {
            data: self.data.clone(),
            index: 0,
            family: self.family.to_owned(),
            same_widths: false,
        })
    }
}

pub fn pdfium_dir() -> PathBuf {
    match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    }
}

/// a rendered page as rgb.
pub fn render(bytes: &[u8], page: u32, pixels_per_point: f32, name: &str) -> RenderedPage {
    let dir = temp_dir(name);
    let path = dir.join("render.pdf");
    std::fs::write(&path, bytes).unwrap();
    let worker = RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap();
    let info = worker.open(&path, None).unwrap();
    let size = info.page_sizes[page as usize];
    let scale_milli = (pixels_per_point * 1000.0) as u32;
    let scale = scale_milli as f32 / 1000.0;
    let width = (size.width_pt * scale).ceil() as u32;
    let height = (size.height_pt * scale).ceil() as u32;
    let mut rgb = vec![255u8; (width * height * 3) as usize];
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document: info.id,
                page,
                scale_milli,
                column,
                row,
            };
            let png = worker.tile(key).unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(png.as_slice()))
                .read_info()
                .unwrap();
            let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
            let frame = reader.next_frame(&mut rgba).unwrap();
            for y in 0..frame.height {
                for x in 0..frame.width {
                    let (tx, ty) = (column * TILE_SIZE + x, row * TILE_SIZE + y);
                    if tx >= width || ty >= height {
                        continue;
                    }
                    let source = ((y * frame.width + x) * 4) as usize;
                    let target = ((ty * width + tx) * 3) as usize;
                    rgb[target..target + 3].copy_from_slice(&rgba[source..source + 3]);
                }
            }
        }
    }
    worker.close(info.id);
    RenderedPage { width, height, rgb }
}

/// how many pixels differ clearly between two renders.
pub fn differing_pixels(a: &RenderedPage, b: &RenderedPage) -> usize {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.rgb
        .chunks(3)
        .zip(b.rgb.chunks(3))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 40))
        .count()
}

/// renders the original document's pages for pdf/a-1 rasterising.
pub struct PdfiumRenderer {
    pub original: Vec<u8>,
}

impl PageRenderer for PdfiumRenderer {
    fn render(&self, page: u32, pixels_per_point: f32) -> Option<RenderedPage> {
        Some(render(&self.original, page, pixels_per_point, "rasterise"))
    }
}

/// qpdf's structural check (through pikepdf in the fidelity tools' venv).
/// none when it isn't installed; otherwise the problems it lists.
pub fn qpdf_problems(bytes: &[u8], name: &str) -> Option<Vec<String>> {
    let python =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fidelity/.venv/Scripts/python.exe");
    if !python.exists() {
        return None;
    }
    let dir = temp_dir(&format!("qpdf-{name}"));
    let path = dir.join("check.pdf");
    std::fs::write(&path, bytes).unwrap();
    // qpdf --check, run through pikepdf's job interface; exit 0 means no
    // syntax or stream encoding errors and no warnings
    let script = "import sys, pikepdf\njob = pikepdf.Job([\"qpdf\", \"--check\", sys.argv[1]])\njob.run()\nprint(\"EXIT\", job.exit_code)\n";
    let output = Command::new(python)
        .arg("-I")
        .arg("-c")
        .arg(script)
        .arg(&path)
        .output()
        .ok()?;
    if !output.status.success() {
        return Some(vec![String::from_utf8_lossy(&output.stderr).into_owned()]);
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let clean = text.contains("EXIT 0");
    let problems: Vec<String> = text
        .lines()
        .filter(|line| line.contains("WARNING") || line.contains("ERROR") || line.contains("error"))
        .map(str::to_owned)
        .collect();
    if clean && problems.is_empty() {
        return Some(Vec::new());
    }
    Some(if problems.is_empty() {
        vec![text]
    } else {
        problems
    })
}

/// asserts qpdf finds nothing wrong (skipped with a note when it isn't installed).
pub fn assert_qpdf_clean(bytes: &[u8], name: &str) {
    match qpdf_problems(bytes, name) {
        Some(problems) => assert!(
            problems.is_empty(),
            "qpdf found problems in {name}: {problems:?}"
        ),
        None => eprintln!("pikepdf was not found; skipping qpdf's check of {name}"),
    }
}
