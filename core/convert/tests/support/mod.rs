//! helpers shared by the integration tests: pdfs written by hand with
//! known content, and a page renderer built on letters-pdf.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use letters_convert::{ConvertError, PageRenderer, RgbaImage};
use letters_pdf::{page_pixel_size, DocumentId, PageSize, RenderWorker, TileKey, TILE_SIZE};

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
        .join("letters-convert-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// draws pages with letters-pdf, stitching its 512 pixel tiles together.
pub struct TestRenderer {
    worker: RenderWorker,
    open: Option<(PathBuf, DocumentId, Vec<PageSize>)>,
    pub pages_drawn: usize,
}

impl TestRenderer {
    pub fn new() -> Self {
        Self {
            worker: RenderWorker::start(&pdfium_dir(), 8 * 1024 * 1024).unwrap(),
            open: None,
            pages_drawn: 0,
        }
    }

    fn document(&mut self, pdf: &Path) -> Result<(DocumentId, Vec<PageSize>), ConvertError> {
        if let Some((path, id, sizes)) = &self.open {
            if path == pdf {
                return Ok((*id, sizes.clone()));
            }
            self.worker.close(*id);
        }
        let info = self
            .worker
            .open(pdf, None)
            .map_err(|error| ConvertError::RenderFailed(error.to_string()))?;
        self.open = Some((pdf.to_path_buf(), info.id, info.page_sizes.clone()));
        Ok((info.id, info.page_sizes))
    }
}

impl Drop for TestRenderer {
    fn drop(&mut self) {
        if let Some((_, id, _)) = self.open.take() {
            self.worker.close(id);
        }
    }
}

impl PageRenderer for TestRenderer {
    fn render(&mut self, pdf: &Path, page: usize, scale: f32) -> Result<RgbaImage, ConvertError> {
        // the same file may have been rewritten since it was opened (a
        // background copy is made for each page), so open it afresh
        if let Some((_, id, _)) = self.open.take() {
            self.worker.close(id);
        }
        let (id, sizes) = self.document(pdf)?;
        let scale_milli = (scale * 1000.0).round() as u32;
        let size = sizes[page];
        let (width, height) = page_pixel_size(size, scale_milli);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for row in 0..height.div_ceil(TILE_SIZE) {
            for column in 0..width.div_ceil(TILE_SIZE) {
                let key = TileKey {
                    document: id,
                    page: page as u32,
                    scale_milli,
                    column,
                    row,
                };
                let tile = self
                    .worker
                    .tile(key)
                    .map_err(|error| ConvertError::RenderFailed(error.to_string()))?;
                let tile = decode_png(&tile);
                copy_tile(
                    &mut pixels,
                    width,
                    &tile,
                    column * TILE_SIZE,
                    row * TILE_SIZE,
                );
            }
        }
        self.pages_drawn += 1;
        RgbaImage::new(width, height, pixels)
    }
}

fn copy_tile(page: &mut [u8], page_width: u32, tile: &DecodedImage, x: u32, y: u32) {
    for row in 0..tile.height {
        let from = (row * tile.width * 4) as usize;
        let to = (((y + row) * page_width + x) * 4) as usize;
        let length = (tile.width * 4) as usize;
        page[to..to + length].copy_from_slice(&tile.pixels[from..from + length]);
    }
}

/// a decoded picture with a way to read single pixels.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// bytes per pixel: 1 gray, 3 rgb, 4 rgba
    pub channels: usize,
    pub pixels: Vec<u8>,
}

impl DecodedImage {
    pub fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let start = (y as usize * self.width as usize + x as usize) * self.channels;
        match self.channels {
            1 => [self.pixels[start]; 3],
            _ => [
                self.pixels[start],
                self.pixels[start + 1],
                self.pixels[start + 2],
            ],
        }
    }
}

