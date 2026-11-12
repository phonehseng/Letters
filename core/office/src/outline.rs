//! headings: turning a paragraph into a heading in one step, and the
//! outline (the list of headings) the outline panel shows.

use serde::Serialize;

use crate::model::{Block, Document, Heading, Paragraph, Style, StyleKind};
use crate::styles::style_heading_level;

/// one line of the outline panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntry {
    /// index into `Document::blocks`, so a click can scroll there
    pub block_index: usize,
    pub level: u8,
    pub text: String,
}

/// every heading of the document, in order.
pub fn outline(document: &Document) -> Vec<OutlineEntry> {
    document
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(block_index, block)| match block {
            Block::Heading(heading) => Some(OutlineEntry {
                block_index,
                level: heading.level,
                text: heading.paragraph.plain_text().trim().to_string(),
            }),
            _ => None,
        })
        .collect()
}

/// makes the block at `index` a heading of `level` (1 to 6), giving it
/// the document's heading style for that level (created when missing).
/// level 0 turns a heading back into body text. returns false when the
/// block is not a paragraph or heading.
pub fn apply_heading(document: &mut Document, index: usize, level: u8) -> bool {
    let Some(block) = document.blocks.get(index) else {
        return false;
    };
    let Some(paragraph) = block.paragraph().cloned() else {
        return false;
    };
    let mut paragraph: Paragraph = paragraph;
    // direct outline levels would fight the style, so they go
    paragraph.props.outline_level = None;
    if level == 0 {
        paragraph.style_id = None;
        document.blocks[index] = Block::Paragraph(paragraph);
        return true;
    }
    let level = level.min(6);
    paragraph.style_id = Some(heading_style(document, level));
    document.blocks[index] = Block::Heading(Heading { level, paragraph });
    true
}

/// the id of the style for headings of `level`, adding one if the
/// document has none. an existing style named "heading N" is preferred,
/// so word and libreoffice recognise it as their own heading style.
pub fn heading_style(document: &mut Document, level: u8) -> String {
    let named = format!("heading {level}");
    let existing = document
        .styles
        .styles
        .iter()
        .filter(|style| style.kind == StyleKind::Paragraph)
        .filter(|style| style_heading_level(document, &style.id) == Some(level))
        .min_by_key(|style| !style.name.eq_ignore_ascii_case(&named))
        .map(|style| style.id.clone());
    if let Some(id) = existing {
        return id;
    }
    let style = new_heading_style(document, level);
    let id = style.id.clone();
    document.styles.styles.push(style);
    id
}

fn new_heading_style(document: &Document, level: u8) -> Style {
    let mut id = format!("Heading{level}");
    while document.style(&id).is_some() {
        id.push('x');
    }
    let mut style = Style::paragraph(&id, &format!("heading {level}"));
    style.based_on = crate::styles::default_paragraph_style(document).map(|s| s.id.clone());
    style.next = style.based_on.clone();
    style.paragraph.outline_level = Some(level - 1);
    style.paragraph.keep_with_next = Some(true);
    style.paragraph.spacing.before = Some(if level == 1 { 360 } else { 240 });
    style.paragraph.spacing.after = Some(120);
    style.run.bold = Some(true);
    // 16, 13, 12, 11, 11, 11 point: a clear but quiet scale
    style.run.size = Some(match level {
        1 => 32,
        2 => 26,
        3 => 24,
        _ => 22,
    });
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_in_one_step_and_the_outline() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::text("Introduction"),
            Block::text("Body"),
            Block::text("Method"),
        ];
        assert!(apply_heading(&mut document, 0, 1));
        assert!(apply_heading(&mut document, 2, 2));
        let entries = outline(&document);
        assert_eq!(
            entries,
            vec![
                OutlineEntry {
                    block_index: 0,
                    level: 1,
                    text: "Introduction".into()
                },
                OutlineEntry {
                    block_index: 2,
                    level: 2,
                    text: "Method".into()
                },
            ]
        );
        // the style is made once and reused
        assert!(apply_heading(&mut document, 1, 1));
        let heading_styles = document
            .styles
            .styles
            .iter()
            .filter(|s| s.id.starts_with("Heading"))
            .count();
        assert_eq!(heading_styles, 2);
        assert!(apply_heading(&mut document, 1, 0));
        assert_eq!(outline(&document).len(), 2);
        assert!(!apply_heading(&mut document, 9, 1));
    }
}
