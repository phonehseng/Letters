//! moving things around the page: pictures between "in the line" and
//! "floating", text boxes made from text and dissolved back into it,
//! stacking order, aligning and distributing, and the snap guides the
//! editor shows while something is dragged.
//!
//! everything is a pure function on the document. positions the editor
//! works with are page coordinates: twips from the page's top-left corner.

use serde::{Deserialize, Serialize};

use crate::model::{
    Block, Document, FloatPosition, Image, Inline, Paragraph, Placement, TextBox, Twips, Wrap,
};

/// a floating object, by where it lives in the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObjectRef {
    TextBox { block: usize },
    Image { block: usize, inline: usize },
}

/// a box in page coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: Twips,
    pub y: Twips,
    pub width: Twips,
    pub height: Twips,
}

impl Rect {
    fn right(&self) -> Twips {
        self.x + self.width
    }
    fn bottom(&self) -> Twips {
        self.y + self.height
    }
}

fn image_mut(document: &mut Document, block: usize, inline: usize) -> Option<&mut Image> {
    match document
        .blocks
        .get_mut(block)?
        .paragraph_mut()?
        .content
        .get_mut(inline)?
    {
        Inline::Image(image) => Some(image),
        _ => None,
    }
}

/// floats a picture that sits in the line. a picture that floated before
/// gets its old place back exactly; otherwise `hint` (page coordinates of
/// where it shows now) decides, or the top of its paragraph.
pub fn float_image(
    document: &mut Document,
    block: usize,
    inline: usize,
    hint: Option<(Twips, Twips)>,
    wrap: Wrap,
) -> bool {
    let Some(image) = image_mut(document, block, inline) else {
        return false;
    };
    if image.placement != Placement::Inline {
        return true;
    }
    image.placement = match (image.previous_placement.take(), hint) {
        (Some(previous), None) => *previous,
        (_, Some((x, y))) => Placement::Floating {
            horizontal: FloatPosition::new("page", x),
            vertical: FloatPosition::new("page", y),
            wrap,
        },
        (None, None) => Placement::Floating {
            horizontal: FloatPosition::new("column", 0),
            vertical: FloatPosition::new("paragraph", 0),
            wrap,
        },
    };
    true
}

/// puts a floating picture back in the line at `to` (block index and
/// character offset in that paragraph), or where its anchor is. its
/// floating geometry is kept, so floating it again is lossless. returns
/// the picture's new place.
pub fn inline_image(
    document: &mut Document,
    block: usize,
    inline: usize,
    to: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let image = image_mut(document, block, inline)?;
    if image.placement != Placement::Inline {
        let previous = std::mem::take(&mut image.placement);
        image.previous_placement = Some(Box::new(previous));
    }
    let Some((target_block, offset)) = to else {
        return Some((block, inline));
    };
    let taken = document.blocks[block]
        .paragraph_mut()?
        .content
        .remove(inline);
    let Some(paragraph) = document
        .blocks
        .get_mut(target_block)
        .and_then(Block::paragraph_mut)
    else {
        // nowhere to go: put it back where it was
        document.blocks[block]
            .paragraph_mut()?
            .content
            .insert(inline, taken);
        return Some((block, inline));
    };
    let index = split_at_character(&mut paragraph.content, offset);
    paragraph.content.insert(index, taken);
    Some((target_block, index))
}

/// splits text so that a boundary falls `offset` characters into the
/// paragraph, and returns the inline index at that boundary. pictures,
/// tabs and other single things count as one character.
pub fn split_at_character(content: &mut Vec<Inline>, offset: usize) -> usize {
    let mut remaining = offset;
    let mut index = 0;
    while index < content.len() {
        if remaining == 0 {
            return index;
        }
        let length = match &content[index] {
            Inline::Text { text, .. } => text.chars().count(),
            Inline::BookmarkStart { .. }
            | Inline::BookmarkEnd { .. }
            | Inline::CommentReference { .. } => 0,
            _ => 1,
        };
        if remaining < length {
            if let Inline::Text { text, marks } = &content[index] {
                let split: usize = text
                    .char_indices()
                    .nth(remaining)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len());
                let tail = Inline::Text {
                    text: text[split..].to_string(),
                    marks: marks.clone(),
                };
                let head = Inline::Text {
                    text: text[..split].to_string(),
                    marks: marks.clone(),
                };
                content[index] = head;
                content.insert(index + 1, tail);
                return index + 1;
            }
        }
        remaining -= length.min(remaining);
        index += 1;
    }
    content.len()
}