pub fn decode_png(bytes: &[u8]) -> DecodedImage {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    DecodedImage {
        width: info.width,
        height: info.height,
        channels: info.color_type.samples(),
        pixels,
    }
}

/// the standard fonts every pdf reader has built in, so no font file is
/// needed. each gets a resource name F1 to F7.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    Courier,
}

const FONTS: [(Font, &str); 7] = [
    (Font::Helvetica, "Helvetica"),
    (Font::HelveticaBold, "Helvetica-Bold"),
    (Font::HelveticaOblique, "Helvetica-Oblique"),
    (Font::TimesRoman, "Times-Roman"),
    (Font::TimesBold, "Times-Bold"),
    (Font::TimesItalic, "Times-Italic"),
    (Font::Courier, "Courier"),
];

fn font_resource(font: Font) -> String {
    let index = FONTS.iter().position(|(f, _)| *f == font).unwrap();
    format!("F{}", index + 1)
}

/// a small rgb picture to place on a page.
#[derive(Clone)]
pub struct TestImage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl TestImage {
    pub fn solid(width: u32, height: u32, color: [u8; 3]) -> Self {
        Self {
            width,
            height,
            rgb: color.repeat((width * height) as usize),
        }
    }
}

/// one page of a hand-written pdf. positions are in points from the top
/// left of the page, y going down, like everything the crate reports.
pub struct TestPage {
    pub width: f32,
    pub height: f32,
    content: String,
    images: Vec<TestImage>,
    links: Vec<([f32; 4], String)>,
}

impl TestPage {
    pub fn letter() -> Self {
        Self::sized(612.0, 792.0)
    }

    pub fn sized(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            content: String::new(),
            images: Vec::new(),
            links: Vec::new(),
        }
    }

    /// a line of black text with its baseline `baseline` points from the top.
    pub fn text(&mut self, font: Font, size: f32, x: f32, baseline: f32, text: &str) -> &mut Self {
        self.colored_text(font, size, x, baseline, text, [0, 0, 0])
    }

    pub fn colored_text(
        &mut self,
        font: Font,
        size: f32,
        x: f32,
        baseline: f32,
        text: &str,
        color: [u8; 3],
    ) -> &mut Self {
        let [r, g, b] = color.map(|c| f32::from(c) / 255.0);
        self.content.push_str(&format!(
            "BT /{} {size} Tf {r} {g} {b} rg {x} {} Td ({}) Tj ET\n",
            font_resource(font),
            self.height - baseline,
            escape(text)
        ));
        self
    }

    /// invisible text (render mode 3), like the searchable layer of a scan.
    pub fn invisible_text(&mut self, size: f32, x: f32, baseline: f32, text: &str) -> &mut Self {
        self.content.push_str(&format!(
            "BT 3 Tr /F1 {size} Tf {x} {} Td ({}) Tj ET\n",
            self.height - baseline,
            escape(text)
        ));
        self
    }

    /// a straight stroked line.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) -> &mut Self {
        self.content.push_str(&format!(
            "q 0 0 0 RG {width} w {x0} {} m {x1} {} l S Q\n",
            self.height - y0,
            self.height - y1
        ));
        self
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 3]) -> &mut Self {
        let [r, g, b] = color.map(|c| f32::from(c) / 255.0);
        self.content.push_str(&format!(
            "q {r} {g} {b} rg {x} {} {w} {h} re f Q\n",
            self.height - y - h
        ));
        self
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32) -> &mut Self {
        self.content.push_str(&format!(
            "q 0 0 0 RG {width} w {x} {} {w} {h} re S Q\n",
            self.height - y - h
        ));
        self
    }

    /// a curved shape (a circle made of four bezier curves), which no
    /// office format can take as a simple shape.
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 3]) -> &mut Self {
        let [red, green, blue] = color.map(|c| f32::from(c) / 255.0);
        let y = self.height - cy;
        let k = r * 0.5523;
        self.content.push_str(&format!(
            "q {red} {green} {blue} rg {} {y} m {} {} {} {} {cx} {} c {} {} {} {} {} {y} c {} {} {} {} {cx} {} c {} {} {} {} {} {y} c f Q\n",
            cx + r,
            cx + r, y + k, cx + k, y + r, y + r,
            cx - k, y + r, cx - r, y + k, cx - r,
            cx - r, y - k, cx - k, y - r, y - r,
            cx + k, y - r, cx + r, y - k, cx + r,
        ));
        self
    }

    /// a picture drawn into the box given in points.
    pub fn image(&mut self, x: f32, y: f32, w: f32, h: f32, image: TestImage) -> &mut Self {
        self.images.push(image);
        self.content.push_str(&format!(
            "q {w} 0 0 {h} {x} {} cm /Im{} Do Q\n",
            self.height - y - h,
            self.images.len()
        ));
        self
    }

    /// a link annotation over the box, leading to a web address.
    pub fn link(&mut self, x: f32, y: f32, w: f32, h: f32, uri: &str) -> &mut Self {
        let bottom = self.height - y - h;
        self.links
            .push(([x, bottom, x + w, bottom + h], uri.to_string()));
        self
    }
}

