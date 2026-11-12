//! list labels: the "3." or bullet in front of a list paragraph, its
//! formatting and what separates it from the text.
//!
//! counting follows the office crate's `ListCounter` (word's rules:
//! instances of one definition count on from each other, deeper levels
//! restart when a shallower one moves on). paragraphs in tables count in
//! document order, as word counts them.

use std::collections::HashMap;

use letters_office::model::{Alignment, Block, Document, ListLevel, ParagraphProps, RunProps};
use letters_office::numbering::ListCounter;
use letters_office::styles;

use crate::flavor::Flavor;
use crate::style::{preserved, run_style_from_props, RunStyle};
use crate::text::LabelSuffix;

/// a paragraph's list label, ready to shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    pub style: RunStyle,
    pub suffix: LabelSuffix,
    /// how the label sits at the number position (w:lvlJc)
    pub alignment: Alignment,
}

/// labels of every list paragraph in the body, by block path.
pub fn body_labels(document: &Document, flavor: Flavor) -> HashMap<Vec<u32>, Label> {
    let mut counter = ListCounter::new(document);
    let mut labels = HashMap::new();
    for (index, block) in document.blocks.iter().enumerate() {
        visit(
            document,
            block,
            vec![index as u32],
            &mut counter,
            &mut labels,
            flavor,
            0,
        );
    }
    labels
}

fn visit(
    document: &Document,
    block: &Block,
    path: Vec<u32>,
    counter: &mut ListCounter,
    labels: &mut HashMap<Vec<u32>, Label>,
    flavor: Flavor,
    depth: usize,
) {
    // tables nest; a damaged file could nest without end
    if depth > 16 {
        return;
    }
    match block {
        Block::Paragraph(_) | Block::Heading(_) => {
            let paragraph = block.paragraph().expect("paragraph block");
            if let Some(label) = label_for(document, paragraph, counter, flavor) {
                labels.insert(path, label);
            }
        }
        Block::Table(table) => {
            for (r, row) in table.rows.iter().enumerate() {
                for (c, cell) in row.cells.iter().enumerate() {
                    for (b, inner) in cell.blocks.iter().enumerate() {
                        let mut inner_path = path.clone();
                        inner_path.extend([r as u32, c as u32, b as u32]);
                        visit(
                            document,
                            inner,
                            inner_path,
                            counter,
                            labels,
                            flavor,
                            depth + 1,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

/// the label of one paragraph, advancing the counter.
pub fn label_for(
    document: &Document,
    paragraph: &letters_office::model::Paragraph,
    counter: &mut ListCounter,
    flavor: Flavor,
) -> Option<Label> {
    let props = styles::resolved_paragraph_props(document, paragraph);
    let list = props.list.clone().filter(|list| list.list_id != "0")?;
    let text = counter.next_label(&list.list_id, list.level)?;
    let level = document.numbering.level(&list.list_id, list.level)?;
    Some(Label {
        text,
        style: label_style(document, paragraph, &props, level, flavor),
        suffix: suffix_of(level),
        alignment: level.alignment.unwrap_or(Alignment::Left),
    })
}

/// the label is formatted like the paragraph mark, then the level's own
/// run properties on top (a bullet's font). underline and highlight of
/// the mark do not reach the label in word.
fn label_style(
    document: &Document,
    paragraph: &letters_office::model::Paragraph,
    props: &ParagraphProps,
    level: &ListLevel,
    flavor: Flavor,
) -> RunStyle {
    let mut run: RunProps =
        styles::resolved_run_props(document, paragraph.style_id.as_deref(), &[]);
    if let Some(size) = preserved::number(&props.preserved, "w:sz") {
        run.size = Some(size.max(1) as u32);
    }
    run.merge(&level.run);
    run.underline = None;
    run.highlight = None;
    run_style_from_props(&run, flavor)
}

/// w:suff: tab (the default), space or nothing.
fn suffix_of(level: &ListLevel) -> LabelSuffix {
    match preserved::find(&level.preserved.children, "w:suff")
        .and_then(|tag| tag.attr("w:val").map(str::to_string))
        .as_deref()
    {
        Some("space") => LabelSuffix::Space,
        Some("nothing") => LabelSuffix::Nothing,
        _ => LabelSuffix::Tab,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{ListDefinition, ListRef, Paragraph};

    fn list_document(levels: &[u8]) -> Document {
        let mut document = Document::new();
        document.blocks.clear();
        document
            .numbering
            .definitions
            .push(ListDefinition::standard("1", true));
        document.numbering.add_instance("1");
        for level in levels {
            let mut paragraph = Paragraph::with_text("item");
            paragraph.props.list = Some(ListRef {
                list_id: "1".into(),
                level: *level,
            });
            document.blocks.push(Block::Paragraph(paragraph));
        }
        document
    }

    #[test]
    fn numbers_count_and_restart_by_level() {
        let document = list_document(&[0, 0, 1, 1, 0, 1]);
        let labels = body_labels(&document, Flavor::Word);
        let texts: Vec<String> = (0..6).map(|i| labels[&vec![i]].text.clone()).collect();
        assert_eq!(texts, vec!["1.", "2.", "a.", "b.", "3.", "a."]);
        assert_eq!(labels[&vec![0]].suffix, LabelSuffix::Tab);
    }

    #[test]
    fn suffix_comes_from_the_level() {
        let mut document = list_document(&[0]);
        document.numbering.definitions[0].levels[0]
            .preserved
            .children
            .push(r#"<w:suff w:val="space"/>"#.into());
        let labels = body_labels(&document, Flavor::Word);
        assert_eq!(labels[&vec![0]].suffix, LabelSuffix::Space);
    }

    #[test]
    fn paragraphs_outside_lists_have_no_label() {
        let mut document = list_document(&[0]);
        document.blocks.push(Block::text("plain"));
        assert_eq!(body_labels(&document, Flavor::Word).len(), 1);
    }
}
