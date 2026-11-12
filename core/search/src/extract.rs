//! reading the text out of each kind of file, as passages.
//!
//! - pdf: one passage per page, through `PdfText`, which the app
//!   implements with its pdf engine. this crate does not depend on pdfium,
//!   so it builds and tests without it. pages with no text are counted
//!   and noted: a scan needs text recognition before it can be searched.
//! - word documents (.docx, .odt, .rtf): one passage per paragraph or
//!   table, read through letters-office, with the headings it sits under
//!   and an estimated page.
//! - plain text and markdown: one passage per paragraph (text between
//!   blank lines); markdown's "#" lines are headings.

use std::path::Path;

use letters_office::model::{Block, Document};

use crate::model::{heading_path, title_from_path, DocNote, ExtractedDocument, Passage};

/// why a file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// it needs a password. passwords are never stored, so it is indexed
    /// by name only
    Locked,
    Unreadable(String),
    /// indexing was paused or stopped while the file was being read
    Stopped,
}

/// one entry of a pdf's table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub title: String,
    /// counted from 0
    pub page: u32,
    pub level: u32,
}

/// what the index needs from a pdf engine. one file is open at a time.
pub trait PdfText {
    /// opens a file and returns how many pages it has.
    fn open(&mut self, path: &Path) -> Result<u32, ExtractError>;
    /// the text of one page (counted from 0) of the open file.
    fn page_text(&mut self, page: u32) -> Result<String, ExtractError>;
    /// the open file's table of contents, used as the headings of its pages.
    fn outline(&mut self) -> Vec<OutlineEntry>;
    fn close(&mut self);
    /// asked between pages: true stops a long file part way (for a pause).
    fn should_stop(&mut self) -> bool {
        false
    }
}

/// pages with fewer letters than this are counted as having no text.
const MIN_LETTERS_FOR_TEXT: usize = 3;

/// reads every page of a pdf.
pub fn extract_pdf(
    path: &Path,
    engine: &mut dyn PdfText,
) -> Result<ExtractedDocument, ExtractError> {
    let pages = engine.open(path)?;
    let result = read_pdf_pages(path, pages, engine);
    engine.close();
    result
}

fn read_pdf_pages(
    path: &Path,
    pages: u32,
    engine: &mut dyn PdfText,
) -> Result<ExtractedDocument, ExtractError> {
    let outline = engine.outline();
    let mut passages = Vec::with_capacity(pages as usize);
    let mut without_text = 0;
    for page in 0..pages {
        if engine.should_stop() {
            return Err(ExtractError::Stopped);
        }
        let text = engine.page_text(page)?;
        if text.chars().filter(|c| c.is_alphanumeric()).count() < MIN_LETTERS_FOR_TEXT {
            without_text += 1;
        }
        passages.push(Passage {
            number: page,
            page,
            heading: heading_for_page(&outline, page),
            text,
        });
    }
    let mut notes = Vec::new();
    if without_text > 0 {
        notes.push(DocNote::NeedsTextRecognition {
            pages: without_text,
        });
    }
    Ok(ExtractedDocument {
        title: title_from_path(path),
        pages,
        passages,
        notes,
    })
}

/// the outline entries a page sits under: for each level, the last
/// entry that starts on or before the page.
pub fn heading_for_page(outline: &[OutlineEntry], page: u32) -> String {
    let mut stack: Vec<&OutlineEntry> = Vec::new();
    for entry in outline.iter().filter(|entry| entry.page <= page) {
        stack.retain(|open| open.level < entry.level);
        stack.push(entry);
    }
    let titles: Vec<String> = stack.iter().map(|e| e.title.trim().to_string()).collect();
    heading_path(&titles)
}

/// reads a word document from its bytes (docx, odt or rtf).
pub fn extract_word(path: &Path, bytes: &[u8]) -> Result<ExtractedDocument, ExtractError> {
    let is_rtf = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("rtf"));
    let document = if is_rtf {
        letters_office::rtf::read_rtf(bytes)
    } else {
        letters_office::open(bytes)
    }
    .map_err(|error| unreadable_office(&error.to_string()))?;
    Ok(passages_of_document(path, &document))
}

