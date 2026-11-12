//! small pdfs written byte by byte for the compare and accessibility
//! tests, and the pdf engine they draw with.

use std::fs;
use std::path::{Path, PathBuf};

use letters_pdf::RenderWorker;

/// where the pdfium library is (the fetch script puts it in vendor/pdfium).
pub fn pdfium_dir() -> PathBuf {
    match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    }
}

pub fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), 32 * 1024 * 1024).expect("run tools/fetch-pdfium first")
}

/// what goes on one page: lines of text, filled boxes, raw drawing.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub lines: Vec<Line>,
    /// left, top, width, height in points from the top left, and the colour
    pub boxes: Vec<([f32; 4], [f32; 3])>,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub x: f32,
    /// the baseline, in points from the top of the page
    pub baseline: f32,
    pub size: f32,
    pub text: String,
    pub colour: [f32; 3],
}

impl Page {
    pub fn text(mut self, x: f32, baseline: f32, size: f32, text: &str) -> Self {
        self.lines.push(Line {
            x,
            baseline,
            size,
            text: text.to_owned(),
            colour: [0.0, 0.0, 0.0],
        });
        self
    }

    pub fn coloured_text(
        mut self,
        x: f32,
        baseline: f32,
        size: f32,
        text: &str,
        colour: [f32; 3],
    ) -> Self {
        self.lines.push(Line {
            x,
            baseline,
            size,
            text: text.to_owned(),
            colour,
        });
        self
    }

    pub fn filled(mut self, rect: [f32; 4], colour: [f32; 3]) -> Self {
        self.boxes.push((rect, colour));
        self
    }
}

/// a page of distinct words, so pages can be told apart by their text.
pub fn numbered_page(seed: usize) -> Page {
    let mut page = Page::default();
    for line in 0..6 {
        let words: Vec<String> = (0..8)
            .map(|word| format!("p{seed}l{line}w{word}"))
            .collect();
        page = page.text(72.0, 100.0 + line as f32 * 16.0, 11.0, &words.join(" "));
    }
    page
}

const HEIGHT: f32 = 792.0;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn content(page: &Page) -> String {
    let mut out = String::new();
    for ([x, top, width, height], [r, g, b]) in &page.boxes {
        out.push_str(&format!(
            "q {r} {g} {b} rg {x} {} {width} {height} re f Q\n",
            HEIGHT - top - height
        ));
    }
    for line in &page.lines {
        let [r, g, b] = line.colour;
        out.push_str(&format!(
            "BT {r} {g} {b} rg /F1 {} Tf {} {} Td ({}) Tj ET\n",
            line.size,
            line.x,
            HEIGHT - line.baseline,
            escape(&line.text)
        ));
    }
    out
}

/// the bytes of a us letter pdf with these pages and no tags.
pub fn pdf_bytes(pages: &[Page]) -> Vec<u8> {
    let mut objects: Vec<String> = vec![String::new(); 4];
    objects[1] = "<< /Type /Catalog /Pages 2 0 R >>".into();
    objects[3] =
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into();
    let mut kids = Vec::new();
    for page in pages {
        let stream = content(page);
        let content_id = objects.len();
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
        let page_id = objects.len();
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 {HEIGHT}] /Contents {content_id} 0 R /Resources << /Font << /F1 3 0 R >> >> >>"
        ));
        kids.push(format!("{page_id} 0 R"));
    }
    objects[2] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        pages.len(),
        kids.join(" ")
    );
    write_objects(&objects, "")
}

/// a pdf whose trailer says it is encrypted, which no password opens.
pub fn locked_pdf_bytes() -> Vec<u8> {
    let mut objects: Vec<String> = vec![String::new(); 4];
    objects[1] = "<< /Type /Catalog /Pages 2 0 R >>".into();
    objects[2] = "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".into();
    objects[3] = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".into();
    let owner = "0".repeat(64);
    let user = "1".repeat(64);
    objects.push(format!(
        "<< /Filter /Standard /V 1 /R 2 /Length 40 /P -4 /O <{owner}> /U <{user}> >>"
    ));
    write_objects(&objects, " /Encrypt 4 0 R /ID [<00112233445566778899aabbccddeeff> <00112233445566778899aabbccddeeff>]")
}

/// numbers objects from 1 (index 0 is unused) and adds the table of positions.
fn write_objects(objects: &[String], trailer_extra: &str) -> Vec<u8> {
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (number, body) in objects.iter().enumerate().skip(1) {
        offsets.push(out.len());
        out.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
    }
    let table = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len()));
    for offset in offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R{trailer_extra} >>\nstartxref\n{table}\n%%EOF\n",
        objects.len()
    ));
    out.into_bytes()
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}
