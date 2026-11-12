//! helpers shared by the integration tests: pdf files written by hand, and
//! rendering through letters-pdf to look at real pixels.

// each test file uses only some of these helpers
#![allow(dead_code)]

pub mod tsa;

use std::fs;
use std::path::{Path, PathBuf};

use letters_forms::VisibleRect;
use letters_pdf::{RenderWorker, TileKey, TILE_SIZE};
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

/// a fresh, empty folder for one test's files.
pub fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-forms-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// builds the bytes of a pdf object by object. the file is written by hand
/// so the tests do not depend on any pdf writing library being correct.
#[derive(Default)]
pub struct RawPdf {
    bodies: Vec<Option<String>>,
}

impl RawPdf {
    pub fn new() -> Self {
        Self::default()
    }

    /// keeps an object number for an object written later with `set`.
    pub fn reserve(&mut self) -> usize {
        self.bodies.push(None);
        self.bodies.len()
    }

    pub fn add(&mut self, body: impl Into<String>) -> usize {
        self.bodies.push(Some(body.into()));
        self.bodies.len()
    }

    pub fn set(&mut self, number: usize, body: impl Into<String>) {
        self.bodies[number - 1] = Some(body.into());
    }

    /// adds a stream object with the given extra dictionary entries.
    pub fn add_stream(&mut self, dict_entries: &str, content: &str) -> usize {
        self.add(stream_body(dict_entries, content))
    }

    pub fn finish(&self, root: usize) -> Vec<u8> {
        let mut bytes = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in self.bodies.iter().enumerate() {
            offsets.push(bytes.len());
            let body = body.as_deref().expect("every reserved object must be set");
            bytes.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
        }
        let table_start = bytes.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
        for offset in offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {} /Root {root} 0 R >>\nstartxref\n{table_start}\n%%EOF\n",
            self.bodies.len() + 1
        ));
        bytes.extend_from_slice(table.as_bytes());
        bytes
    }
}