fn unreadable_office(message: &str) -> ExtractError {
    let lower = message.to_lowercase();
    if lower.contains("password") || lower.contains("encrypt") {
        ExtractError::Locked
    } else {
        ExtractError::Unreadable(message.to_string())
    }
}

/// the passages of an office document, with headings and estimated pages.
pub fn passages_of_document(path: &Path, document: &Document) -> ExtractedDocument {
    let mut passages = Vec::new();
    let mut headings: Vec<(u8, String)> = Vec::new();
    for (index, block) in document.blocks.iter().enumerate() {
        if let Block::Heading(heading) = block {
            let title = heading.paragraph.plain_text().trim().to_string();
            headings.retain(|(level, _)| *level < heading.level);
            headings.push((heading.level, title));
        }
        let text = block_text(block);
        if text.trim().is_empty() {
            continue;
        }
        let path_titles: Vec<String> = headings.iter().map(|(_, t)| t.clone()).collect();
        passages.push(Passage {
            number: index as u32,
            page: 0,
            heading: heading_path(&path_titles),
            text,
        });
    }
    add_notes(document, &mut passages);
    let pages = letters_office::counts::count(document)
        .pages_estimate
        .max(1) as u32;
    estimate_pages(&mut passages, pages);
    ExtractedDocument {
        title: document_title(path, document),
        pages,
        passages,
        notes: Vec::new(),
    }
}

/// footnotes and endnotes are searchable too; they jump to the end.
fn add_notes(document: &Document, passages: &mut Vec<Passage>) {
    let end = document.blocks.len() as u32;
    for note in document.notes.iter().filter(|n| n.special.is_none()) {
        let text: Vec<String> = note.blocks.iter().map(block_text).collect();
        let text = text.join(" ");
        if !text.trim().is_empty() {
            passages.push(Passage {
                number: end,
                page: 0,
                heading: "Notes".into(),
                text,
            });
        }
    }
}

fn document_title(path: &Path, document: &Document) -> String {
    let title = document.metadata.title.clone().unwrap_or_default();
    if title.trim().is_empty() {
        title_from_path(path)
    } else {
        format!("{} \u{2014} {}", title_from_path(path), title.trim())
    }
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Paragraph(_) | Block::Heading(_) => block
            .paragraph()
            .map(|paragraph| paragraph.plain_text())
            .unwrap_or_default(),
        Block::Table(table) => {
            let mut cells = Vec::new();
            for row in &table.rows {
                for cell in &row.cells {
                    for inner in &cell.blocks {
                        cells.push(block_text(inner));
                    }
                }
            }
            cells.join(" ")
        }
        Block::TextBox(text_box) => {
            let parts: Vec<String> = text_box.blocks.iter().map(block_text).collect();
            parts.join(" ")
        }
        _ => String::new(),
    }
}

/// spreads `pages` over the passages by how many words come before each.
fn estimate_pages(passages: &mut [Passage], pages: u32) {
    let total: usize = passages.iter().map(|p| word_total(&p.text)).sum();
    let mut before = 0usize;
    for passage in passages.iter_mut() {
        passage.page = if total == 0 {
            0
        } else {
            ((before as u64 * u64::from(pages)) / total as u64).min(u64::from(pages - 1)) as u32
        };
        before += word_total(&passage.text);
    }
}

fn word_total(text: &str) -> usize {
    letters_office::counts::word_count(text)
}

/// reads plain text or markdown.
pub fn extract_text(path: &Path, bytes: &[u8]) -> ExtractedDocument {
    let text = String::from_utf8_lossy(bytes);
    let markdown = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    let mut passages = Vec::new();
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut paragraph = String::new();
    let mut number = 0u32;
    let mut flush = |paragraph: &mut String, headings: &[(usize, String)], number: &mut u32| {
        if !paragraph.trim().is_empty() {
            let titles: Vec<String> = headings.iter().map(|(_, t)| t.clone()).collect();
            passages.push(Passage {
                number: *number,
                page: 0,
                heading: heading_path(&titles),
                text: std::mem::take(paragraph),
            });
            *number += 1;
        }
        paragraph.clear();
    };
    for line in text.lines() {
        if let Some((level, title)) = markdown.then(|| markdown_heading(line)).flatten() {
            flush(&mut paragraph, &headings, &mut number);
            headings.retain(|(l, _)| *l < level);
            headings.push((level, title.clone()));
            paragraph.push_str(&title);
            flush(&mut paragraph, &headings, &mut number);
        } else if line.trim().is_empty() {
            flush(&mut paragraph, &headings, &mut number);
        } else {
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(line);
        }
    }
    flush(&mut paragraph, &headings, &mut number);
    // about 500 words to a printed page
    let words: usize = passages.iter().map(|p| word_total(&p.text)).sum();
    let pages = (words / 500 + 1) as u32;
    estimate_pages(&mut passages, pages);
    ExtractedDocument {
        title: title_from_path(path),
        pages,
        passages,
        notes: Vec::new(),
    }
}

