//! Word's own layout, for Strict Word mode.
//!
//! the office engine's bridge (`letters_office::bridge::word_layout`) opens
//! a private copy in a hidden word and reads every line back from word's
//! own layout. this module turns that into what the editor needs: for each
//! line, the model block (and table cell) it belongs to and the character
//! offset in that paragraph where word starts the line. the editor then
//! breaks its lines and pages at exactly those places. this is word doing
//! the layout, not an imitation of it.
//!
//! layouts are cached by a fingerprint of the document's bytes, so asking
//! again for an unchanged document costs milliseconds instead of the half
//! minute a cold word needs.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use letters_office::bridge::{
    self, BridgeError, LayoutShape, WordLayout as BridgeLayout, WordParagraph,
};
use letters_office::model::Document;
use serde::Serialize;

/// one line as the editor uses it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStart {
    /// the page, counted from 1
    pub page: u32,
    /// the top-level block the line belongs to
    pub block: usize,
    /// inside a table: row, cell and block within the cell
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<(usize, usize, usize)>,
    /// characters from the start of word's paragraph to the line's start
    pub offset: i64,
    /// twips from the page's top-left corner
    pub top: i64,
    pub left: i64,
    /// the first characters of the line, to check the match
    pub text: String,
}

/// what the editor gets.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLayout {
    /// the page each top-level block starts on (1 = first); empty when
    /// word's paragraphs could not be matched to the blocks
    pub block_pages: Vec<u32>,
    pub page_count: u32,
    /// every line of the main text, in order
    pub lines: Vec<LineStart>,
    /// where word put each floating object
    pub shapes: Vec<LayoutShape>,
    /// how long the answer took, word's start included (a cached answer
    /// takes a few milliseconds)
    pub milliseconds: u64,
    /// how long word itself worked, without starting
    pub word_milliseconds: i64,
}

/// asks word for the layout of `document`. Err with a sentence when word
/// is missing or fails.
pub fn word_layout(document: &Document, cache: &Path) -> Result<WordLayout, String> {
    let started = Instant::now();
    let docx = letters_office::write_docx(document).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let layout = bridge::word_layout(&docx, Some(cache)).map_err(|error| match error {
        BridgeError::WordNotInstalled => {
            "Microsoft Word isn’t installed on this computer.".to_string()
        }
        other => format!("Word couldn’t lay out the document. ({other})"),
    })?;
    let map = bridge::word_paragraph_map(document);
    let mut answer = editor_layout(&layout, &map, document.blocks.len());
    answer.milliseconds = started.elapsed().as_millis() as u64;
    Ok(answer)
}

/// word's lines traced back to the model's blocks.
pub fn editor_layout(
    layout: &BridgeLayout,
    map: &[Option<WordParagraph>],
    block_count: usize,
) -> WordLayout {
    // where each of word's paragraphs starts: the start of its first line
    let mut paragraph_start: HashMap<i64, i64> = HashMap::new();
    for line in main_lines(layout) {
        let entry = paragraph_start
            .entry(line.1.paragraph)
            .or_insert(line.1.start);
        *entry = (*entry).min(line.1.start);
    }
    let mut lines = Vec::new();
    for (page, line) in main_lines(layout) {
        let Some(Some(target)) = usize::try_from(line.paragraph)
            .ok()
            .and_then(|index| map.get(index))
        else {
            continue;
        };
        lines.push(LineStart {
            page,
            block: target.block,
            cell: target.cell,
            offset: line.start
                - paragraph_start
                    .get(&line.paragraph)
                    .copied()
                    .unwrap_or(line.start),
            top: line.top,
            left: line.left,
            text: line.text.clone(),
        });
    }
    WordLayout {
        block_pages: block_pages(&lines, block_count),
        page_count: u32::try_from(layout.word_pages.max(layout.pages.len() as i64)).unwrap_or(1),
        lines,
        shapes: layout.shapes.clone(),
        milliseconds: 0,
        word_milliseconds: layout.milliseconds,
    }
}

fn main_lines(layout: &BridgeLayout) -> impl Iterator<Item = (u32, &bridge::LayoutLine)> {
    layout.pages.iter().flat_map(|page| {
        let number = u32::try_from(page.number).unwrap_or(1);
        page.lines
            .iter()
            .filter(|line| line.story == 1 && line.paragraph >= 0)
            .map(move |line| (number, line))
    })
}

