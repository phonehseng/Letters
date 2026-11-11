//! helpers shared by the integration tests.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

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
        .join("letters-pdf-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// writes a small but valid pdf with `page_count` us-letter pages.
///
/// every page is white with one red square whose corners sit exactly
/// one and two inches from the top left corner. tests use that known
/// position to check that tiles land where they should.
///
/// the file is written by hand so the tests do not depend on any pdf
/// writing library being correct.
pub fn write_test_pdf(path: &Path, page_count: usize) {
    let mut pdf = PdfWriter::default();
    let content = "1 0 0 rg 72 648 72 72 re f";

    let kids: String = (0..page_count)
        .map(|page| format!("{} 0 R ", 4 + page))
        .collect();
    pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>".into());
    pdf.object(
        2,
        format!("<< /Type /Pages /Count {page_count} /Kids [ {kids}] /MediaBox [0 0 612 792] >>"),
    );
    pdf.object(
        3,
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    );
    for page in 0..page_count {
        pdf.object(
            4 + page,
            "<< /Type /Page /Parent 2 0 R /Contents 3 0 R /Resources << >> >>".into(),
        );
    }
    fs::write(path, pdf.finish()).unwrap();
}

/// us letter, in points.
pub const PAGE_WIDTH: f32 = 612.0;
pub const PAGE_HEIGHT: f32 = 792.0;

/// one line of text in helvetica, placed by its baseline.
pub struct TextLine {
    pub x: f32,
    /// the baseline's distance from the top of the page, in points
    pub baseline_from_top: f32,
    pub size: f32,
    pub text: String,
}

impl TextLine {
    pub fn new(x: f32, baseline_from_top: f32, size: f32, text: &str) -> Self {
        Self {
            x,
            baseline_from_top,
            size,
            text: text.to_owned(),
        }
    }
}

/// how an outline entry says where it leads. pdf writers use both forms.
#[derive(Clone, Copy)]
pub enum Target {
    /// a /Dest entry naming the page
    Page(usize),
    /// a /A "go to" action naming the page
    GoToAction(usize),
    /// leads nowhere
    Nowhere,
}

/// one entry of a test pdf's outline, with its children.
pub struct Bookmark {
    pub title: String,
    pub target: Target,
    pub children: Vec<Bookmark>,
}

impl Bookmark {
    pub fn new(title: &str, target: Target, children: Vec<Bookmark>) -> Self {
        Self {
            title: title.to_owned(),
            target,
            children,
        }
    }
}

/// writes a us-letter pdf whose pages hold the given lines of text, with
/// the given outline (pass an empty slice for none).
///
/// the text uses helvetica, one of the fonts every pdf reader has built
/// in, so no font file needs to be embedded.
pub fn write_text_pdf(path: &Path, pages: &[Vec<TextLine>], outline: &[Bookmark]) {
    write_rotated_text_pdf(path, pages, outline, 0);
}

/// like `write_text_pdf`, but every page is shown turned clockwise by
/// `rotate_degrees` (a multiple of 90), the way a scanned page held
/// sideways often is. text positions are given on the unturned page.
pub fn write_rotated_text_pdf(
    path: &Path,
    pages: &[Vec<TextLine>],
    outline: &[Bookmark],
    rotate_degrees: u32,
) {
    let page_count = pages.len();
    let page_object = |page: usize| 4 + 2 * page;
    let outline_root = 4 + 2 * page_count;
    let mut pdf = PdfWriter::default();

    let outlines_entry = if outline.is_empty() {
        String::new()
    } else {
        format!(" /Outlines {outline_root} 0 R")
    };
    pdf.object(
        1,
        format!("<< /Type /Catalog /Pages 2 0 R{outlines_entry} >>"),
    );
    let kids: String = (0..page_count)
        .map(|page| format!("{} 0 R ", page_object(page)))
        .collect();
    pdf.object(
        2,
        format!(
            "<< /Type /Pages /Count {page_count} /Kids [ {kids}] /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] >>"
        ),
    );
    pdf.object(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    );
    for (page, lines) in pages.iter().enumerate() {
        pdf.object(
            page_object(page),
            format!(
                "<< /Type /Page /Parent 2 0 R /Rotate {rotate_degrees} /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
                page_object(page) + 1
            ),
        );
        let content = text_content(lines);
        pdf.object(
            page_object(page) + 1,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        );
    }
    if !outline.is_empty() {
        write_outline(&mut pdf, outline, outline_root, page_object);
    }
    fs::write(path, pdf.finish()).unwrap();
}

/// the drawing instructions for some lines of text.
fn text_content(lines: &[TextLine]) -> String {
    lines
        .iter()
        .map(|line| {
            format!(
                "BT /F1 {} Tf {} {} Td ({}) Tj ET\n",
                line.size,
                line.x,
                PAGE_HEIGHT - line.baseline_from_top,
                escape_pdf_string(&line.text)
            )
        })
        .collect()
}

fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// an outline entry with the object numbers of its neighbours, which
/// pdf stores as links between entries.
struct NumberedBookmark<'a> {
    bookmark: &'a Bookmark,
    number: usize,
    parent: usize,
    previous: Option<usize>,
    next: Option<usize>,
    first_child: Option<usize>,
    last_child: Option<usize>,
    descendants: usize,
}

