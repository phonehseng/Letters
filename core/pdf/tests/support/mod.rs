//! helpers shared by the integration tests.

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