pub fn stream_body(dict_entries: &str, content: &str) -> String {
    format!(
        "<< {dict_entries} /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

/// a one page document with the given page content and nothing else.
pub struct PageSetup {
    pub media_box: [f32; 4],
    pub crop_box: Option<[f32; 4]>,
    pub rotate: i32,
    pub content: String,
}

impl PageSetup {
    pub fn letter(rotate: i32) -> Self {
        Self {
            media_box: [0.0, 0.0, 612.0, 792.0],
            crop_box: None,
            rotate,
            content: String::new(),
        }
    }
}

fn boxes(page: &PageSetup) -> String {
    let [a, b, c, d] = page.media_box;
    let mut out = format!("/MediaBox [{a} {b} {c} {d}]");
    if let Some([a, b, c, d]) = page.crop_box {
        out.push_str(&format!(" /CropBox [{a} {b} {c} {d}]"));
    }
    out
}

/// a plain page with text and lines but no form, for detection and stamps.
pub fn plain_page_pdf(page: &PageSetup) -> Vec<u8> {
    let mut pdf = RawPdf::new();
    let catalog = pdf.add("<< /Type /Catalog /Pages 2 0 R >>");
    pdf.add("<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    pdf.add(format!(
        "<< /Type /Page /Parent 2 0 R {} /Rotate {} /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        boxes(page),
        page.rotate
    ));
    pdf.add_stream("", &page.content);
    pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
    pdf.finish(catalog)
}

/// one page with a form holding one field of every kind, written by hand
/// the way other pdf programs write them. field rectangles (user space):
///
/// | field              | kind                         | rect                |
/// |--------------------|------------------------------|---------------------|
/// | name               | text, max 20, tooltip        | 100 700 300 720     |
/// | notes              | multiline, auto size         | 100 560 300 660     |
/// | agree              | checkbox, on state "Yes"     | 100 520 115 535     |
/// | colour             | radio: Red, Green, Blue      | 100/130/160 480 +15 |
/// | country            | dropdown with labels         | 100 440 250 460     |
/// | subjects           | list, multi-select           | 300 380 450 460     |
/// | born               | date dd/mm/yyyy              | 100 400 250 420     |
/// | code               | comb, 6 cells                | 100 360 220 380     |
/// | locked             | read-only text "fixed"       | 300 700 450 720     |
/// | must               | required text                | 300 660 450 680     |
/// | address.street     | text, type and /DA inherited | 100 320 300 340     |
/// | address.city       | text, centred                | 100 290 300 310     |
/// | sign_here          | signature                    | 300 200 500 260     |
pub fn sample_form(page: &PageSetup) -> Vec<u8> {
    let mut pdf = RawPdf::new();
    let catalog = pdf.add("<< /Type /Catalog /Pages 2 0 R /AcroForm 6 0 R >>");
    pdf.add("<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    let page_number = pdf.reserve(); // 3
    pdf.add_stream("", &page.content); // 4
    pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"); // 5
    let form = pdf.reserve(); // 6
    let mut fields = Vec::new();
    let mut annots = Vec::new();

    // a field that is its own widget, listed at the top of the form
    let top = |pdf: &mut RawPdf, annots: &mut Vec<usize>, fields: &mut Vec<usize>, body: &str| {
        fields.push(widget(pdf, annots, body));
    };
    top(&mut pdf, &mut annots, &mut fields, "/FT /Tx /T (name) /TU (Your full name) /MaxLen 20 /DA (/Helv 12 Tf 0 g) /Rect [100 700 300 720]");
    top(
        &mut pdf,
        &mut annots,
        &mut fields,
        "/FT /Tx /T (notes) /Ff 4096 /DA (/Helv 0 Tf 0 g) /Rect [100 560 300 660]",
    );
    let on = pdf.add_stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 15 15]",
        "0 g 2 2 11 11 re f",
    );
    let off = pdf.add_stream("/Type /XObject /Subtype /Form /BBox [0 0 15 15]", "");
    top(
        &mut pdf,
        &mut annots,
        &mut fields,
        &format!("/FT /Btn /T (agree) /V /Off /AS /Off /DA (/ZaDb 0 Tf 0 g) /Rect [100 520 115 535] /AP << /N << /Yes {on} 0 R /Off {off} 0 R >> >>"),
    );
    let radio = pdf.reserve();
    fields.push(radio);
    let mut kids = Vec::new();
    for (index, colour) in ["Red", "Green", "Blue"].iter().enumerate() {
        let x = 100 + 30 * index;
        kids.push(widget(
            &mut pdf,
            &mut annots,
            &format!("/Parent {radio} 0 R /AS /Off /MK << /BC [0 0 0] >> /Rect [{x} 480 {} 495] /AP << /N << /{colour} {on} 0 R /Off {off} 0 R >> >>", x + 15),
        ));
    }
    pdf.set(
        radio,
        format!(
            "<< /FT /Btn /T (colour) /Ff 49152 /V /Off /DA (/Helv 0 Tf 0 g) /Kids [{}] >>",
            refs(&kids)
        ),
    );
    top(&mut pdf, &mut annots, &mut fields, "/FT /Ch /T (country) /Ff 131072 /DA (/Helv 10 Tf 0 g) /Opt [[(fr) (France)] [(de) (Germany)] [(nz) (New Zealand)]] /Rect [100 440 250 460]");
    top(&mut pdf, &mut annots, &mut fields, "/FT /Ch /T (subjects) /Ff 2097152 /DA (/Helv 10 Tf 0 g) /Opt [(Maths) (Art) (History) (Music)] /Rect [300 380 450 460]");
    top(&mut pdf, &mut annots, &mut fields, "/FT /Tx /T (born) /DA (/Helv 10 Tf 0 g) /AA << /F << /S /JavaScript /JS (AFDate_FormatEx\\(\"dd/mm/yyyy\"\\);) >> >> /Rect [100 400 250 420]");
    top(&mut pdf, &mut annots, &mut fields, "/FT /Tx /T (code) /Ff 16777216 /MaxLen 6 /DA (/Helv 12 Tf 0 g) /MK << /BC [0 0 0] >> /Rect [100 360 220 380]");
    top(
        &mut pdf,
        &mut annots,
        &mut fields,
        "/FT /Tx /T (locked) /Ff 1 /V (fixed) /DA (/Helv 10 Tf 0 g) /Rect [300 700 450 720]",
    );
    top(
        &mut pdf,
        &mut annots,
        &mut fields,
        "/FT /Tx /T (must) /Ff 2 /DA (/Helv 10 Tf 0 g) /Rect [300 660 450 680]",
    );
    let address = pdf.reserve();
    fields.push(address);
    let street = widget(
        &mut pdf,
        &mut annots,
        &format!("/Parent {address} 0 R /T (street) /Rect [100 320 300 340]"),
    );
    let city = widget(
        &mut pdf,
        &mut annots,
        &format!("/Parent {address} 0 R /T (city) /Q 1 /Rect [100 290 300 310]"),
    );
    pdf.set(
        address,
        format!(
            "<< /T (address) /FT /Tx /DA (/Helv 11 Tf 0 0 1 rg) /Kids [{street} 0 R {city} 0 R] >>"
        ),
    );
    top(
        &mut pdf,
        &mut annots,
        &mut fields,
        "/FT /Sig /T (sign_here) /Rect [300 200 500 260]",
    );

    pdf.set(
        page_number,
        format!(
            "<< /Type /Page /Parent 2 0 R {} /Rotate {} /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [{}] >>",
            boxes(page),
            page.rotate,
            refs(&annots)
        ),
    );
    pdf.set(
        form,
        format!(
            "<< /Fields [{}] /DR << /Font << /Helv 5 0 R >> >> /DA (/Helv 0 Tf 0 g) >>",
            refs(&fields)
        ),
    );
    pdf.finish(catalog)
}