fn escape(text: &str) -> String {
    // the standard fonts use winansi, which holds latin-1 here
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\\' | '(' | ')' => {
                out.push('\\');
                out.push(c);
            }
            '•' => out.push_str("\\225"),
            '–' => out.push_str("\\226"),
            c if (c as u32) < 128 => out.push(c),
            c if (c as u32) < 256 => out.push_str(&format!("\\{:03o}", c as u32)),
            _ => out.push('?'),
        }
    }
    out
}

/// writes the pages as a pdf. the file is written by hand so the tests
/// don't depend on any pdf writing library being correct.
pub fn write_pdf(path: &Path, pages: &[TestPage]) {
    fs::write(path, pdf_bytes(pages, None)).unwrap();
}

/// a pdf whose trailer says it is encrypted, which every reader refuses
/// to read without the password.
pub fn write_encrypted_pdf(path: &Path) {
    let mut page = TestPage::letter();
    page.text(Font::Helvetica, 12.0, 72.0, 72.0, "Secret");
    fs::write(path, pdf_bytes(&[page], Some(()))).unwrap();
}

/// a pdf with an empty page tree.
pub fn write_empty_pdf(path: &Path) {
    let mut pdf = PdfWriter::default();
    pdf.object("<< /Type /Catalog /Pages 2 0 R >>".into());
    pdf.object("<< /Type /Pages /Count 0 /Kids [] >>".into());
    fs::write(path, pdf.finish(None)).unwrap();
}

