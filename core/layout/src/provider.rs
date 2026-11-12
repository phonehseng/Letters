//! the engine as a drop-in layout provider for the editor.
//!
//! the editor already applies word's own layout through
//! `letters_office::bridge::word_layout`, which answers a
//! `bridge::WordLayout`: every page, every line (twips from the page's
//! top-left corner, its story, its paragraph in word's order and its
//! character range) and every floating object. `word_layout` here gives
//! the same shape from this engine, so the editor can switch providers
//! without other changes; `word_layout_json` takes and gives json, for the
//! app's command layer.
//!
//! where the shapes differ in meaning:
//! - `start` and `end` count the engine's model units (one per character,
//!   tab, break, picture, note reference and field), from the start of the
//!   main text with one unit per paragraph mark and per table row end, as
//!   word counts them. word counts a field as its code and result, so in a
//!   paragraph with fields the two differ; within a paragraph without
//!   fields they are the same.
//! - `milliseconds` is the engine's own time; `fingerprint` names the
//!   flavor and the document's block count, not a hash of docx bytes.

use std::collections::HashMap;
use std::time::Instant;

use letters_office::bridge::{word_paragraph_map, LayoutLine, LayoutPage, LayoutShape, WordLayout};
use letters_office::model::{Block, Document, Inline, Placement};

use crate::engine::{layout, LayoutOptions};
use crate::flavor::Flavor;
use crate::result::{LayoutResult, Story};

/// word's story numbers (WdStoryType), as the bridge reports them.
const MAIN_TEXT: i64 = 1;
const FOOTNOTES: i64 = 2;
const ENDNOTES: i64 = 3;
const TEXT_FRAMES: i64 = 5;
const PRIMARY_HEADER: i64 = 7;
const PRIMARY_FOOTER: i64 = 9;

/// lays the document out and answers in the bridge's shape.
pub fn word_layout(document: &Document, options: &LayoutOptions) -> WordLayout {
    let started = Instant::now();
    let result = layout(document, options);
    let mut answer = from_result(document, &result);
    answer.milliseconds = started.elapsed().as_millis() as i64;
    answer.fingerprint = format!(
        "letters-{}-{}",
        match options.flavor {
            Flavor::Word => "word",
            Flavor::LibreOffice => "libreoffice",
        },
        document.blocks.len()
    );
    answer
}

/// the same from json: a `letters_office::model::Document` in, a
/// `letters_office::bridge::WordLayout` out. errors are plain sentences.
pub fn word_layout_json(document_json: &str, flavor: Flavor) -> Result<String, String> {
    let document: Document = serde_json::from_str(document_json)
        .map_err(|error| format!("The document could not be read ({error})."))?;
    let answer = word_layout(&document, &LayoutOptions::flavor(flavor));
    serde_json::to_string(&answer)
        .map_err(|error| format!("The layout could not be written ({error})."))
}

/// a finished layout in the bridge's shape.
pub fn from_result(document: &Document, result: &LayoutResult) -> WordLayout {
    let positions = MainTextPositions::of(document);
    let pages = result
        .pages
        .iter()
        .map(|page| LayoutPage {
            number: i64::from(page.index) + 1,
            width: twips(page.width),
            height: twips(page.height),
            lines: page
                .lines
                .iter()
                .map(|line| {
                    let (story, paragraph, base) = match &line.story {
                        Story::Body => match positions.find(&line.block) {
                            Some((paragraph, base)) => (MAIN_TEXT, paragraph, base),
                            None => (MAIN_TEXT, -1, 0),
                        },
                        Story::Footnote { .. } => (FOOTNOTES, -1, 0),
                        Story::Endnote { .. } => (ENDNOTES, -1, 0),
                        Story::TextBox { .. } => (TEXT_FRAMES, -1, 0),
                        Story::Header { .. } => (PRIMARY_HEADER, -1, 0),
                        Story::Footer { .. } => (PRIMARY_FOOTER, -1, 0),
                        Story::Positioned { .. } => (MAIN_TEXT, -1, 0),
                    };
                    LayoutLine {
                        top: twips(line.top),
                        left: twips(line.x),
                        height: twips(line.height),
                        width: twips(line.width),
                        story,
                        paragraph,
                        start: base + i64::from(line.start),
                        end: base + i64::from(line.end),
                        text: line_text(line, 24),
                    }
                })
                .collect(),
        })
        .collect();
    WordLayout {
        word_pages: result.pages.len() as i64,
        shapes: shapes(document, result),
        pages,
        milliseconds: 0,
        fingerprint: String::new(),
    }
}

fn twips(points: f64) -> i64 {
    (points * 20.0).round() as i64
}

/// the first characters of a line's text.
fn line_text(line: &crate::result::Line, length: usize) -> String {
    line.runs
        .iter()
        .flat_map(|run| run.text.chars())
        .take(length)
        .collect()
}

/// a paragraph's place in the model: its block, and inside a table its
/// row, cell and block within the cell (as `bridge::WordParagraph`).
type ParagraphPlace = (usize, Option<(usize, usize, usize)>);

/// where each of word's paragraphs starts in its main text, by model path.
struct MainTextPositions {
    /// place -> (paragraph index in word's order, first position)
    starts: HashMap<ParagraphPlace, (i64, i64)>,
}

impl MainTextPositions {
    fn of(document: &Document) -> MainTextPositions {
        let mut starts = HashMap::new();
        let mut position: i64 = 0;
        for (index, entry) in word_paragraph_map(document).into_iter().enumerate() {
            match entry {
                Some(paragraph) => {
                    let units = paragraph_units(document, paragraph.block, paragraph.cell);
                    starts.insert((paragraph.block, paragraph.cell), (index as i64, position));
                    // the paragraph's text and its mark
                    position += units + 1;
                }
                // a table row's end mark
                None => position += 1,
            }
        }
        MainTextPositions { starts }
    }

