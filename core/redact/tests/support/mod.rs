//! helpers shared by the integration tests: writing pdf files by hand,
//! drawing them with pdfium, and looking for a secret everywhere a secret
//! could hide.
//!
//! the files are written byte by byte so the tests do not depend on any
//! pdf writing library being correct, least of all the one under test.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use letters_pdf::{DocumentId, RenderWorker, TileKey, TILE_SIZE};
use letters_redact::lopdf::{Document, Object};

// ---- writing pdfs ----

/// a pdf being written: object bodies numbered from 1.
#[derive(Default)]
pub struct Pdf {
    bodies: Vec<Option<Vec<u8>>>,
}

impl Pdf {
    pub fn new() -> Self {
        Self::default()
    }

    /// keeps a number for an object written later with `set`.
    pub fn reserve(&mut self) -> usize {
        self.bodies.push(None);
        self.bodies.len()
    }

    pub fn set(&mut self, id: usize, body: impl Into<Vec<u8>>) {
        self.bodies[id - 1] = Some(body.into());
    }

    pub fn add(&mut self, body: impl Into<Vec<u8>>) -> usize {
        let id = self.reserve();
        self.set(id, body);
        id
    }

    /// a stream object; `dict` is the inside of its dictionary without /Length.
    pub fn add_stream(&mut self, dict: &str, data: &[u8]) -> usize {
        let id = self.reserve();
        self.set_stream(id, dict, data);
        id
    }

    pub fn set_stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.set(id, body);
    }

    /// a flate compressed stream.
    pub fn add_flate_stream(&mut self, dict: &str, data: &[u8]) -> usize {
        self.add_stream(&format!("{dict} /Filter /FlateDecode"), &deflate(data))
    }

    pub fn finish(&self, root: usize, trailer_extra: &str) -> Vec<u8> {
        let mut bytes = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in self.bodies.iter().enumerate() {
            offsets.push(bytes.len());
            let body = body
                .as_ref()
                .unwrap_or_else(|| panic!("object {} was reserved but never written", index + 1));
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
            "trailer\n<< /Size {count} /Root {root} 0 R {trailer_extra} >>\nstartxref\n{table_start}\n%%EOF\n"
        ));
        bytes.extend_from_slice(table.as_bytes());
        bytes
    }
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// one page of a test document.
pub struct PageSpec {
    /// the inside of the page's /Resources dictionary
    pub resources: String,
    /// its content, possibly split over several streams
    pub contents: Vec<Vec<u8>>,
    /// anything else for the page dictionary, e.g. "/Rotate 90"
    pub extra: String,
}

impl PageSpec {
    pub fn new(resources: &str, content: &str) -> Self {
        Self {
            resources: resources.to_owned(),
            contents: vec![content.as_bytes().to_vec()],
            extra: String::new(),
        }
    }

    pub fn with_extra(mut self, extra: &str) -> Self {
        self.extra = extra.to_owned();
        self
    }
}

/// writes the page tree and catalog for some pages and returns the
/// catalog's number. `catalog_extra` goes into the catalog dictionary.
pub fn add_pages(pdf: &mut Pdf, pages: Vec<PageSpec>, catalog_extra: &str) -> (usize, Vec<usize>) {
    let catalog = pdf.reserve();
    let tree = pdf.reserve();
    let mut page_ids = Vec::new();
    for page in pages {
        let streams: Vec<usize> = page
            .contents
            .iter()
            .map(|content| pdf.add_stream("", content))
            .collect();
        let contents = if streams.len() == 1 {
            format!("{} 0 R", streams[0])
        } else {
            format!(
                "[{}]",
                streams
                    .iter()
                    .map(|id| format!("{id} 0 R "))
                    .collect::<String>()
            )
        };
        page_ids.push(pdf.add(format!(
            "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /Resources << {} >> /Contents {contents} {} >>",
            page.resources, page.extra
        )));
    }
    let kids: String = page_ids.iter().map(|id| format!("{id} 0 R ")).collect();
    pdf.set(
        tree,
        format!(
            "<< /Type /Pages /Count {} /Kids [{kids}] >>",
            page_ids.len()
        ),
    );
    pdf.set(
        catalog,
        format!("<< /Type /Catalog /Pages {tree} 0 R {catalog_extra} >>"),
    );
    (catalog, page_ids)
}

