//! drawings found inside a paragraph while it is read. each reader leaves
//! a placeholder where a drawing sat, then `place_drawings` turns the
//! paragraph and its drawings into blocks the same way for docx and odt:
//! a paragraph holding only drawings becomes the drawings (remembering the
//! paragraph), a paragraph of text keeps its text and its drawings come
//! just before it, each remembering the inline it came before.

use crate::model::{AnchorWith, Block, Inline, Mark, ObjectAnchor, Paragraph};

/// the id of a placeholder; a control character keeps it apart from any
/// real bookmark id.
const PLACEHOLDER: &str = "\u{1}object";

/// a drawing read from a paragraph, with the formatting of the run that
/// held it (docx only).
pub(crate) type FoundDrawing = (Block, Vec<Mark>);

pub(crate) fn placeholder(number: usize) -> Inline {
    Inline::BookmarkEnd {
        id: format!("{PLACEHOLDER}{number}"),
    }
}

/// takes the drawings found since `first` out of a paragraph's content.
/// returns the content without placeholders, and each drawing with the
/// index of the inline it came before.
pub(crate) fn take_drawings(
    content: Vec<Inline>,
    pending: &mut Vec<FoundDrawing>,
    first: usize,
) -> (Vec<Inline>, Vec<(FoundDrawing, usize)>) {
    if pending.len() <= first {
        return (content, Vec::new());
    }
    let mut found: Vec<Option<FoundDrawing>> = pending.drain(first..).map(Some).collect();
    let mut kept = Vec::with_capacity(content.len());
    let mut drawings = Vec::new();
    for inline in content {
        let number = match &inline {
            Inline::BookmarkEnd { id } => id
                .strip_prefix(PLACEHOLDER)
                .and_then(|n| n.parse::<usize>().ok()),
            _ => None,
        };
        let Some(number) = number else {
            kept.push(inline);
            continue;
        };
        let taken = number
            .checked_sub(first)
            .and_then(|at| found.get_mut(at))
            .and_then(Option::take);
        if let Some(drawing) = taken {
            drawings.push((drawing, kept.len()));
        }
    }
    (kept, drawings)
}

/// pushes a paragraph and the drawings it held onto `blocks`. `paragraph`
/// is the paragraph's block as the reader made it (a paragraph, heading
/// or page break); `own` is that paragraph's formatting, kept on the first
/// drawing when the paragraph holds nothing else.
pub(crate) fn place_drawings(
    block: Block,
    own: &Paragraph,
    drawings: Vec<(FoundDrawing, usize)>,
    blocks: &mut Vec<Block>,
) {
    if drawings.is_empty() {
        blocks.push(block);
        return;
    }
    let only_drawings = own.content.is_empty() && !matches!(block, Block::PageBreak);
    let keep_paragraph = *own != Paragraph::default();
    for (index, ((mut drawing, marks), inline_index)) in drawings.into_iter().enumerate() {
        let anchor = if only_drawings {
            ObjectAnchor {
                with: if index == 0 {
                    AnchorWith::Own
                } else {
                    AnchorWith::Previous
                },
                paragraph: (index == 0 && keep_paragraph).then(|| own.clone()),
                inline_index: None,
                marks,
                page_number: None,
            }
        } else {
            ObjectAnchor {
                with: AnchorWith::Next,
                paragraph: None,
                inline_index: Some(inline_index),
                marks,
                page_number: None,
            }
        };
        if let Some(slot) = drawing.object_anchor_mut() {
            *slot = Some(anchor);
        }
        blocks.push(drawing);
    }
    if !only_drawings {
        blocks.push(block);
    }
}