fn pdf_bytes(pages: &[TestPage], encrypted: Option<()>) -> Vec<u8> {
    let mut pdf = PdfWriter::default();
    // objects: 1 catalog, 2 pages, 3..9 fonts, then per page: page,
    // content, images, links
    pdf.object("<< /Type /Catalog /Pages 2 0 R >>".into());
    let mut page_numbers = Vec::new();
    let mut next = 3 + FONTS.len();
    for page in pages {
        page_numbers.push(next);
        next += 2 + page.images.len() + page.links.len();
    }
    let kids: String = page_numbers.iter().map(|n| format!("{n} 0 R ")).collect();
    pdf.object(format!(
        "<< /Type /Pages /Count {} /Kids [ {kids}] >>",
        pages.len()
    ));
    for (_, name) in FONTS {
        pdf.object(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>"
        ));
    }
    let fonts: String = (0..FONTS.len())
        .map(|i| format!("/F{} {} 0 R ", i + 1, 3 + i))
        .collect();
    for (page, number) in pages.iter().zip(&page_numbers) {
        let content_number = number + 1;
        let image_numbers: Vec<usize> = (0..page.images.len())
            .map(|i| content_number + 1 + i)
            .collect();
        let link_numbers: Vec<usize> = (0..page.links.len())
            .map(|i| content_number + 1 + page.images.len() + i)
            .collect();
        let xobjects: String = image_numbers
            .iter()
            .enumerate()
            .map(|(i, n)| format!("/Im{} {n} 0 R ", i + 1))
            .collect();
        let annots: String = link_numbers.iter().map(|n| format!("{n} 0 R ")).collect();
        pdf.object(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {content_number} 0 R /Resources << /Font << {fonts}>> /XObject << {xobjects}>> >> /Annots [ {annots}] >>",
            page.width, page.height
        ));
        pdf.stream("", page.content.as_bytes());
        for image in &page.images {
            pdf.stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8",
                    image.width, image.height
                ),
                &image.rgb,
            );
        }
        for (rect, uri) in &page.links {
            pdf.object(format!(
                "<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI ({uri}) >> >>",
                rect[0], rect[1], rect[2], rect[3]
            ));
        }
    }
    let encrypt = encrypted.map(|_| {
        pdf.object("<< /Filter /Standard /V 1 /R 2 /Length 40 /P -4 /O <0000000000000000000000000000000000000000000000000000000000000000> /U <0000000000000000000000000000000000000000000000000000000000000000> >>".into())
    });
    pdf.finish(encrypt)
}

/// builds the bytes of a pdf and remembers where each object starts,
/// because the table at the end of a pdf has to list those positions.
#[derive(Default)]
struct PdfWriter {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfWriter {
    fn object(&mut self, body: String) -> usize {
        self.raw_object(body.as_bytes())
    }