    /// the paragraph and its first position for a line's block path.
    fn find(&self, path: &[u32]) -> Option<(i64, i64)> {
        let block = *path.first()? as usize;
        let cell =
            (path.len() >= 4).then(|| (path[1] as usize, path[2] as usize, path[3] as usize));
        self.starts.get(&(block, cell)).copied()
    }
}

/// a paragraph's length in model units (0 for what is not a paragraph).
fn paragraph_units(document: &Document, block: usize, cell: Option<(usize, usize, usize)>) -> i64 {
    let found = match (document.blocks.get(block), cell) {
        (Some(block), None) => block.paragraph(),
        (Some(Block::Table(table)), Some((row, cell, inner))) => table
            .rows
            .get(row)
            .and_then(|row| row.cells.get(cell))
            .and_then(|cell| cell.blocks.get(inner))
            .and_then(Block::paragraph),
        _ => None,
    };
    found.map_or(0, |paragraph| {
        paragraph
            .content
            .iter()
            .map(|inline| i64::from(crate::text::units_of(inline)))
            .sum()
    })
}

/// every floating picture and text box with where it landed.
fn shapes(document: &Document, result: &LayoutResult) -> Vec<LayoutShape> {
    let mut out = Vec::new();
    for page in &result.pages {
        for image in page.images.iter().filter(|image| image.floating) {
            let (horizontal, vertical, name) = image_anchor(document, image);
            out.push(LayoutShape {
                name,
                page: i64::from(page.index) + 1,
                left: twips(image.x),
                top: twips(image.y),
                width: twips(image.width),
                height: twips(image.height),
                relative_horizontal: horizontal,
                relative_vertical: vertical,
                text: String::new(),
            });
        }
        for frame in &page.frames {
            let text: String = page
                .lines
                .iter()
                .filter(|line| line.story == Story::TextBox { block: frame.block })
                .flat_map(|line| line.runs.iter().flat_map(|run| run.text.chars()))
                .take(64)
                .collect();
            let (horizontal, vertical) = match document.blocks.get(frame.block as usize) {
                Some(Block::TextBox(text_box)) => (
                    horizontal_code(&text_box.horizontal.relative_to),
                    vertical_code(&text_box.vertical.relative_to),
                ),
                _ => (0, 0),
            };
            out.push(LayoutShape {
                name: format!("Text Box {}", frame.block),
                page: i64::from(page.index) + 1,
                left: twips(frame.x),
                top: twips(frame.y),
                width: twips(frame.width),
                height: twips(frame.height),
                relative_horizontal: horizontal,
                relative_vertical: vertical,
                text,
            });
        }
    }
    out
}

/// the anchor codes and name of a placed floating picture, from the model.
fn image_anchor(document: &Document, image: &crate::result::PlacedImage) -> (i64, i64, String) {
    let paragraph = match (&image.story, image.block.first()) {
        (Story::Body, Some(block)) => document
            .blocks
            .get(*block as usize)
            .and_then(Block::paragraph),
        _ => None,
    };
    let mut offset = 0;
    for inline in paragraph.map_or(&[][..], |p| p.content.as_slice()) {
        if offset == image.offset {
            if let Inline::Image(found) = inline {
                if let Placement::Floating {
                    horizontal,
                    vertical,
                    ..
                } = &found.placement
                {
                    return (
                        horizontal_code(&horizontal.relative_to),
                        vertical_code(&vertical.relative_to),
                        found.name.clone().unwrap_or_else(|| "Picture".into()),
                    );
                }
            }
        }
        offset += crate::text::units_of(inline);
    }
    (0, 2, "Picture".into())
}

/// word's codes: 0 margin, 1 page, 2 column, 3 character.
fn horizontal_code(relative_to: &str) -> i64 {
    match relative_to {
        "page" => 1,
        "column" => 2,
        "character" => 3,
        _ => 0,
    }
}

/// word's codes: 0 margin, 1 page, 2 paragraph, 3 line.
fn vertical_code(relative_to: &str) -> i64 {
    match relative_to {
        "margin" | "topMargin" | "bottomMargin" => 0,
        "page" => 1,
        "line" => 3,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_count_from_the_start_of_the_main_text() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("first"),
            Block::text("second paragraph"),
            Block::PageBreak,
            Block::text("after"),
        ];
        let answer = word_layout(&document, &LayoutOptions::default());
        let body: Vec<&LayoutLine> = answer
            .pages
            .iter()
            .flat_map(|page| &page.lines)
            .filter(|line| line.story == MAIN_TEXT)
            .collect();
        assert_eq!(body[0].paragraph, 0);
        assert_eq!(body[0].start, 0);
        // "first" and its paragraph mark come before the second paragraph
        assert_eq!(body[1].paragraph, 1);
        assert_eq!(body[1].start, 6);
        assert_eq!(answer.word_pages, 2);
        assert_eq!(body.last().unwrap().text, "after");
        // margins of an inch: 1440 twips from the top-left corner, about
        assert!((body[0].left - 1440).abs() <= 1);
    }

    #[test]
    fn json_in_and_out() {
        let mut document = Document::new();
        document.blocks = vec![Block::text("hello")];
        let json = serde_json::to_string(&document).unwrap();
        let answer = word_layout_json(&json, Flavor::Word).unwrap();
        let back: WordLayout = serde_json::from_str(&answer).unwrap();
        assert_eq!(back.word_pages, 1);
        assert!(word_layout_json("not json", Flavor::Word)
            .unwrap_err()
            .starts_with("The document could not be read"));
    }
}
