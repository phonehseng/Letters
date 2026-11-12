//! stacking blocks top to bottom without pages: headers, footers, table
//! cells, text boxes. spacing follows the same rules as the body, but
//! nothing ever moves to another page.

use letters_office::model::Block;

use crate::pages::emit::{emit_line, LinePlacement};
use crate::pages::{Context, EmittedBlocks, PreparedParagraph};
use crate::result::Story;
use crate::text::FieldValues;

/// lays `blocks` out from `top` down in a box at page x `left` of
/// `width`, and returns what they draw and how tall they are.
#[allow(clippy::too_many_arguments)]
pub fn stack_blocks(
    context: &mut Context,
    blocks: &[Block],
    story: &Story,
    path: &[u32],
    left: f64,
    width: f64,
    top: f64,
    fields: FieldValues,
) -> (EmittedBlocks, f64) {
    let mut out = EmittedBlocks::default();
    let mut y = top;
    let mut previous: Option<(Option<String>, f64, bool)> = None;
    for (index, block) in blocks.iter().enumerate() {
        let mut block_path = path.to_vec();
        block_path.push(index as u32);
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                let paragraph = block.paragraph().expect("paragraph block");
                let prepared =
                    context.prepare_paragraph(paragraph, story.clone(), block_path, width, fields);
                y += spacing(&previous, &prepared, context.flavor);
                y = draw_paragraph(context, &prepared, left, y, &mut out);
                previous = Some((
                    prepared.style.style_id.clone(),
                    prepared.style.space_after,
                    prepared.style.contextual_spacing,
                ));
            }
            Block::Table(table) => {
                if let Some((_, after, _)) = previous.take() {
                    y += after;
                }
                let prepared = crate::tables::prepare_table(
                    context,
                    table,
                    story,
                    &block_path,
                    width,
                    fields,
                    0,
                );
                y = crate::tables::draw_whole_table(context, &prepared, left, y, &mut out);
            }
            _ => {}
        }
    }
    if let Some((_, after, _)) = previous {
        y += after;
    }
    (out, y - top)
}

/// the space between two stacked paragraphs; the first one keeps its
/// space before (a header's first paragraph is not at a page top).
fn spacing(
    previous: &Option<(Option<String>, f64, bool)>,
    next: &PreparedParagraph,
    flavor: crate::Flavor,
) -> f64 {
    let style = &next.style;
    let rules = flavor.rules();
    match previous {
        None => style.space_before,
        Some((style_id, after, contextual)) => {
            let same = *style_id == style.style_id && rules.contextual_spacing;
            let after = if *contextual && same { 0.0 } else { *after };
            let before = if style.contextual_spacing && same {
                0.0
            } else {
                style.space_before
            };
            crate::pages::spacing_between(after, before)
        }
    }
}

/// draws every line of a prepared paragraph from `top`; returns the y
/// below its last line.
pub fn draw_paragraph(
    context: &Context,
    prepared: &PreparedParagraph,
    left: f64,
    top: f64,
    out: &mut EmittedBlocks,
) -> f64 {
    let mut y = top;
    for line in &prepared.layout.lines {
        let placement = LinePlacement {
            story: prepared.story.clone(),
            block: &prepared.path,
            left,
            top: y,
        };
        let emitted = emit_line(
            &prepared.text,
            &prepared.layout,
            line,
            &placement,
            &context.fonts,
        );
        out.lines.push(emitted.line);
        out.rects.extend(emitted.rects);
        out.images.extend(emitted.images);
        y += line.vertical.height;
    }
    y
}