/// a Helvetica font object, built into every reader.
pub fn add_helvetica(pdf: &mut Pdf) -> usize {
    pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
}

/// escapes a pdf literal string.
pub fn literal(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

// ---- files and pdfium ----

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

pub fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-redact-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

pub fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), 64 * 1024 * 1024).unwrap()
}

/// a whole page drawn by pdfium at `scale` pixels per point.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    rgb: Vec<[u8; 3]>,
}

impl Picture {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        self.rgb[(y * self.width + x) as usize]
    }
}

pub fn render_page(
    worker: &RenderWorker,
    document: DocumentId,
    page: u32,
    scale: f32,
    page_size: (f32, f32),
) -> Picture {
    let scale_milli = (scale * 1000.0) as u32;
    let width = (page_size.0 * scale).ceil() as u32;
    let height = (page_size.1 * scale).ceil() as u32;
    let mut rgb = vec![[0u8; 3]; (width * height) as usize];
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document,
                page,
                scale_milli,
                column,
                row,
            };
            let png = worker.tile(key).unwrap();
            let tile = decode_png(&png);
            for y in 0..tile.1 {
                for x in 0..tile.0 {
                    let page_x = column * TILE_SIZE + x;
                    let page_y = row * TILE_SIZE + y;
                    if page_x < width && page_y < height {
                        let at = ((y * tile.0 + x) * 4) as usize;
                        rgb[(page_y * width + page_x) as usize] =
                            [tile.2[at], tile.2[at + 1], tile.2[at + 2]];
                    }
                }
            }
        }
    }
    Picture { width, height, rgb }
}

fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    (info.width, info.height, rgba)
}

// ---- looking for secrets ----