    fn stream(&mut self, dictionary: &str, data: &[u8]) -> usize {
        let mut body = format!("<< {dictionary} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.raw_object(&body)
    }

    fn raw_object(&mut self, body: &[u8]) -> usize {
        if self.bytes.is_empty() {
            self.bytes.extend_from_slice(b"%PDF-1.7\n");
        }
        self.offsets.push(self.bytes.len());
        let number = self.offsets.len();
        self.bytes
            .extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        self.bytes.extend_from_slice(body);
        self.bytes.extend_from_slice(b"\nendobj\n");
        number
    }

    fn finish(mut self, encrypt: Option<usize>) -> Vec<u8> {
        let table_start = self.bytes.len();
        let object_count = self.offsets.len() + 1;
        let mut table = format!("xref\n0 {object_count}\n0000000000 65535 f \n");
        for offset in &self.offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        let encrypt = encrypt
            .map(|number| {
                format!(" /Encrypt {number} 0 R /ID [<00112233445566778899aabbccddeeff> <00112233445566778899aabbccddeeff>]")
            })
            .unwrap_or_default();
        table.push_str(&format!(
            "trailer\n<< /Size {object_count} /Root 1 0 R{encrypt} >>\nstartxref\n{table_start}\n%%EOF\n"
        ));
        self.bytes.extend_from_slice(table.as_bytes());
        self.bytes
    }
}

/// every file in a zip package (xlsx, pptx), read through its central
/// directory the way office programs do.
pub fn read_zip(path: &Path) -> Vec<(String, Vec<u8>)> {
    let bytes = fs::read(path).unwrap();
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let end = (0..bytes.len() - 21)
        .rev()
        .find(|at| bytes[*at..*at + 4] == [0x50, 0x4b, 0x05, 0x06])
        .expect("no end of central directory");
    let count = u16_at(end + 10);
    let mut at = u32_at(end + 16);
    let mut files = Vec::new();
    for _ in 0..count {
        assert_eq!(bytes[at..at + 4], [0x50, 0x4b, 0x01, 0x02]);
        let method = u16_at(at + 10);
        let crc = u32_at(at + 16) as u32;
        let compressed = u32_at(at + 20);
        let size = u32_at(at + 24);
        let name_length = u16_at(at + 28);
        let extra_length = u16_at(at + 30);
        let comment_length = u16_at(at + 32);
        let local = u32_at(at + 42);
        let name = String::from_utf8(bytes[at + 46..at + 46 + name_length].to_vec()).unwrap();
        let data_start = local + 30 + u16_at(local + 26) + u16_at(local + 28);
        let data = &bytes[data_start..data_start + compressed];
        let content = match method {
            0 => data.to_vec(),
            8 => {
                use std::io::Read;
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(data)
                    .read_to_end(&mut out)
                    .unwrap();
                out
            }
            other => panic!("unexpected compression {other}"),
        };
        assert_eq!(content.len(), size, "size of {name}");
        let mut check = flate2::Crc::new();
        check.update(&content);
        assert_eq!(check.sum(), crc, "checksum of {name}");
        files.push((name, content));
        at += 46 + name_length + extra_length + comment_length;
    }
    files
}

/// one text file out of a zip package.
pub fn read_zip_entry(path: &Path, name: &str) -> Option<String> {
    read_zip(path)
        .into_iter()
        .find(|(entry, _)| entry == name)
        .map(|(_, bytes)| String::from_utf8(bytes).unwrap())
}

pub fn zip_entry_names(path: &Path) -> Vec<String> {
    read_zip(path).into_iter().map(|(name, _)| name).collect()
}

/// a three page handbook with headings, lists, a ruled table with its
/// caption, lined-up text, a picture with a caption, a link, a footnote,
/// and a header and page numbers on every page.
pub fn handbook_pages() -> Vec<TestPage> {
    let mut pages = Vec::new();
    for number in 1..=3 {
        let mut page = TestPage::letter();
        page.text(Font::Helvetica, 9.0, 72.0, 40.0, "Course Handbook");
        page.text(Font::Helvetica, 9.0, 300.0, 775.0, &number.to_string());
        pages.push(page);
    }

    let first = &mut pages[0];
    first.text(Font::HelveticaBold, 20.0, 72.0, 90.0, "Chemistry Handbook");
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        120.0,
        "This handbook explains how the laboratory works and",
    );
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        133.2,
        "what every student needs before the first session.",
    );
    first.text(Font::HelveticaBold, 14.0, 72.0, 170.0, "Safety rules");
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        195.0,
        "• Wear goggles at all times",
    );
    first.text(Font::Helvetica, 11.0, 72.0, 208.2, "• Tie back long hair");
    first.text(Font::Helvetica, 11.0, 90.0, 221.4, "– Use a band or a clip");
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        234.6,
        "• Know where the exits are",
    );
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        262.0,
        "Before each experiment:",
    );
    first.text(Font::Helvetica, 11.0, 72.0, 280.0, "1. Read the method");
    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        293.2,
        "2. Collect the equipment",
    );

    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        322.0,
        "Table 1: Melting points",
    );
    for y in [330.0, 350.0, 370.0, 390.0] {
        first.line(72.0, y, 372.0, y, 0.75);
    }
    for x in [72.0, 192.0, 282.0, 372.0] {
        first.line(x, 330.0, x, 390.0, 0.75);
    }
    let rows = [
        ("Substance", "Melting point", "State"),
        ("Ice", "0 °C", "solid"),
        ("Iron", "1538 °C", "solid"),
    ];
    for (row, (a, b, c)) in rows.iter().enumerate() {
        let baseline = 344.0 + row as f32 * 20.0;
        let font = if row == 0 {
            Font::HelveticaBold
        } else {
            Font::Helvetica
        };
        first.text(font, 10.0, 76.0, baseline, a);
        first.text(font, 10.0, 196.0, baseline, b);
        first.text(font, 10.0, 286.0, baseline, c);
    }

    first.text(
        Font::Helvetica,
        11.0,
        72.0,
        420.0,
        "Read the full guide online.",
    );
    first.link(72.0, 410.0, 150.0, 14.0, "https://example.org/guide");
    first.line(72.0, 735.0, 200.0, 735.0, 0.5);
    first.text(
        Font::Helvetica,
        8.0,
        72.0,
        748.0,
        "1 Measured at normal air pressure.",
    );

    let second = &mut pages[1];
    let items = [
        ("Item", "Qty", "Price"),
        ("Pens", "4", "$2.00"),
        ("Paper", "10", "$5.50"),
        ("Ink", "1", "$9.99"),
    ];
    for (row, (a, b, c)) in items.iter().enumerate() {
        let baseline = 100.0 + row as f32 * 16.0;
        let font = if row == 0 {
            Font::HelveticaBold
        } else {
            Font::Helvetica
        };
        second.text(font, 10.0, 72.0, baseline, a);
        second.text(font, 10.0, 200.0, baseline, b);
        second.text(font, 10.0, 300.0, baseline, c);
    }
    second.image(
        72.0,
        200.0,
        144.0,
        96.0,
        TestImage::solid(4, 4, [200, 30, 30]),
    );
    second.text(Font::Helvetica, 9.0, 72.0, 312.0, "Figure 1: A flame test.");

    pages[2].text(
        Font::Helvetica,
        11.0,
        72.0,
        100.0,
        "The last page has only a short paragraph.",
    );
    pages
}