/// the page each block starts on. a block without a line of its own (a
/// section break) starts where the block before it was; if the first
/// block has none, the matching failed and the list is empty.
fn block_pages(lines: &[LineStart], block_count: usize) -> Vec<u32> {
    let mut first: Vec<Option<u32>> = vec![None; block_count];
    for line in lines {
        if let Some(slot) = first.get_mut(line.block) {
            if slot.is_none() {
                *slot = Some(line.page);
            }
        }
    }
    let mut pages = Vec::with_capacity(block_count);
    let mut last = None;
    for page in first {
        let page = page.or(last);
        match page {
            Some(value) => {
                pages.push(value);
                last = Some(value);
            }
            None => return Vec::new(),
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::bridge::{LayoutLine, LayoutPage};
    use letters_office::model::Block;

    fn line(paragraph: i64, start: i64, text: &str) -> LayoutLine {
        LayoutLine {
            top: 1440,
            left: 1440,
            height: 276,
            width: 100,
            story: 1,
            paragraph,
            start,
            end: start + 5,
            text: text.into(),
        }
    }

    fn layout(pages: Vec<Vec<LayoutLine>>) -> BridgeLayout {
        BridgeLayout {
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(index, lines)| LayoutPage {
                    number: index as i64 + 1,
                    width: 12240,
                    height: 15840,
                    lines,
                })
                .collect(),
            shapes: Vec::new(),
            word_pages: 2,
            milliseconds: 9,
            fingerprint: String::new(),
        }
    }

    #[test]
    fn lines_are_traced_to_blocks_with_offsets_in_their_paragraph() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("one two three"),
            Block::PageBreak,
            Block::text("four"),
        ];
        let map = bridge::word_paragraph_map(&document);
        let word = layout(vec![
            vec![line(0, 0, "one two"), line(0, 8, "three"), line(1, 14, "")],
            vec![line(2, 15, "four")],
        ]);
        let answer = editor_layout(&word, &map, document.blocks.len());
        assert_eq!(answer.block_pages, vec![1, 1, 2]);
        assert_eq!(answer.lines[1].offset, 8);
        assert_eq!(answer.lines[1].block, 0);
        assert_eq!(answer.lines[3].block, 2);
        assert_eq!(answer.lines[3].offset, 0);
        assert_eq!(answer.page_count, 2);
    }

    #[test]
    fn a_section_break_takes_the_page_of_the_block_before_it() {
        let lines = vec![
            LineStart {
                page: 1,
                block: 0,
                cell: None,
                offset: 0,
                top: 0,
                left: 0,
                text: String::new(),
            },
            LineStart {
                page: 2,
                block: 2,
                cell: None,
                offset: 0,
                top: 0,
                left: 0,
                text: String::new(),
            },
        ];
        assert_eq!(block_pages(&lines, 3), vec![1, 1, 2]);
        assert_eq!(block_pages(&lines[1..], 3), Vec::<u32>::new());
    }

    /// what word makes of a saved file, for the end-to-end checks: word's
    /// shapes, lines and page count through the bridge, and the model's
    /// text boxes and pictures as letters reads them back. run with
    /// LETTERS_VERIFY_DOCX (the file) and LETTERS_VERIFY_OUT (the report):
    /// `cargo test -p letters-shell-core word_report_for_a_file -- --ignored`
    #[test]
    #[ignore = "needs Microsoft Word on windows and a file in LETTERS_VERIFY_DOCX"]
    fn word_report_for_a_file() {
        let path = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_DOCX").unwrap());
        let out = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_OUT").unwrap());
        let bytes = std::fs::read(&path).unwrap();
        let document = letters_office::formats::open_any(&bytes).unwrap();
        let started = std::time::Instant::now();
        let layout = bridge::word_layout(&bytes, None).unwrap();
        let cold = started.elapsed().as_millis();
        let map = bridge::word_paragraph_map(&document);
        let lines = editor_layout(&layout, &map, document.blocks.len());
        let objects: Vec<serde_json::Value> = document
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| match block {
                Block::TextBox(text_box) => Some(serde_json::json!({ "block": index, "kind": "textBox", "horizontal": text_box.horizontal, "vertical": text_box.vertical, "width": text_box.width, "height": text_box.height, "rotation": text_box.rotation })),
                _ => None,
            })
            .collect();
        let images: Vec<serde_json::Value> = serde_json::to_value(&document.blocks)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|block| block["content"].as_array().cloned().unwrap_or_default())
            .filter(|inline| inline["type"] == "image")
            .collect();
        let report = serde_json::json!({ "pages": layout.word_pages, "shapes": layout.shapes, "lines": lines.lines, "blockPages": lines.block_pages, "objects": objects, "images": images, "coldMilliseconds": cold, "wordMilliseconds": layout.milliseconds });
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }

    /// word opens a rich text file and saves it again as docx through the
    /// bridge (no complaint means the conversion succeeds), and the text is
    /// the same as letters reads from the rtf: LETTERS_VERIFY_DOCX (the
    /// .rtf) and LETTERS_VERIFY_OUT (the report).
    #[test]
    #[ignore = "needs Microsoft Word on windows and a file in LETTERS_VERIFY_DOCX"]
    fn word_opens_rich_text() {
        let path = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_DOCX").unwrap());
        let out = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_OUT").unwrap());
        let rtf = std::fs::read(&path).unwrap();
        let ours = letters_office::formats::open_any(&rtf).unwrap();
        let (docx, engine) = bridge::convert_word_format(&rtf, false).unwrap();
        let words = letters_office::open(&docx).unwrap();
        let text = |document: &Document| -> Vec<String> {
            document
                .blocks
                .iter()
                .filter_map(|block| block.paragraph().map(|p| p.plain_text()))
                .filter(|text| !text.is_empty())
                .collect()
        };
        let report = serde_json::json!({ "engine": engine, "same": text(&ours) == text(&words), "ours": text(&ours).len(), "word": text(&words).len() });
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }

    /// runs the real word through the bridge:
    /// `cargo test -p letters-shell-core word_lays_out -- --ignored`
    #[test]
    #[ignore = "needs Microsoft Word on windows"]
    fn word_lays_out_a_page_break() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("first page"),
            Block::PageBreak,
            Block::text("second page"),
        ];
        let cache =
            std::env::temp_dir().join(format!("letters-layout-cache-{}", std::process::id()));
        let layout = word_layout(&document, &cache).unwrap();
        assert_eq!(layout.page_count, 2);
        assert_eq!(layout.block_pages, vec![1, 1, 2]);
        let again = word_layout(&document, &cache).unwrap();
        assert!(
            again.milliseconds < 2000,
            "a cached layout took {} ms",
            again.milliseconds
        );
    }
}