/// adds a widget annotation on page object 3 and remembers it for /Annots.
fn widget(pdf: &mut RawPdf, annots: &mut Vec<usize>, body: &str) -> usize {
    let number = pdf.add(format!(
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 {body} >>"
    ));
    annots.push(number);
    number
}

fn refs(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(|number| format!("{number} 0 R"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn load(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("the test pdf should load")
}

pub fn save(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// saves and loads again, so tests see only what reached the file.
pub fn round_trip(doc: &mut Document) -> Document {
    load(&save(doc))
}

/// a whole rendered page, so tests can look at single pixels.
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    /// pixels per point
    pub scale: f32,
    rgba: Vec<u8>,
}

impl PageImage {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let start = ((y * self.width + x) * 4) as usize;
        [self.rgba[start], self.rgba[start + 1], self.rgba[start + 2]]
    }

    /// how many pixels inside a rectangle (in points from the visible top
    /// left) are clearly darker than white.
    pub fn ink_in(&self, rect: VisibleRect) -> usize {
        let x0 = (rect.x * self.scale).max(0.0) as u32;
        let y0 = (rect.y * self.scale).max(0.0) as u32;
        let x1 = ((rect.right() * self.scale) as u32).min(self.width);
        let y1 = ((rect.bottom() * self.scale) as u32).min(self.height);
        let mut count = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let [r, g, b] = self.pixel(x, y);
                if (r as u32 + g as u32 + b as u32) < 3 * 200 {
                    count += 1;
                }
            }
        }
        count
    }

    /// where two renders differ, as (x, y) pixel positions.
    pub fn differences(&self, other: &PageImage) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let (a, b) = (self.pixel(x, y), other.pixel(x, y));
                if a.iter().zip(b.iter()).any(|(p, q)| p.abs_diff(*q) > 40) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// how many pixels differ between two renders of the same page.
    pub fn differing_pixels(&self, other: &PageImage) -> usize {
        assert_eq!((self.width, self.height), (other.width, other.height));
        self.rgba
            .chunks(4)
            .zip(other.rgba.chunks(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 40))
            .count()
    }
}

/// renders one page of a pdf at `scale_milli` thousandths of a pixel per
/// point, putting its tiles together into one picture.
pub fn render_page(bytes: &[u8], page: u32, scale_milli: u32, test_name: &str) -> PageImage {
    let dir = temp_dir(test_name);
    let path = dir.join("render.pdf");
    fs::write(&path, bytes).unwrap();
    let worker = RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap();
    let info = worker.open(&path, None).unwrap();
    let size = info.page_sizes[page as usize];
    let scale = scale_milli as f32 / 1000.0;
    let width = (size.width_pt * scale).ceil() as u32;
    let height = (size.height_pt * scale).ceil() as u32;
    let mut rgba = vec![255u8; (width * height * 4) as usize];
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
            let (tile_width, tile_height, pixels) = decode_png(&png);
            for y in 0..tile_height {
                for x in 0..tile_width {
                    let target =
                        (((row * TILE_SIZE + y) * width + column * TILE_SIZE + x) * 4) as usize;
                    let source = ((y * tile_width + x) * 4) as usize;
                    rgba[target..target + 4].copy_from_slice(&pixels[source..source + 4]);
                }
            }
        }
    }
    worker.close(info.id);
    PageImage {
        width,
        height,
        scale,
        rgba,
    }
}

fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    (info.width, info.height, rgba)
}

/// a small png with a fully see-through background and an opaque dark
/// stroke across the middle, like a drawn signature.
pub fn signature_png(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for x in 0..width {
        for dy in 0..(height / 5).max(1) {
            let y = height / 2 - height / 10 + dy;
            let index = ((y * width + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&[10, 20, 120, 255]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
    out
}