fn markdown_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().trim_end_matches('#').trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Heading, Paragraph};

    #[test]
    fn page_headings_come_from_the_outline() {
        let outline = vec![
            OutlineEntry {
                title: "Part I".into(),
                page: 0,
                level: 0,
            },
            OutlineEntry {
                title: "Methods".into(),
                page: 3,
                level: 1,
            },
            OutlineEntry {
                title: "Part II".into(),
                page: 8,
                level: 0,
            },
        ];
        assert_eq!(heading_for_page(&outline, 1), "Part I");
        assert_eq!(heading_for_page(&outline, 4), "Part I \u{203A} Methods");
        assert_eq!(heading_for_page(&outline, 9), "Part II");
        assert_eq!(heading_for_page(&[], 9), "");
    }

    struct FakePdf {
        pages: Vec<String>,
    }

    impl PdfText for FakePdf {
        fn open(&mut self, _path: &Path) -> Result<u32, ExtractError> {
            Ok(self.pages.len() as u32)
        }
        fn page_text(&mut self, page: u32) -> Result<String, ExtractError> {
            Ok(self.pages[page as usize].clone())
        }
        fn outline(&mut self) -> Vec<OutlineEntry> {
            Vec::new()
        }
        fn close(&mut self) {}
    }

    #[test]
    fn pdf_pages_without_text_are_noted() {
        let mut pdf = FakePdf {
            pages: vec!["Real words here".into(), "".into(), " - ".into()],
        };
        let document = extract_pdf(Path::new("scan.pdf"), &mut pdf).unwrap();
        assert_eq!(document.title, "scan");
        assert_eq!(document.passages.len(), 3);
        assert_eq!(
            document.notes,
            vec![DocNote::NeedsTextRecognition { pages: 2 }]
        );
    }

    #[test]
    fn office_passages_carry_their_heading_path() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::Heading(Heading {
                level: 1,
                paragraph: Paragraph::with_text("Chapter One"),
            }),
            Block::text("It was a bright cold day."),
            Block::Heading(Heading {
                level: 2,
                paragraph: Paragraph::with_text("The Station"),
            }),
            Block::text("Trains came and went."),
            Block::text(""),
        ];
        let extracted = passages_of_document(Path::new("novel.docx"), &document);
        let found: Vec<(u32, &str)> = extracted
            .passages
            .iter()
            .map(|p| (p.number, p.heading.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                (0, "Chapter One"),
                (1, "Chapter One"),
                (2, "Chapter One \u{203A} The Station"),
                (3, "Chapter One \u{203A} The Station"),
            ]
        );
    }

    #[test]
    fn markdown_paragraphs_and_headings() {
        let text = "# Notes\n\nfirst line\nsame paragraph\n\n## Later\n\nsecond\n";
        let extracted = extract_text(Path::new("n.md"), text.as_bytes());
        let found: Vec<(&str, &str)> = extracted
            .passages
            .iter()
            .map(|p| (p.heading.as_str(), p.text.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                ("Notes", "Notes"),
                ("Notes", "first line\nsame paragraph"),
                ("Notes \u{203A} Later", "Later"),
                ("Notes \u{203A} Later", "second"),
            ]
        );
        // plain text has no headings
        let plain = extract_text(Path::new("n.txt"), b"# not a heading\n\nnext");
        assert_eq!(plain.passages[0].heading, "");
    }
}