/// turns one paragraph into two text boxes, the text before `offset` in
/// the first and the rest in the second, each keeping its formatting, so
/// the two halves can be moved around separately. returns the two block
/// indexes.
pub fn split_paragraph_into_text_boxes(
    document: &mut Document,
    block: usize,
    offset: usize,
) -> Option<(usize, usize)> {
    let paragraph = document.blocks.get(block)?.paragraph()?.clone();
    let section = document.section_of(block);
    let mut content = paragraph.content.clone();
    let boundary = split_at_character(&mut content, offset);
    let tail = content.split_off(boundary);
    let make = |content: Vec<Inline>| {
        Block::Paragraph(Paragraph {
            content,
            ..paragraph.clone()
        })
    };
    let width = section.page.text_width() / 2 - 144;
    let top = section.page.margins.top;
    let left = section.page.margins.left;
    let mut first = TextBox::new(left, top, width, 1440, vec![make(content)]);
    first.auto_height = true;
    let mut second = TextBox::new(left + width + 288, top, width, 1440, vec![make(tail)]);
    second.auto_height = true;
    let z = next_z(document);
    first.z_order = Some(z);
    second.z_order = Some(z + 1);
    document.blocks.splice(
        block..=block,
        [Block::TextBox(first), Block::TextBox(second)],
    );
    Some((block, block + 1))
}

/// moves blocks `start..end` into one new text box at the top-left
/// margin corner, and returns its index.
pub fn text_box_from_blocks(document: &mut Document, start: usize, end: usize) -> Option<usize> {
    if start >= end || end > document.blocks.len() {
        return None;
    }
    let moved: Vec<Block> = document.blocks[start..end].to_vec();
    if moved
        .iter()
        .any(|b| matches!(b, Block::SectionBreak | Block::TextBox(_)))
    {
        // a section break cannot live in a box, nor can another box
        return None;
    }
    let section = document.section_of(start);
    let mut text_box = TextBox::new(
        section.page.margins.left,
        section.page.margins.top,
        section.page.text_width() / 2,
        1440,
        moved,
    );
    text_box.auto_height = true;
    text_box.z_order = Some(next_z(document));
    document
        .blocks
        .splice(start..end, [Block::TextBox(text_box)]);
    Some(start)
}

/// takes the content out of a text box and puts it back in the flow at
/// block index `at` (counted after the box is removed). returns the range
/// of the restored blocks.
pub fn dissolve_text_box(
    document: &mut Document,
    block: usize,
    at: usize,
) -> Option<std::ops::Range<usize>> {
    let Block::TextBox(_) = document.blocks.get(block)? else {
        return None;
    };
    let Block::TextBox(text_box) = document.blocks.remove(block) else {
        return None;
    };
    let at = at.min(document.blocks.len());
    let count = text_box.blocks.len();
    document.blocks.splice(at..at, text_box.blocks);
    Some(at..at + count)
}

/// every floating object of the main text, in document order.
pub fn floating_objects(document: &Document) -> Vec<ObjectRef> {
    let mut objects = Vec::new();
    for (block, item) in document.blocks.iter().enumerate() {
        if let Block::TextBox(_) = item {
            objects.push(ObjectRef::TextBox { block });
        }
        if let Some(paragraph) = item.paragraph() {
            for (inline, content) in paragraph.content.iter().enumerate() {
                if matches!(content, Inline::Image(image) if image.placement != Placement::Inline) {
                    objects.push(ObjectRef::Image { block, inline });
                }
            }
        }
    }
    objects
}

fn z_of(document: &Document, object: ObjectRef) -> Option<i64> {
    match object {
        ObjectRef::TextBox { block } => match document.blocks.get(block)? {
            Block::TextBox(text_box) => Some(text_box.z_order.unwrap_or(0)),
            _ => None,
        },
        ObjectRef::Image { block, inline } => match document
            .blocks
            .get(block)?
            .paragraph()?
            .content
            .get(inline)?
        {
            Inline::Image(image) => Some(image.z_order.unwrap_or(0)),
            _ => None,
        },
    }
}

fn set_z(document: &mut Document, object: ObjectRef, z: i64) {
    match object {
        ObjectRef::TextBox { block } => {
            if let Some(Block::TextBox(text_box)) = document.blocks.get_mut(block) {
                text_box.z_order = Some(z);
            }
        }
        ObjectRef::Image { block, inline } => {
            if let Some(image) = image_mut(document, block, inline) {
                image.z_order = Some(z);
            }
        }
    }
}

fn next_z(document: &Document) -> i64 {
    floating_objects(document)
        .into_iter()
        .filter_map(|object| z_of(document, object))
        .max()
        .map(|z| z + 1)
        .unwrap_or(1)
}