/// writes a zip with its parts stored as they are (no compression), for
/// hand-written office files.
pub fn write_stored_zip(path: &Path, parts: &[(&str, &str)]) {
    let mut bytes = Vec::new();
    let mut directory = Vec::new();
    for (name, content) in parts {
        let mut crc = flate2::Crc::new();
        crc.update(content.as_bytes());
        let offset = bytes.len() as u32;
        let mut header = Vec::new();
        header.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        header.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        header.extend_from_slice(&crc.sum().to_le_bytes());
        header.extend_from_slice(&(content.len() as u32).to_le_bytes());
        header.extend_from_slice(&(content.len() as u32).to_le_bytes());
        header.extend_from_slice(&(name.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(content.as_bytes());
        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&[20, 0]);
        directory.extend_from_slice(&header[4..]);
        // comment length, disk, internal and external attributes
        directory.extend_from_slice(&[0; 10]);
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let start = bytes.len() as u32;
    bytes.extend_from_slice(&directory);
    bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(parts.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&start.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    fs::write(path, bytes).unwrap();
}

/// the page count of a pdf and the text of each page, read by letters-pdf.
pub fn pdf_pages_text(path: &Path) -> Vec<String> {
    let worker = RenderWorker::start(&pdfium_dir(), 1024 * 1024).unwrap();
    let info = worker.open(path, None).unwrap();
    let pages = (0..info.page_sizes.len())
        .map(|page| {
            worker
                .page_text(info.id, page as u32)
                .unwrap()
                .chars
                .iter()
                .map(|c| c.ch)
                .collect()
        })
        .collect();
    worker.close(info.id);
    pages
}

/// the process ids of every running program with this file name.
pub fn process_ids(image: &str) -> Vec<u32> {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split(',').nth(1))
        .filter_map(|pid| pid.trim_matches('"').parse().ok())
        .collect()
}

/// runs one of the office check scripts beside this file and returns its
/// "name=value" lines. if it takes longer than `seconds` (a dialog such
/// as a repair prompt is waiting for someone), the script and the office
/// program it started are stopped and the test fails.
pub fn run_office_check(
    script: &str,
    args: &[&str],
    image: &str,
    seconds: u64,
) -> std::collections::HashMap<String, String> {
    let before = process_ids(image);
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support")
        .join(script);
    let mut child = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if started.elapsed().as_secs() > seconds {
            let _ = child.kill();
            for pid in process_ids(image)
                .into_iter()
                .filter(|pid| !before.contains(pid))
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output();
            }
            panic!("{image} didn't finish within {seconds} seconds: a prompt (such as a repair prompt) is probably open");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the check failed: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}
