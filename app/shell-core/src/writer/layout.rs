//! Word's own page breaks, for Strict Word mode.
//!
//! Word opens a copy of the document (privately, read-only) and says on
//! which page each of its paragraphs starts. those paragraphs are matched
//! to the model's top-level blocks, so the editor can break its pages
//! exactly where Word does. this is Word doing the layout, not an
//! imitation of it.

use std::fs;
use std::path::Path;
use std::process::Command;

use letters_office::model::Block;
use serde::{Deserialize, Serialize};

use crate::writer::export::{hidden, run_with_timeout, word_installed};

const LAYOUT_SCRIPT: &str = include_str!("word_layout.ps1");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordParagraph {
    pub page: u32,
    pub in_table: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordAnswer {
    pub paragraphs: Vec<WordParagraph>,
    pub page_count: u32,
}

/// what the editor gets: the page each top-level block starts on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLayout {
    /// empty when Word's paragraphs could not be matched to the blocks
    pub block_pages: Vec<u32>,
    pub page_count: u32,
}

/// matches Word's paragraphs to the model's blocks: a paragraph or heading
/// is one Word paragraph, a page break block is one, a table is all the
/// table paragraphs in a row. a section break has no paragraph of its own.
/// `None` when the two don't line up (content Letters keeps opaquely can
/// hold paragraphs of its own), so the editor falls back to its estimate.
pub fn match_blocks(blocks: &[Block], paragraphs: &[WordParagraph]) -> Option<Vec<u32>> {
    let mut pages = Vec::with_capacity(blocks.len());
    let mut next = 0usize;
    let mut last_page = 1;
    for block in blocks {
        match block {
            Block::Table(_) => {
                let first = paragraphs.get(next).filter(|paragraph| paragraph.in_table)?;
                pages.push(first.page);
                last_page = first.page;
                while paragraphs.get(next).is_some_and(|paragraph| paragraph.in_table) {
                    next += 1;
                }
            }
            Block::SectionBreak => pages.push(last_page),
            Block::Preserved { .. } => return None,
            _ => {
                let paragraph = paragraphs.get(next).filter(|paragraph| !paragraph.in_table)?;
                pages.push(paragraph.page);
                last_page = paragraph.page;
                next += 1;
            }
        }
    }
    (next == paragraphs.len()).then_some(pages)
}

/// asks Word for the layout of `docx`. Err when Word is missing or fails.
pub fn word_layout(docx: &Path, blocks: &[Block], work_dir: &Path) -> Result<WordLayout, String> {
    if !word_installed() {
        return Err("Microsoft Word isn’t installed on this computer.".into());
    }
    fs::create_dir_all(work_dir).map_err(|error| error.to_string())?;
    let script = work_dir.join("word_layout.ps1");
    fs::write(&script, LAYOUT_SCRIPT).map_err(|error| error.to_string())?;
    let out = work_dir.join("layout.json");
    let _ = fs::remove_file(&out);
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script).arg("-Path").arg(docx).arg("-Out").arg(&out);
    run_with_timeout(hidden(&mut command)).map_err(|detail| format!("Word couldn’t lay out the document ({detail})."))?;
    let text = fs::read_to_string(&out).map_err(|error| error.to_string())?;
    let answer: WordAnswer = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| error.to_string())?;
    Ok(WordLayout {
        block_pages: match_blocks(blocks, &answer.paragraphs).unwrap_or_default(),
        page_count: answer.page_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Table, TableCell, TableRow};

    fn paragraph(page: u32, in_table: bool) -> WordParagraph {
        WordParagraph { page, in_table }
    }

    #[test]
    fn paragraphs_tables_and_breaks_line_up_with_blocks() {
        let table = Block::Table(Table {
            rows: vec![TableRow { cells: vec![TableCell::with_text("a"), TableCell::with_text("b")], ..Default::default() }],
            ..Default::default()
        });
        let blocks = vec![Block::text("one"), table, Block::PageBreak, Block::text("two"), Block::SectionBreak, Block::text("three")];
        let word = vec![paragraph(1, false), paragraph(1, true), paragraph(1, true), paragraph(1, false), paragraph(2, false), paragraph(2, false)];
        assert_eq!(match_blocks(&blocks, &word), Some(vec![1, 1, 1, 2, 2, 2]));
    }

    #[test]
    fn a_mismatch_falls_back_to_the_estimate() {
        assert_eq!(match_blocks(&[Block::text("one")], &[paragraph(1, false), paragraph(1, false)]), None);
        assert_eq!(match_blocks(&[Block::Preserved { xml: "<w:sdt/>".into() }], &[paragraph(1, false)]), None);
    }

    /// runs the real word: `cargo test -p letters-shell-core word_lays_out -- --ignored`
    #[test]
    #[ignore]
    fn word_lays_out_a_page_break() {
        let dir = std::env::temp_dir().join(format!("letters-layout-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut document = letters_office::model::Document::new();
        document.blocks = vec![Block::text("first page"), Block::PageBreak, Block::text("second page")];
        let docx = dir.join("in.docx");
        fs::write(&docx, letters_office::write_docx(&document).unwrap()).unwrap();
        let layout = word_layout(&docx, &document.blocks, &dir.join("work")).unwrap();
        assert_eq!(layout.page_count, 2);
        assert_eq!(layout.block_pages, vec![1, 1, 2]);
    }
}
