//! "smart indentation": first-line and hanging indents set the proper
//! way, and the common habit of indenting with tabs or spaces turned into
//! real indents, so every paragraph lines up.

use std::collections::HashMap;

use crate::model::{Block, Document, Inline, Paragraph, Twips};

/// half an inch, the usual first-line and hanging indent.
pub const HALF_INCH: Twips = 720;

pub fn set_first_line_indent(paragraph: &mut Paragraph, amount: Twips) {
    paragraph.props.indent.hanging = None;
    paragraph.props.indent.first_line = Some(amount);
}

/// a hanging indent: the first line starts at the margin and the rest
/// of the lines `amount` further in (reference lists use this).
pub fn set_hanging_indent(paragraph: &mut Paragraph, amount: Twips) {
    paragraph.props.indent.first_line = None;
    paragraph.props.indent.hanging = Some(amount);
    paragraph.props.indent.left = Some(amount);
}

/// body paragraphs: not headings, not list items, not empty.
fn is_body(block: &Block) -> bool {
    matches!(block, Block::Paragraph(p) if p.props.list.is_none() && !p.plain_text().trim().is_empty())
}

/// turns leading tabs and runs of spaces into a first-line indent and
/// returns how many paragraphs changed. one tab, or four or more spaces,
/// counts as one half-inch step.
pub fn fix_manual_indents(document: &mut Document) -> usize {
    let mut changed = 0;
    for block in &mut document.blocks {
        if !is_body(block) {
            continue;
        }
        let Some(paragraph) = block.paragraph_mut() else {
            continue;
        };
        let steps = remove_leading_whitespace(paragraph);
        if steps > 0 {
            set_first_line_indent(paragraph, HALF_INCH * steps as Twips);
            changed += 1;
        }
    }
    changed
}

/// removes leading tabs and spaces, returning how many indent steps they
/// stood for.
fn remove_leading_whitespace(paragraph: &mut Paragraph) -> usize {
    let mut tabs = 0;
    let mut spaces = 0;
    while let Some(first) = paragraph.content.first_mut() {
        match first {
            Inline::Tab { .. } => {
                tabs += 1;
                paragraph.content.remove(0);
            }
            Inline::Text { text, .. } => {
                let trimmed = text.trim_start_matches([' ', '\t', '\u{A0}']);
                let removed = &text[..text.len() - trimmed.len()];
                tabs += removed.matches('\t').count();
                spaces += removed
                    .chars()
                    .filter(|c| *c == ' ' || *c == '\u{A0}')
                    .count();
                let rest = trimmed.to_string();
                if rest.is_empty() {
                    paragraph.content.remove(0);
                } else {
                    *text = rest;
                    break;
                }
            }
            _ => break,
        }
    }
    tabs + spaces / 4
}

/// gives every body paragraph that has a first-line indent the most common
/// one, so near misses (0.49 in, 0.6 in) line up. returns how many changed.
pub fn make_indents_consistent(document: &mut Document) -> usize {
    let mut counts: HashMap<Twips, usize> = HashMap::new();
    for block in &document.blocks {
        if let (true, Some(paragraph)) = (is_body(block), block.paragraph()) {
            if let Some(amount) = paragraph.props.indent.first_line.filter(|a| *a > 0) {
                *counts.entry(amount).or_default() += 1;
            }
        }
    }
    let Some((&usual, _)) = counts
        .iter()
        .max_by_key(|(amount, count)| (**count, -**amount))
    else {
        return 0;
    };
    let mut changed = 0;
    for block in &mut document.blocks {
        if !is_body(block) {
            continue;
        }
        let Some(paragraph) = block.paragraph_mut() else {
            continue;
        };
        if let Some(amount) = paragraph.props.indent.first_line {
            if amount > 0 && amount != usual {
                paragraph.props.indent.first_line = Some(usual);
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_and_spaces_become_real_indents() {
        let mut document = Document::new();
        let mut tabbed = Paragraph::with_text("Tabbed start.");
        tabbed.content.insert(0, Inline::Tab { marks: vec![] });
        document.blocks = vec![
            Block::Paragraph(tabbed),
            Block::text("    Four spaces."),
            Block::text("No indent."),
        ];
        assert_eq!(fix_manual_indents(&mut document), 2);
        let first = document.blocks[0].paragraph().unwrap();
        assert_eq!(first.plain_text(), "Tabbed start.");
        assert_eq!(first.props.indent.first_line, Some(720));
        assert_eq!(
            document.blocks[1].paragraph().unwrap().plain_text(),
            "Four spaces."
        );
        assert_eq!(
            document.blocks[2]
                .paragraph()
                .unwrap()
                .props
                .indent
                .first_line,
            None
        );
    }

    #[test]
    fn near_misses_line_up() {
        let mut document = Document::new();
        document.blocks = [720, 720, 700, 864]
            .iter()
            .map(|amount| {
                let mut paragraph = Paragraph::with_text("text");
                set_first_line_indent(&mut paragraph, *amount);
                Block::Paragraph(paragraph)
            })
            .collect();
        assert_eq!(make_indents_consistent(&mut document), 2);
        assert!(document
            .blocks
            .iter()
            .all(|b| b.paragraph().unwrap().props.indent.first_line == Some(720)));
    }

    #[test]
    fn hanging_indent_sets_both_sides() {
        let mut paragraph = Paragraph::with_text("Author, A. (2020). Title.");
        set_hanging_indent(&mut paragraph, HALF_INCH);
        assert_eq!(paragraph.props.indent.first_line_offset(), -720);
        assert_eq!(paragraph.props.indent.left, Some(720));
    }
}
