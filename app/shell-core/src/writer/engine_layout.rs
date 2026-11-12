//! Letters' own page layout (`core/layout`), as the second provider of
//! line and page breaks, behind a setting. it gives the editor the same
//! answer word's layout does (layout.rs `WordLayout`), so the editor
//! applies either the same way. it follows word's rules but is not word,
//! and the status bar says which one is in use.

use std::time::Instant;

use letters_layout::{layout, LayoutOptions, Story};
use letters_office::model::{Block, Document, Inline};

use crate::writer::layout::{LineStart, WordLayout};

/// the engine's lines and pages for `document`.
pub fn engine_layout(document: &Document) -> WordLayout {
    let started = Instant::now();
    let result = layout(document, &LayoutOptions::default());
    let mut lines = Vec::new();
    for (index, page) in result.pages.iter().enumerate() {
        for line in &page.lines {
            if line.story != Story::Body {
                continue;
            }
            let Some(&block) = line.block.first() else {
                continue;
            };
            let block = block as usize;
            // a path of one is a top-level paragraph; longer ones are inside a table
            let cell = (line.block.len() >= 4).then(|| {
                (
                    line.block[1] as usize,
                    line.block[2] as usize,
                    line.block[3] as usize,
                )
            });
            lines.push(LineStart {
                page: index as u32 + 1,
                block,
                cell,
                offset: i64::from(line.start),
                // points to twips
                top: (line.top * 20.0).round() as i64,
                left: (line.x * 20.0).round() as i64,
                text: line_text(document, block, cell.is_none(), line.start as usize, 24),
            });
        }
    }
    let block_pages = first_pages(&lines, document.blocks.len());
    WordLayout {
        block_pages,
        page_count: result.pages.len().max(1) as u32,
        lines,
        shapes: Vec::new(),
        milliseconds: started.elapsed().as_millis() as u64,
        word_milliseconds: 0,
    }
}

/// the first characters of a line, counted in the engine's units (a
/// character of text is one, a tab or picture one, a bookmark none).
fn line_text(
    document: &Document,
    block: usize,
    top_level: bool,
    start: usize,
    length: usize,
) -> String {
    let Some(paragraph) = document
        .blocks
        .get(block)
        .and_then(Block::paragraph)
        .filter(|_| top_level)
    else {
        return String::new();
    };
    let mut text = String::new();
    let mut offset = 0;
    for inline in &paragraph.content {
        match inline {
            Inline::Text { text: part, .. } => {
                for character in part.chars() {
                    if offset >= start && text.chars().count() < length {
                        text.push(character);
                    }
                    offset += 1;
                }
            }
            Inline::BookmarkStart { .. }
            | Inline::BookmarkEnd { .. }
            | Inline::CommentReference { .. }
            | Inline::Preserved { .. } => {}
            _ => {
                if offset >= start && text.chars().count() < length {
                    text.push(if matches!(inline, Inline::Tab { .. }) {
                        '\t'
                    } else {
                        '\u{fffc}'
                    });
                }
                offset += 1;
            }
        }
    }
    text
}

fn first_pages(lines: &[LineStart], count: usize) -> Vec<u32> {
    let mut pages: Vec<Option<u32>> = vec![None; count];
    for line in lines {
        if let Some(slot) = pages.get_mut(line.block) {
            slot.get_or_insert(line.page);
        }
    }
    let mut last = 1;
    pages
        .into_iter()
        .map(|page| {
            last = page.unwrap_or(last);
            last
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_break_starts_page_two() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("first page"),
            Block::PageBreak,
            Block::text("second page"),
        ];
        let answer = engine_layout(&document);
        assert_eq!(answer.page_count, 2);
        assert_eq!(answer.block_pages.last(), Some(&2));
        assert_eq!(
            answer.lines.last().map(|line| line.text.as_str()),
            Some("second page")
        );
    }

    /// compares the engine's page and line breaks with word's on a file:
    /// LETTERS_VERIFY_DOCX (the file), LETTERS_VERIFY_OUT (the report).
    /// `cargo test -p letters-shell-core engine_against_word -- --ignored`
    #[test]
    #[ignore]
    fn engine_against_word() {
        let path = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_DOCX").unwrap());
        let out = std::path::PathBuf::from(std::env::var("LETTERS_VERIFY_OUT").unwrap());
        let document = letters_office::open(&std::fs::read(path).unwrap()).unwrap();
        let ours = engine_layout(&document);
        let cache = std::env::temp_dir().join("letters-engine-compare");
        let word = crate::writer::layout::word_layout(&document, &cache).unwrap();
        let starts = |layout: &WordLayout| {
            layout
                .lines
                .iter()
                .map(|line| (line.block, line.offset))
                .collect::<Vec<_>>()
        };
        let word_starts = starts(&word);
        let our_starts = starts(&ours);
        let same_lines = word_starts
            .iter()
            .filter(|start| our_starts.contains(start))
            .count();
        let page_starts = |layout: &WordLayout| {
            let mut found = Vec::new();
            let mut page = 0;
            for line in &layout.lines {
                if line.page != page {
                    page = line.page;
                    found.push((line.block, line.offset));
                }
            }
            found
        };
        let report = serde_json::json!({
            "wordPages": word.page_count, "enginePages": ours.page_count,
            "wordLines": word_starts.len(), "engineLines": our_starts.len(), "sameLineStarts": same_lines,
            "wordPageStarts": page_starts(&word), "enginePageStarts": page_starts(&ours),
            "engineMilliseconds": ours.milliseconds,
        });
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}
