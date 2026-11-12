//! word, character and page counts for the status bar.
//!
//! words and characters are counted exactly (deleted tracked text left
//! out). pages are an estimate: real page breaks need the full layout,
//! which happens in the editor, so this works from font size, line
//! spacing, page size and margins, and is usually within a page or so.

use serde::Serialize;

use crate::model::{Block, Document, LineRule, SectionStart};
use crate::styles::{resolved_paragraph_props, resolved_run_props};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub words: usize,
    pub characters: usize,
    pub characters_without_spaces: usize,
    pub paragraphs: usize,
    /// words in footnotes and endnotes, which some assignments exclude
    pub note_words: usize,
    pub pages_estimate: usize,
}

/// counts the main text (tables included).
pub fn count(document: &Document) -> Counts {
    let mut counts = Counts::default();
    let mut texts = Vec::new();
    collect_texts(&document.blocks, &mut texts);
    for text in &texts {
        if !text.trim().is_empty() {
            counts.paragraphs += 1;
        }
        counts.words += word_count(text);
        counts.characters += text.chars().filter(|c| *c != '\n').count();
        counts.characters_without_spaces += text.chars().filter(|c| !c.is_whitespace()).count();
    }
    for note in document.notes.iter().filter(|note| note.special.is_none()) {
        let mut note_texts = Vec::new();
        collect_texts(&note.blocks, &mut note_texts);
        counts.note_words += note_texts.iter().map(|t| word_count(t)).sum::<usize>();
    }
    counts.pages_estimate = estimate_pages(document);
    counts
}

/// a word is a run of non-space characters with at least one letter or
/// digit in it, so a lone dash is not a word.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

fn collect_texts(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_texts(&cell.blocks, out);
                    }
                }
            }
            _ => {
                if let Some(paragraph) = block.paragraph() {
                    out.push(paragraph.plain_text());
                }
            }
        }
    }
}

/// adds up the height every paragraph takes and divides by the space a
/// page has, starting a new page at page and section breaks.
fn estimate_pages(document: &Document) -> usize {
    let mut pages = 1usize;
    let mut used = 0.0f64;
    for (index, block) in document.blocks.iter().enumerate() {
        let section = document.section_of(index);
        let page_height = f64::from(section.page.text_height().max(1440));
        let text_width = f64::from(section.page.text_width().max(1440))
            / f64::from(section.columns.count.max(1));
        let height = match block {
            Block::PageBreak => {
                pages += 1;
                used = 0.0;
                continue;
            }
            Block::SectionBreak => {
                let next = document.sections.get(document.section_index_of(index) + 1);
                if next.is_some_and(|s| s.start != SectionStart::Continuous) {
                    pages += 1;
                    used = 0.0;
                }
                continue;
            }
            Block::Table(table) => table.rows.len() as f64 * 400.0,
            _ => paragraph_height(document, block, text_width),
        };
        if block
            .paragraph()
            .is_some_and(|p| p.props.page_break_before == Some(true))
            && used > 0.0
        {
            pages += 1;
            used = 0.0;
        }
        used += height;
        while used > page_height {
            pages += 1;
            used -= page_height;
        }
    }
    pages
}

/// the height of a paragraph in twips.
fn paragraph_height(document: &Document, block: &Block, text_width: f64) -> f64 {
    let Some(paragraph) = block.paragraph() else {
        return 0.0;
    };
    let props = resolved_paragraph_props(document, paragraph);
    let first_marks = paragraph
        .content
        .first()
        .map(|inline| inline.marks())
        .unwrap_or(&[]);
    let run = resolved_run_props(document, paragraph.style_id.as_deref(), first_marks);
    let size_twips = f64::from(run.size.unwrap_or(22)) * 10.0;
    let line_height = match props.spacing.line {
        Some(line) if line.rule == LineRule::Auto => {
            size_twips * 1.15 * f64::from(line.value) / 240.0
        }
        Some(line) => f64::from(line.value).max(size_twips),
        None => size_twips * 1.15,
    };
    // an average character is about half the font size wide
    let width =
        text_width - f64::from(props.indent.left.unwrap_or(0) + props.indent.right.unwrap_or(0));
    let per_line = (width / (size_twips * 0.5)).max(10.0);
    let characters = paragraph.plain_text().chars().count() as f64;
    let lines = (characters / per_line).ceil().max(1.0);
    let spacing = f64::from(props.spacing.before.unwrap_or(0) + props.spacing.after.unwrap_or(0));
    lines * line_height + spacing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_characters() {
        assert_eq!(word_count("Hello, world — again"), 3);
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("One two three."),
            Block::text(""),
            Block::text("Four"),
        ];
        let counts = count(&document);
        assert_eq!(counts.words, 4);
        assert_eq!(counts.paragraphs, 2);
        assert_eq!(counts.characters, 18);
        assert_eq!(counts.characters_without_spaces, 16);
        assert_eq!(counts.pages_estimate, 1);
    }

    #[test]
    fn many_double_spaced_lines_fill_pages() {
        let mut document = Document::new();
        document.styles.styles[0].paragraph.spacing.line = Some(crate::model::LineSpacing::DOUBLE);
        document.styles.styles[0].run.size = Some(24);
        let sentence = "This is a sentence of a student essay that goes on. ".repeat(6);
        document.blocks = (0..60).map(|_| Block::text(&sentence)).collect();
        let pages = count(&document).pages_estimate;
        // about 300 words a page double spaced; 60 paragraphs of 60 words
        assert!((9..=16).contains(&pages), "{pages}");
    }
}