/// every way the secret could be spelled in raw bytes.
fn spellings(secret: &str) -> Vec<Vec<u8>> {
    let utf16_be: Vec<u8> = secret.encode_utf16().flat_map(u16::to_be_bytes).collect();
    let utf16_le: Vec<u8> = secret.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let hex_upper: String = secret.bytes().map(|b| format!("{b:02X}")).collect();
    let hex_lower = hex_upper.to_lowercase();
    let utf16_hex: String = utf16_be.iter().map(|b| format!("{b:02X}")).collect();
    vec![
        secret.as_bytes().to_vec(),
        secret.to_lowercase().into_bytes(),
        utf16_be,
        utf16_le,
        hex_upper.into_bytes(),
        hex_lower.into_bytes(),
        utf16_hex.clone().into_bytes(),
        utf16_hex.to_lowercase().into_bytes(),
    ]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// (a): the raw file, and every stream in it inflated, holds the secret
/// in no spelling.
pub fn assert_not_in_bytes(file: &[u8], secret: &str) {
    for spelling in spellings(secret) {
        assert!(
            !contains(file, &spelling),
            "the raw file still holds {:?}",
            String::from_utf8_lossy(&spelling)
        );
    }
    for (index, inflated) in inflate_every_stream(file).iter().enumerate() {
        for spelling in spellings(secret) {
            assert!(
                !contains(inflated, &spelling),
                "inflated stream {index} still holds {:?}",
                String::from_utf8_lossy(&spelling)
            );
        }
    }
}

/// finds every `stream` keyword and tries to inflate what follows.
fn inflate_every_stream(file: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let mut position = 0;
    while let Some(offset) = file[position..]
        .windows(6)
        .position(|window| window == b"stream")
    {
        let mut start = position + offset + 6;
        while matches!(file.get(start), Some(b'\r' | b'\n')) {
            start += 1;
        }
        let mut decoder = flate2::read::ZlibDecoder::new(&file[start..]);
        let mut inflated = Vec::new();
        let _ = decoder.read_to_end(&mut inflated);
        if !inflated.is_empty() {
            found.push(inflated);
        }
        position = start;
    }
    found
}

/// (b): parsing the file and walking every object (strings, names,
/// decoded streams) finds no trace of the secret.
pub fn assert_not_in_objects(file: &[u8], secret: &str) {
    let doc = Document::load_mem(file).expect("the saved file opens");
    for (id, object) in &doc.objects {
        walk_object(object, &mut |bytes| {
            for spelling in spellings(secret) {
                assert!(
                    !contains(bytes, &spelling),
                    "object {id:?} still holds {:?}",
                    String::from_utf8_lossy(&spelling)
                );
            }
        });
    }
    walk_object(&Object::Dictionary(doc.trailer.clone()), &mut |bytes| {
        assert!(
            !contains(bytes, secret.as_bytes()),
            "the trailer still holds the secret"
        );
    });
}

fn walk_object(object: &Object, check: &mut dyn FnMut(&[u8])) {
    match object {
        Object::String(bytes, _) | Object::Name(bytes) => check(bytes),
        Object::Array(items) => items.iter().for_each(|item| walk_object(item, check)),
        Object::Dictionary(dict) => dict.iter().for_each(|(key, value)| {
            check(key);
            walk_object(value, check);
        }),
        Object::Stream(stream) => {
            stream
                .dict
                .iter()
                .for_each(|(_, value)| walk_object(value, check));
            check(&stream.content);
            if let Ok(decoded) = stream.decompressed_content() {
                check(&decoded);
            }
        }
        _ => {}
    }
}

/// (c): pdfium's own text extraction finds neither the secret nor any
/// four letter piece of it.
pub fn assert_not_found_by_pdfium(
    worker: &RenderWorker,
    path: &Path,
    password: Option<&str>,
    secret: &str,
) {
    let info = worker.open(path, password).unwrap();
    let mut pieces = vec![secret.to_owned()];
    let letters: Vec<char> = secret.chars().collect();
    for start in 0..letters.len().saturating_sub(3) {
        pieces.push(letters[start..start + 4].iter().collect());
    }
    for piece in pieces {
        let results = worker.search(info.id, &piece).unwrap();
        assert!(
            results.matches.is_empty(),
            "pdfium still finds {piece:?}: {:?}",
            results.matches
        );
    }
    worker.close(info.id);
}

/// all three checks at once.
pub fn assert_secret_gone(worker: &RenderWorker, dir: &Path, file: &[u8], secret: &str) {
    assert_not_in_bytes(file, secret);
    assert_not_in_objects(file, secret);
    let path = write(dir, "checked.pdf", file);
    assert_not_found_by_pdfium(worker, &path, None, secret);
}

// ---- comparing pictures ----

/// inside every box the new picture is exactly `fill`; outside them (with
/// a two pixel margin for the box's soft edge) it is exactly what it was.
/// boxes are in visible page points.
pub fn assert_pixels(
    old: &Picture,
    new: &Picture,
    rects: &[letters_redact::PageRect],
    scale: f64,
    fill: [u8; 3],
) {
    let inside = |x: u32, y: u32, margin: f64| {
        rects.iter().any(|rect| {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            px > rect.x * scale - margin
                && px < (rect.x + rect.width) * scale + margin
                && py > rect.y * scale - margin
                && py < (rect.y + rect.height) * scale + margin
        })
    };
    let mut changed_outside = 0;
    for y in 0..new.height {
        for x in 0..new.width {
            if inside(x, y, -1.5) {
                assert_eq!(
                    new.pixel(x, y),
                    fill,
                    "pixel ({x}, {y}) inside a box is not the fill colour"
                );
            } else if !inside(x, y, 2.0) && old.pixel(x, y) != new.pixel(x, y) {
                changed_outside += 1;
            }
        }
    }
    assert_eq!(changed_outside, 0, "pixels outside the boxes changed");
}