/// puts an object in front of every other.
pub fn bring_to_front(document: &mut Document, object: ObjectRef) {
    let z = next_z(document);
    set_z(document, object, z);
}

/// puts an object behind every other. stacking numbers stay positive (word
/// needs that), so the others move up one when needed.
pub fn send_to_back(document: &mut Document, object: ObjectRef) {
    let others: Vec<ObjectRef> = floating_objects(document)
        .into_iter()
        .filter(|o| *o != object)
        .collect();
    let lowest = others
        .iter()
        .filter_map(|o| z_of(document, *o))
        .min()
        .unwrap_or(1);
    if lowest <= 1 {
        for other in &others {
            let z = z_of(document, *other).unwrap_or(0) + 2 - lowest;
            set_z(document, *other, z);
        }
        set_z(document, object, 1);
    } else {
        set_z(document, object, lowest - 1);
    }
}

/// where an object sits on its page, when that can be known without
/// laying out the text: positions relative to the page, the margins or
/// the column. positions relative to a paragraph or line give `None`.
pub fn page_rect(document: &Document, object: ObjectRef) -> Option<Rect> {
    let block = match object {
        ObjectRef::TextBox { block } | ObjectRef::Image { block, .. } => block,
    };
    let (horizontal, vertical, width, height) = geometry(document, object)?;
    let margins = document.section_of(block).page.margins;
    let x = match horizontal.relative_to.as_str() {
        "page" => horizontal.offset,
        "margin" | "column" => margins.left + horizontal.offset,
        _ => return None,
    };
    let y = match vertical.relative_to.as_str() {
        "page" => vertical.offset,
        "margin" => margins.top + vertical.offset,
        _ => return None,
    };
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

fn geometry(
    document: &Document,
    object: ObjectRef,
) -> Option<(FloatPosition, FloatPosition, Twips, Twips)> {
    match object {
        ObjectRef::TextBox { block } => match document.blocks.get(block)? {
            Block::TextBox(t) => {
                Some((t.horizontal.clone(), t.vertical.clone(), t.width, t.height))
            }
            _ => None,
        },
        ObjectRef::Image { block, inline } => match document
            .blocks
            .get(block)?
            .paragraph()?
            .content
            .get(inline)?
        {
            Inline::Image(Image {
                placement:
                    Placement::Floating {
                        horizontal,
                        vertical,
                        ..
                    },
                width,
                height,
                ..
            }) => Some((horizontal.clone(), vertical.clone(), *width, *height)),
            _ => None,
        },
    }
}

/// moves an object to page coordinates `x`, `y` (it is then positioned
/// relative to the page).
pub fn move_to(document: &mut Document, object: ObjectRef, x: Twips, y: Twips) -> bool {
    let (horizontal, vertical) = (FloatPosition::new("page", x), FloatPosition::new("page", y));
    match object {
        ObjectRef::TextBox { block } => match document.blocks.get_mut(block) {
            Some(Block::TextBox(text_box)) => {
                text_box.horizontal = horizontal;
                text_box.vertical = vertical;
                true
            }
            _ => false,
        },
        ObjectRef::Image { block, inline } => match image_mut(document, block, inline) {
            Some(Image {
                placement:
                    Placement::Floating {
                        horizontal: h,
                        vertical: v,
                        ..
                    },
                ..
            }) => {
                *h = horizontal;
                *v = vertical;
                true
            }
            _ => false,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// lines objects up on the edge or centre they share with the first one
/// that is furthest out. objects whose page position is unknown are left
/// alone; the ones moved are returned.
pub fn align(document: &mut Document, objects: &[ObjectRef], edge: Edge) -> Vec<ObjectRef> {
    let placed: Vec<(ObjectRef, Rect)> = objects
        .iter()
        .filter_map(|o| Some((*o, page_rect(document, *o)?)))
        .collect();
    if placed.len() < 2 {
        return Vec::new();
    }
    let target = match edge {
        Edge::Left => placed.iter().map(|(_, r)| r.x).min(),
        Edge::Right => placed.iter().map(|(_, r)| r.right()).max(),
        Edge::Top => placed.iter().map(|(_, r)| r.y).min(),
        Edge::Bottom => placed.iter().map(|(_, r)| r.bottom()).max(),
        Edge::Center => Some(
            placed.iter().map(|(_, r)| r.x + r.width / 2).sum::<Twips>() / placed.len() as Twips,
        ),
        Edge::Middle => Some(
            placed
                .iter()
                .map(|(_, r)| r.y + r.height / 2)
                .sum::<Twips>()
                / placed.len() as Twips,
        ),
    }
    .unwrap_or(0);
    for (object, rect) in &placed {
        let (x, y) = match edge {
            Edge::Left => (target, rect.y),
            Edge::Right => (target - rect.width, rect.y),
            Edge::Center => (target - rect.width / 2, rect.y),
            Edge::Top => (rect.x, target),
            Edge::Bottom => (rect.x, target - rect.height),
            Edge::Middle => (rect.x, target - rect.height / 2),
        };
        move_to(document, *object, x, y);
    }
    placed.into_iter().map(|(o, _)| o).collect()
}

/// spaces three or more objects evenly between the outermost two.
pub fn distribute(
    document: &mut Document,
    objects: &[ObjectRef],
    horizontal: bool,
) -> Vec<ObjectRef> {
    let mut placed: Vec<(ObjectRef, Rect)> = objects
        .iter()
        .filter_map(|o| Some((*o, page_rect(document, *o)?)))
        .collect();
    if placed.len() < 3 {
        return Vec::new();
    }
    let start = |r: &Rect| if horizontal { r.x } else { r.y };
    let size = |r: &Rect| if horizontal { r.width } else { r.height };
    placed.sort_by_key(|(_, r)| start(r));
    let first = start(&placed[0].1);
    let last = placed
        .last()
        .map(|(_, r)| start(r) + size(r))
        .unwrap_or(first);
    let total: Twips = placed.iter().map(|(_, r)| size(r)).sum();
    let gap = (last - first - total) / (placed.len() as Twips - 1);
    let mut position = first;
    for (object, rect) in &placed {
        let (x, y) = if horizontal {
            (position, rect.y)
        } else {
            (rect.x, position)
        };
        move_to(document, *object, x, y);
        position += size(rect) + gap;
    }
    placed.into_iter().map(|(o, _)| o).collect()
}

/// one line the editor can show while dragging.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    /// true for a vertical line (an x position), false for a horizontal one
    pub vertical: bool,
    pub position: Twips,
    /// "page edge", "margin", "page centre", "object edge", "object centre"
    pub kind: &'static str,
}

/// where a dragged box should snap to, and the guides that explain it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snap {
    pub rect: Rect,
    pub guides: Vec<Guide>,
}

/// snaps `proposed` (page coordinates of the dragged object) to page edges,
/// margins, the page's centre lines and the edges and centres of other
/// objects within `threshold` twips. `moving` is left out of the targets.
pub fn snap(
    document: &Document,
    block_for_section: usize,
    moving: Option<ObjectRef>,
    proposed: Rect,
    threshold: Twips,
) -> Snap {
    let page = document.section_of(block_for_section).page;
    let mut vertical_lines = vec![
        (0, "page edge"),
        (page.width, "page edge"),
        (page.margins.left, "margin"),
        (page.width - page.margins.right, "margin"),
        (page.width / 2, "page centre"),
    ];
    let mut horizontal_lines = vec![
        (0, "page edge"),
        (page.height, "page edge"),
        (page.margins.top, "margin"),
        (page.height - page.margins.bottom, "margin"),
        (page.height / 2, "page centre"),
    ];
    for object in floating_objects(document) {
        if Some(object) == moving {
            continue;
        }
        if let Some(r) = page_rect(document, object) {
            vertical_lines.extend([
                (r.x, "object edge"),
                (r.right(), "object edge"),
                (r.x + r.width / 2, "object centre"),
            ]);
            horizontal_lines.extend([
                (r.y, "object edge"),
                (r.bottom(), "object edge"),
                (r.y + r.height / 2, "object centre"),
            ]);
        }
    }
    let mut rect = proposed;
    let mut guides = Vec::new();
    // the box's left edge, centre and right edge, each against every line
    let candidates_x = [
        (0, rect.x),
        (rect.width / 2, rect.x + rect.width / 2),
        (rect.width, rect.right()),
    ];
    if let Some((shift, line, kind)) = best(&candidates_x, &vertical_lines, threshold) {
        rect.x = line - shift;
        guides.push(Guide {
            vertical: true,
            position: line,
            kind,
        });
    }
    let candidates_y = [
        (0, rect.y),
        (rect.height / 2, rect.y + rect.height / 2),
        (rect.height, rect.bottom()),
    ];
    if let Some((shift, line, kind)) = best(&candidates_y, &horizontal_lines, threshold) {
        rect.y = line - shift;
        guides.push(Guide {
            vertical: false,
            position: line,
            kind,
        });
    }
    Snap { rect, guides }
}

/// the closest pairing of a box position and a line within the threshold.
fn best(
    candidates: &[(Twips, Twips)],
    lines: &[(Twips, &'static str)],
    threshold: Twips,
) -> Option<(Twips, Twips, &'static str)> {
    let mut found: Option<(Twips, Twips, Twips, &'static str)> = None;
    for (shift, position) in candidates {
        for (line, kind) in lines {
            let distance = (position - line).abs();
            if distance <= threshold && found.is_none_or(|(d, ..)| distance < d) {
                found = Some((distance, *shift, *line, *kind));
            }
        }
    }
    found.map(|(_, shift, line, kind)| (shift, line, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mark;

    fn document_with_image(placement: Placement) -> Document {
        let mut document = Document::new();
        document.blocks = vec![Block::Paragraph(Paragraph {
            content: vec![
                Inline::text("Before "),
                Inline::Image(Image {
                    media_id: "a.png".into(),
                    width: 1440,
                    height: 720,
                    placement,
                    ..Default::default()
                }),
                Inline::text("after."),
            ],
            ..Default::default()
        })];
        document
    }

    #[test]
    fn floating_and_back_is_lossless() {
        let floating = Placement::Floating {
            horizontal: FloatPosition {
                relative_to: "page".into(),
                offset: 2000,
                align: None,
                offset_emu: Some(1_270_123),
            },
            vertical: FloatPosition::new("paragraph", 300),
            wrap: Wrap::Tight,
        };
        let mut document = document_with_image(floating.clone());
        let place = inline_image(&mut document, 0, 1, Some((0, 3))).unwrap();
        assert_eq!(place, (0, 1));
        assert_eq!(
            document.blocks[0].paragraph().unwrap().plain_text(),
            "Before after."
        );
        assert!(float_image(&mut document, 0, 1, None, Wrap::Square));
        let Inline::Image(image) = &document.blocks[0].paragraph().unwrap().content[1] else {
            panic!()
        };
        assert_eq!(image.placement, floating);
        assert!(image.previous_placement.is_none());
    }

    #[test]
    fn a_paragraph_splits_into_two_boxes_with_formatting() {
        let mut document = Document::new();
        document.blocks = vec![Block::Paragraph(Paragraph {
            content: vec![
                Inline::styled("Hello bold", vec![Mark::Bold]),
                Inline::text(" world"),
            ],
            ..Default::default()
        })];
        let (a, b) = split_paragraph_into_text_boxes(&mut document, 0, 6).unwrap();
        let text = |index: usize| match &document.blocks[index] {
            Block::TextBox(t) => t.blocks[0].paragraph().unwrap().plain_text(),
            _ => panic!(),
        };
        assert_eq!(text(a), "Hello ");
        assert_eq!(text(b), "bold world");
        let Block::TextBox(second) = &document.blocks[b] else {
            panic!()
        };
        assert_eq!(
            second.blocks[0].paragraph().unwrap().content[0].marks(),
            &[Mark::Bold]
        );
        let range = dissolve_text_box(&mut document, b, 1).unwrap();
        assert_eq!(range, 1..2);
        assert!(matches!(document.blocks[1], Block::Paragraph(_)));
    }

    #[test]
    fn stacking_alignment_distribution_and_snapping() {
        let mut document = Document::new();
        document.blocks = (0..3)
            .map(|i| {
                Block::TextBox(TextBox::new(
                    1440 + i * 2000,
                    1440 + i * 100,
                    1000,
                    500,
                    vec![Block::text("x")],
                ))
            })
            .collect();
        let objects = floating_objects(&document);
        bring_to_front(&mut document, objects[0]);
        send_to_back(&mut document, objects[2]);
        let z: Vec<i64> = objects
            .iter()
            .map(|o| z_of(&document, *o).unwrap())
            .collect();
        assert!(z[0] > z[1] && z[1] > z[2] && z[2] >= 1, "{z:?}");
        align(&mut document, &objects, Edge::Top);
        assert!(objects
            .iter()
            .all(|o| page_rect(&document, *o).unwrap().y == 1440));
        move_to(&mut document, objects[1], 2000, 1440);
        distribute(&mut document, &objects, true);
        let xs: Vec<Twips> = objects
            .iter()
            .map(|o| page_rect(&document, *o).unwrap().x)
            .collect();
        assert_eq!(xs[1] - xs[0], xs[2] - xs[1]);
        let snapped = snap(
            &document,
            0,
            Some(objects[0]),
            Rect {
                x: 1460,
                y: 3000,
                width: 1000,
                height: 500,
            },
            40,
        );
        assert_eq!(snapped.rect.x, 1440);
        assert_eq!(snapped.guides[0].kind, "margin");
    }
}