/// writes the outline root and every entry after it, numbered in reading order.
fn write_outline(
    pdf: &mut PdfWriter,
    outline: &[Bookmark],
    root: usize,
    page_object: impl Fn(usize) -> usize,
) {
    let mut numbered = Vec::new();
    let mut next_number = root + 1;
    let top_level = number_bookmarks(outline, root, &mut next_number, &mut numbered);
    pdf.object(
        root,
        format!(
            "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
            top_level[0],
            top_level[top_level.len() - 1],
            numbered.len()
        ),
    );
    for entry in &numbered {
        pdf.object(entry.number, bookmark_body(entry, &page_object));
    }
}

/// gives each entry an object number, parents before their children,
/// and returns the numbers of the entries in `bookmarks` itself.
fn number_bookmarks<'a>(
    bookmarks: &'a [Bookmark],
    parent: usize,
    next_number: &mut usize,
    numbered: &mut Vec<NumberedBookmark<'a>>,
) -> Vec<usize> {
    let mut siblings = Vec::new();
    for bookmark in bookmarks {
        let number = *next_number;
        *next_number += 1;
        let slot = numbered.len();
        numbered.push(NumberedBookmark {
            bookmark,
            number,
            parent,
            previous: siblings.last().copied(),
            next: None,
            first_child: None,
            last_child: None,
            descendants: 0,
        });
        let children = number_bookmarks(&bookmark.children, number, next_number, numbered);
        numbered[slot].first_child = children.first().copied();
        numbered[slot].last_child = children.last().copied();
        numbered[slot].descendants = *next_number - number - 1;
        siblings.push(number);
    }
    for pair in siblings.windows(2) {
        let entry = numbered
            .iter_mut()
            .find(|entry| entry.number == pair[0])
            .unwrap();
        entry.next = Some(pair[1]);
    }
    siblings
}

fn bookmark_body(entry: &NumberedBookmark, page_object: &impl Fn(usize) -> usize) -> String {
    let mut body = format!(
        "<< /Title ({}) /Parent {} 0 R",
        escape_pdf_string(&entry.bookmark.title),
        entry.parent
    );
    if let Some(previous) = entry.previous {
        body.push_str(&format!(" /Prev {previous} 0 R"));
    }
    if let Some(next) = entry.next {
        body.push_str(&format!(" /Next {next} 0 R"));
    }
    if let (Some(first), Some(last)) = (entry.first_child, entry.last_child) {
        body.push_str(&format!(
            " /First {first} 0 R /Last {last} 0 R /Count {}",
            entry.descendants
        ));
    }
    let destination = |page: usize| format!("[{} 0 R /XYZ 0 {PAGE_HEIGHT} 0]", page_object(page));
    match entry.bookmark.target {
        Target::Page(page) => body.push_str(&format!(" /Dest {}", destination(page))),
        Target::GoToAction(page) => {
            body.push_str(&format!(" /A << /S /GoTo /D {} >>", destination(page)))
        }
        Target::Nowhere => {}
    }
    body.push_str(" >>");
    body
}

/// builds the bytes of a pdf and remembers where each object starts,
/// because the table at the end of a pdf has to list those positions.
#[derive(Default)]
struct PdfWriter {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfWriter {
    fn object(&mut self, number: usize, body: String) {
        if self.bytes.is_empty() {
            self.bytes.extend_from_slice(b"%PDF-1.7\n");
        }
        assert_eq!(
            number,
            self.offsets.len() + 1,
            "objects must be added in order"
        );
        self.offsets.push(self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn finish(mut self) -> Vec<u8> {
        let table_start = self.bytes.len();
        let object_count = self.offsets.len() + 1;
        let mut table = format!("xref\n0 {object_count}\n0000000000 65535 f \n");
        for offset in &self.offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {object_count} /Root 1 0 R >>\nstartxref\n{table_start}\n%%EOF\n"
        ));
        self.bytes.extend_from_slice(table.as_bytes());
        self.bytes
    }
}

/// a decoded tile, so tests can look at single pixels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}

impl Image {
    pub fn from_png(bytes: &[u8]) -> Self {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        Self {
            width: info.width,
            height: info.height,
            rgba,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let start = ((y * self.width + x) * 4) as usize;
        [self.rgba[start], self.rgba[start + 1], self.rgba[start + 2]]
    }
}

pub const RED: [u8; 3] = [255, 0, 0];
pub const WHITE: [u8; 3] = [255, 255, 255];
