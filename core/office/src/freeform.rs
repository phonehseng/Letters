//! freeform layout: canvas pages (everything positioned, no text flow),
//! pulling a table apart into text boxes at the same places and snapping
//! them back, and turning a section's columns into a chain of linked text
//! boxes and back. pure functions on the document.
//!
//! positions here are page coordinates: twips from the page's top-left
//! corner, as in `floating`.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::model::*;

/// one object on a canvas page.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CanvasObject {
    TextBox(TextBox),
    Shape(Shape),
    Group(Group),
    /// a floating picture (its placement must be floating)
    Picture(Image),
    /// a floating table (its `position` must be set)
    Table(Table),
}

/// a page of positioned objects, ready to be added to a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPage {
    pub section: Section,
    pub blocks: Vec<Block>,
}

/// the stacking convention of a canvas page: objects are given back to
/// front and numbered from here up, one apart. word compares stacking
/// numbers within a page only, so every page can start again.
pub const CANVAS_FIRST_Z: i64 = 1;

/// the minimal paragraph a canvas page needs (word wants one to anchor
/// drawings to): no spacing and a one-point line, so it never pushes
/// anything onto a second page.
fn anchor_paragraph(pictures: Vec<Image>) -> Paragraph {
    let mut paragraph = Paragraph::default();
    paragraph.props.spacing = Spacing {
        before: Some(0),
        after: Some(0),
        line: Some(LineSpacing {
            rule: LineRule::Exact,
            value: 20,
        }),
    };
    paragraph.content = pictures.into_iter().map(Inline::Image).collect();
    paragraph
}

/// a canvas page of `page`'s size with `objects` on it, back to front.
/// objects keep the positions they have; give them positions relative to
/// the page. drawings and tables come before the anchor paragraph, which
/// holds the pictures, as both readers lay out a page like it.
pub fn canvas_page_from_objects(page: PageSetup, objects: Vec<CanvasObject>) -> CanvasPage {
    let mut blocks = Vec::new();
    let mut pictures = Vec::new();
    for (index, object) in objects.into_iter().enumerate() {
        let z = CANVAS_FIRST_Z + index as i64;
        let next = || {
            Some(ObjectAnchor {
                with: AnchorWith::Next,
                inline_index: Some(0),
                ..Default::default()
            })
        };
        match object {
            CanvasObject::TextBox(mut text_box) => {
                text_box.z_order = Some(z);
                text_box.anchor = next();
                blocks.push(Block::TextBox(text_box));
            }
            CanvasObject::Shape(mut shape) => {
                shape.z_order = Some(z);
                shape.anchor = next();
                blocks.push(Block::Shape(shape));
            }
            CanvasObject::Group(mut group) => {
                group.z_order = Some(z);
                group.anchor = next();
                blocks.push(Block::Group(group));
            }
            CanvasObject::Picture(mut image) => {
                image.z_order = Some(z);
                pictures.push(image);
            }
            CanvasObject::Table(mut table) => {
                if let Some(position) = table.position.as_mut() {
                    position.anchor = next();
                }
                blocks.push(Block::Table(table));
            }
        }
    }
    blocks.push(Block::Paragraph(anchor_paragraph(pictures)));
    let section = Section {
        page,
        canvas: true,
        ..Section::default()
    };
    CanvasPage { section, blocks }
}

/// adds a canvas page at the end of a document. a document that is still
/// the empty one `Document::new` makes is replaced by the page.
pub fn append_canvas_page(document: &mut Document, page: CanvasPage) {
    document.normalize_sections();
    let untouched =
        document.blocks.len() == 1 && document.blocks[0] == Block::Paragraph(Paragraph::default());
    if document.blocks.is_empty() || untouched {
        document.blocks = page.blocks;
        document.sections = vec![page.section];
        return;
    }
    document.blocks.push(Block::SectionBreak);
    document.blocks.extend(page.blocks);
    document.sections.push(page.section);
}

/// true for a paragraph with nothing to show but floating pictures (and
/// bookmarks): what anchors the objects of a canvas page.
fn is_anchor_only(block: &Block) -> bool {
    let Some(paragraph) = block.paragraph() else {
        return false;
    };
    paragraph.content.iter().all(|inline| match inline {
        Inline::Image(image) => image.placement != Placement::Inline,
        Inline::BookmarkStart { .. } | Inline::BookmarkEnd { .. } => true,
        _ => false,
    })
}

/// marks every section that holds positioned objects and nothing else as
/// a canvas page, so a canvas page written to a file reads back as one.
/// the readers call this.
pub fn mark_canvas_sections(document: &mut Document) {
    let mut has_object = false;
    let mut only_objects = true;
    let mut found = Vec::new();
    for block in &document.blocks {
        if matches!(block, Block::SectionBreak) {
            found.push(has_object && only_objects);
            has_object = false;
            only_objects = true;
            continue;
        }
        let pictures = block.paragraph().is_some_and(|p| {
            p.content
                .iter()
                .any(|i| matches!(i, Inline::Image(image) if image.placement != Placement::Inline))
        });
        has_object |= block.is_positioned() || pictures;
        only_objects &= block.is_positioned() || is_anchor_only(block);
    }
    found.push(has_object && only_objects);
    for (slot, canvas) in document.sections.iter_mut().zip(found) {
        slot.canvas = canvas;
    }
}

// tables pulled apart ---------------------------------------------------------

/// where a flowing table sits, which only a layout knows: its top-left
/// corner in page coordinates and each row's height.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableGeometry {
    pub x: Twips,
    pub y: Twips,
    #[serde(default)]
    pub row_heights: Vec<Twips>,
}

/// word's cell margins when nothing sets them: 0.075 inch at the sides.
const WORD_CELL_SIDE_MARGIN: Twips = 108;

/// a floating table's top-left corner on the page, when its position is
/// measured from the page or the margins.
fn table_corner(
    document: &Document,
    block: usize,
    position: &TablePosition,
) -> Option<(Twips, Twips)> {
    if position.horizontal.align.is_some() || position.vertical.align.is_some() {
        return None;
    }
    let margins = document.section_of(block).page.margins;
    let x = match position.horizontal.relative_to.as_str() {
        "page" => position.horizontal.offset,
        "margin" | "column" => margins.left + position.horizontal.offset,
        _ => return None,
    };
    let y = match position.vertical.relative_to.as_str() {
        "page" => position.vertical.offset,
        "margin" => margins.top + position.vertical.offset,
        _ => return None,
    };
    Some((x, y))
}

/// pulls a table apart: each cell becomes a text box at the cell's place,
/// with its text, fill, margins and alignment, remembering the cell so
/// `text_boxes_to_table` puts the table back exactly. `geometry` is needed
/// for a table in the flow and for rows without a fixed height. returns
/// the boxes' block range.
pub fn table_to_text_boxes(
    document: &mut Document,
    block: usize,
    geometry: Option<&TableGeometry>,
) -> Option<Range<usize>> {
    let Some(Block::Table(table)) = document.blocks.get(block) else {
        return None;
    };
    let table = table.clone();
    let corner = match &table.position {
        Some(position) => table_corner(document, block, position),
        None => None,
    }
    .or_else(|| geometry.map(|g| (g.x, g.y)))?;
    let layout = crate::tables::grid(&table);
    let columns = layout.first().map(Vec::len).unwrap_or(0);
    if table.column_widths.len() < columns {
        return None;
    }
    let mut row_tops = vec![corner.1];
    for (index, row) in table.rows.iter().enumerate() {
        let height = row
            .height
            .filter(|_| row.height_rule == Some(HeightRule::Exact))
            .or_else(|| geometry.and_then(|g| g.row_heights.get(index).copied()))
            .or(row.height)?;
        row_tops.push(row_tops[index] + height);
    }
    let mut column_lefts = vec![corner.0];
    for width in &table.column_widths[..columns] {
        column_lefts.push(column_lefts.last().copied().unwrap_or(0) + width);
    }
    let wrap = if table.position.is_some() {
        Wrap::Square
    } else {
        Wrap::TopAndBottom
    };
    let mut boxes = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        for (slot_column, slot) in layout[row_index].iter().enumerate() {
            let crate::tables::Slot::Cell { cell, first: true } = *slot else {
                continue;
            };
            let source = &row.cells[cell];
            let span_columns = source.col_span.max(1) as usize;
            let span_rows = source.row_span.max(1) as usize;
            let right = column_lefts.get(slot_column + span_columns).copied()?;
            let bottom = row_tops.get(row_index + span_rows).copied()?;
            let mut text_box = cell_box(
                &table,
                source,
                (
                    column_lefts[slot_column],
                    row_tops[row_index],
                    right,
                    bottom,
                ),
                wrap,
            );
            text_box.name = Some(format!("Cell {} {}", row_index + 1, slot_column + 1));
            let mut kept_cell = source.clone();
            kept_cell.blocks = Vec::new();
            let mut kept_row = (cell == 0).then(|| row.clone());
            if let Some(kept_row) = kept_row.as_mut() {
                kept_row.cells = Vec::new();
            }
            let mut kept_table = boxes.is_empty().then(|| table.clone());
            if let Some(kept_table) = kept_table.as_mut() {
                kept_table.rows = Vec::new();
            }
            text_box.table_cell = Some(Box::new(TableCellOrigin {
                cell: kept_cell,
                row: kept_row,
                table: kept_table,
            }));
            boxes.push(Block::TextBox(text_box));
        }
    }
    let first_z = crate::floating::next_z(document);
    for (offset, object) in boxes.iter_mut().enumerate() {
        if let Block::TextBox(text_box) = object {
            text_box.z_order = Some(first_z + offset as i64);
        }
    }
    let count = boxes.len();
    document.blocks.splice(block..=block, boxes);
    Some(block..block + count)
}

/// one cell's text box over `(left, top, right, bottom)`.
fn cell_box(
    table: &Table,
    cell: &TableCell,
    (left, top, right, bottom): (Twips, Twips, Twips, Twips),
    wrap: Wrap,
) -> TextBox {
    let defaults = table.cell_margins.unwrap_or_default();
    let own = cell.margins.unwrap_or_default();
    let margin =
        |own: Option<Twips>, table: Option<Twips>, word: Twips| own.or(table).unwrap_or(word);
    let mut text_box = TextBox::new(left, top, right - left, bottom - top, cell.blocks.clone());
    text_box.wrap = wrap;
    text_box.margins = BoxMargins {
        left: margin(own.left, defaults.left, WORD_CELL_SIDE_MARGIN),
        top: margin(own.top, defaults.top, 0),
        right: margin(own.right, defaults.right, WORD_CELL_SIDE_MARGIN),
        bottom: margin(own.bottom, defaults.bottom, 0),
    };
    let borders = cell.borders.as_ref().or(table.borders.as_ref());
    text_box.border = borders.and_then(|b| b.top.clone().or_else(|| b.left.clone()));
    text_box.fill = cell.shading.clone();
    text_box.vertical_align = cell.vertical_align;
    text_box
}

/// a text box's box in page coordinates, when it is placed on the page.
fn box_rect(document: &Document, block: usize) -> Option<Edges> {
    let rect = crate::floating::page_rect(document, crate::floating::ObjectRef::TextBox { block })?;
    Some((rect.x, rect.y, rect.x + rect.width, rect.y + rect.height))
}

/// snaps text boxes back into one table when their edges form a grid:
/// every box spans whole grid cells and together they cover the grid's
/// rectangle exactly once. cells remembered by `table_to_text_boxes` get
/// their own formatting back. returns the table's block index.
pub fn text_boxes_to_table(document: &mut Document, blocks: &[usize]) -> Option<usize> {
    let mut placed = Vec::new();
    for &block in blocks {
        let Some(Block::TextBox(text_box)) = document.blocks.get(block) else {
            return None;
        };
        placed.push((block, box_rect(document, block)?, text_box.clone()));
    }
    let edges = |pick: &dyn Fn(&Edges) -> [Twips; 2]| {
        let mut values: Vec<Twips> = placed.iter().flat_map(|(_, r, _)| pick(r)).collect();
        values.sort_unstable();
        values.dedup();
        values
    };
    let xs = edges(&|r| [r.0, r.2]);
    let ys = edges(&|r| [r.1, r.3]);
    let (columns, rows) = (xs.len().checked_sub(1)?, ys.len().checked_sub(1)?);
    let mut owner: Vec<Vec<Option<usize>>> = vec![vec![None; columns]; rows];
    for (index, (_, rect, _)) in placed.iter().enumerate() {
        let (c0, c1) = (
            xs.binary_search(&rect.0).ok()?,
            xs.binary_search(&rect.2).ok()?,
        );
        let (r0, r1) = (
            ys.binary_search(&rect.1).ok()?,
            ys.binary_search(&rect.3).ok()?,
        );
        for row in owner.iter_mut().take(r1).skip(r0) {
            for slot in row.iter_mut().take(c1).skip(c0) {
                if slot.replace(index).is_some() {
                    return None;
                }
            }
        }
    }
    if owner.iter().flatten().any(Option::is_none) {
        return None;
    }
    let table = build_table(&placed, &owner, &xs, &ys);
    let at = *blocks.iter().min()?;
    let mut sorted = blocks.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    for block in sorted {
        document.blocks.remove(block);
    }
    document.blocks.insert(at, Block::Table(table));
    Some(at)
}

/// a box's left, top, right and bottom edges in page coordinates.
type Edges = (Twips, Twips, Twips, Twips);

type PlacedBox = (usize, Edges, TextBox);

fn build_table(
    placed: &[PlacedBox],
    owner: &[Vec<Option<usize>>],
    xs: &[Twips],
    ys: &[Twips],
) -> Table {
    let origin = placed
        .iter()
        .find_map(|(_, _, b)| b.table_cell.as_ref().and_then(|o| o.table.clone()));
    let mut table = origin.unwrap_or_else(|| Table {
        fixed_layout: Some(true),
        ..Table::default()
    });
    table.column_widths = xs.windows(2).map(|w| w[1] - w[0]).collect();
    table.width = Some(xs[xs.len() - 1] - xs[0]);
    let flowing = placed.iter().any(|(_, _, b)| {
        b.table_cell
            .as_ref()
            .is_some_and(|o| o.table.as_ref().is_some_and(|t| t.position.is_none()))
    });
    if !flowing {
        let mut position = table.position.clone().unwrap_or(TablePosition {
            horizontal: FloatPosition::new("page", 0),
            vertical: FloatPosition::new("page", 0),
            distance: Distances::default(),
            overlap: true,
            anchor: None,
        });
        position.horizontal = FloatPosition::new("page", xs[0]);
        position.vertical = FloatPosition::new("page", ys[0]);
        table.position = Some(position);
    }
    table.rows = Vec::new();
    for row_index in 0..owner.len() {
        let kept_row = owner[row_index]
            .iter()
            .flatten()
            .find_map(|&i| placed[i].2.table_cell.as_ref().and_then(|o| o.row.clone()));
        let mut row = kept_row.unwrap_or_default();
        row.height = Some(ys[row_index + 1] - ys[row_index]);
        row.height_rule = Some(HeightRule::Exact);
        row.cells = Vec::new();
        for column in 0..owner[row_index].len() {
            let Some(index) = owner[row_index][column] else {
                continue;
            };
            let starts_here = (row_index == 0 || owner[row_index - 1][column] != Some(index))
                && (column == 0 || owner[row_index][column - 1] != Some(index));
            if starts_here {
                row.cells.push(box_cell(&placed[index], xs, ys));
            }
        }
        table.rows.push(row);
    }
    table
}

/// the cell a box becomes: its remembered cell, or one made from the box.
fn box_cell((_, rect, text_box): &PlacedBox, xs: &[Twips], ys: &[Twips]) -> TableCell {
    let span = |edges: &[Twips], from: Twips, to: Twips| {
        let start = edges.binary_search(&from).unwrap_or(0);
        let end = edges.binary_search(&to).unwrap_or(start + 1);
        (end - start).max(1) as u32
    };
    let mut cell = match &text_box.table_cell {
        Some(origin) => origin.cell.clone(),
        None => {
            let border = text_box.border.clone();
            let sides = border.map(|b| CellBorders {
                top: Some(b.clone()),
                left: Some(b.clone()),
                bottom: Some(b.clone()),
                right: Some(b),
                inside_horizontal: None,
                inside_vertical: None,
            });
            let m = text_box.margins;
            TableCell {
                borders: sides,
                shading: text_box.fill.clone(),
                vertical_align: text_box.vertical_align,
                margins: Some(CellMargins {
                    top: Some(m.top),
                    left: Some(m.left),
                    bottom: Some(m.bottom),
                    right: Some(m.right),
                }),
                ..TableCell::default()
            }
        }
    };
    cell.col_span = span(xs, rect.0, rect.2);
    cell.row_span = span(ys, rect.1, rect.3);
    cell.width = Some(rect.2 - rect.0);
    cell.blocks = text_box.blocks.clone();
    if cell.blocks.is_empty() {
        cell.blocks.push(Block::Paragraph(Paragraph::default()));
    }
    cell
}

// columns and linked boxes ------------------------------------------------------

/// the block range of section `section` (without its closing break).
fn section_range(document: &Document, section: usize) -> Option<Range<usize>> {
    let mut start = 0;
    let mut current = 0;
    for (index, block) in document.blocks.iter().enumerate() {
        if matches!(block, Block::SectionBreak) {
            if current == section {
                return Some(start..index);
            }
            current += 1;
            start = index + 1;
        }
    }
    (current == section).then_some(start..document.blocks.len())
}

/// turns a section's columns into a chain of linked text boxes, one per
/// column at the column's place on its first page, holding the section's
/// text as one story. the section then has one column. returns the
/// boxes' block range. text longer than one page of columns keeps
/// flowing in the last box's overflow, which the applications hide.
pub fn columns_to_linked_boxes(document: &mut Document, section: usize) -> Option<Range<usize>> {
    document.normalize_sections();
    let range = section_range(document, section)?;
    let setup = document.sections.get(section)?.clone();
    let layout = setup.column_layout();
    if layout.len() < 2 {
        return None;
    }
    let story: Vec<Block> = document.blocks[range.clone()].to_vec();
    if story.iter().any(Block::is_positioned) {
        return None;
    }
    let chain = format!("columns-{section}");
    let margins = setup.page.margins.clone();
    let mut left = margins.left + margins.gutter;
    let mut boxes = Vec::new();
    for (index, column) in layout.iter().enumerate() {
        let blocks = if index == 0 {
            story.clone()
        } else {
            Vec::new()
        };
        let mut text_box = TextBox::new(
            left,
            margins.top,
            column.width,
            setup.page.text_height(),
            blocks,
        );
        text_box.name = Some(format!("{chain} {index}"));
        text_box.border = None;
        text_box.fill = None;
        text_box.margins = BoxMargins {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        text_box.wrap = Wrap::InFrontOfText;
        text_box.chain = Some(TextChain {
            id: chain.clone(),
            sequence: index as u32,
        });
        text_box.anchor = Some(ObjectAnchor::with(if index == 0 {
            AnchorWith::Own
        } else {
            AnchorWith::Previous
        }));
        boxes.push(Block::TextBox(text_box));
        left += column.width + column.space;
    }
    let count = boxes.len();
    document.blocks.splice(range.clone(), boxes);
    let target = &mut document.sections[section];
    target.columns.count = 1;
    target.column_widths.clear();
    Some(range.start..range.start + count)
}

/// turns a chain of linked boxes standing side by side (same top, same
/// height, left to right without overlapping) back into columns of the
/// section they are in, with their widths and gaps. returns the section.
pub fn linked_boxes_to_columns(document: &mut Document, chain: &str) -> Option<usize> {
    let mut members: Vec<(usize, u32, Edges)> = Vec::new();
    for (index, block) in document.blocks.iter().enumerate() {
        if let Block::TextBox(TextBox {
            chain: Some(link), ..
        }) = block
        {
            if link.id == chain {
                members.push((index, link.sequence, box_rect(document, index)?));
            }
        }
    }
    members.sort_by_key(|(_, sequence, _)| *sequence);
    let (first_index, _, _) = *members.first()?;
    let section = document.section_index_of(first_index);
    let lined_up = members.windows(2).all(|pair| {
        let (a, b) = (pair[0].2, pair[1].2);
        (a.1 - b.1).abs() <= 1 && ((a.3 - a.1) - (b.3 - b.1)).abs() <= 1 && b.0 >= a.2
    });
    let same_section = members
        .iter()
        .all(|(index, _, _)| document.section_index_of(*index) == section);
    if members.len() < 2 || !lined_up || !same_section {
        return None;
    }
    let widths: Vec<ColumnWidth> = members
        .iter()
        .enumerate()
        .map(|(position, (_, _, rect))| ColumnWidth {
            width: rect.2 - rect.0,
            space: members
                .get(position + 1)
                .map(|next| next.2 .0 - rect.2)
                .unwrap_or(0),
        })
        .collect();
    let Block::TextBox(head) = &document.blocks[first_index] else {
        return None;
    };
    let story = head.blocks.clone();
    for (index, _, _) in members.iter().rev() {
        document.blocks.remove(*index);
    }
    let at = members
        .iter()
        .map(|(index, _, _)| *index)
        .min()
        .unwrap_or(first_index);
    document.blocks.splice(at..at, story);
    document.normalize_sections();
    let target = &mut document.sections[section];
    let equal = widths.iter().all(|w| w.width == widths[0].width)
        && widths[..widths.len() - 1]
            .iter()
            .all(|w| w.space == widths[0].space);
    target.columns.count = widths.len() as u32;
    if equal {
        target.columns.space = widths[0].space;
        target.column_widths.clear();
    } else {
        target.column_widths = widths;
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let cell = |text: &str| TableCell::with_text(text);
        let mut merged = cell("wide");
        merged.col_span = 2;
        merged.shading = Some("DCEBFF".into());
        Table {
            column_widths: vec![1440, 2880],
            rows: vec![
                TableRow {
                    height: Some(400),
                    height_rule: Some(HeightRule::Exact),
                    cells: vec![cell("a"), cell("b")],
                    ..Default::default()
                },
                TableRow {
                    height: Some(600),
                    height_rule: Some(HeightRule::Exact),
                    cells: vec![merged],
                    ..Default::default()
                },
            ],
            position: Some(TablePosition {
                horizontal: FloatPosition::new("page", 2000),
                vertical: FloatPosition::new("page", 3000),
                distance: Distances::default(),
                overlap: true,
                anchor: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_table_pulled_apart_snaps_back_the_same() {
        let mut document = Document::new();
        document.blocks = vec![Block::Table(table()), Block::text("after")];
        let boxes = table_to_text_boxes(&mut document, 0, None).unwrap();
        assert_eq!(boxes, 0..3);
        let Block::TextBox(wide) = &document.blocks[2] else {
            panic!()
        };
        assert_eq!((wide.horizontal.offset, wide.vertical.offset), (2000, 3400));
        assert_eq!((wide.width, wide.height), (4320, 600));
        let at = text_boxes_to_table(&mut document, &[0, 1, 2]).unwrap();
        let Block::Table(back) = &document.blocks[at] else {
            panic!()
        };
        let original = table();
        assert_eq!(back.column_widths, original.column_widths);
        assert_eq!(back.position.as_ref().unwrap().horizontal.offset, 2000);
        for (a, b) in back.rows.iter().zip(&original.rows) {
            assert_eq!(a.height, b.height);
            assert_eq!(a.cells.len(), b.cells.len());
            for (x, y) in a.cells.iter().zip(&b.cells) {
                assert_eq!(
                    (x.col_span, x.row_span, &x.shading, &x.blocks),
                    (y.col_span, y.row_span, &y.shading, &y.blocks)
                );
            }
        }
    }

    #[test]
    fn boxes_that_do_not_tile_stay_boxes() {
        let mut document = Document::new();
        document.blocks = vec![
            Block::TextBox(TextBox::new(0, 0, 100, 100, vec![])),
            Block::TextBox(TextBox::new(50, 50, 100, 100, vec![])),
        ];
        assert!(text_boxes_to_table(&mut document, &[0, 1]).is_none());
    }

    #[test]
    fn columns_become_linked_boxes_and_back() {
        let mut document = Document::new();
        document.blocks = vec![Block::text("one"), Block::text("two")];
        document.sections[0].column_widths = vec![
            ColumnWidth {
                width: 3000,
                space: 360,
            },
            ColumnWidth {
                width: 6000,
                space: 0,
            },
        ];
        let boxes = columns_to_linked_boxes(&mut document, 0).unwrap();
        assert_eq!(boxes.len(), 2);
        let Block::TextBox(second) = &document.blocks[1] else {
            panic!()
        };
        assert_eq!(second.horizontal.offset, 1440 + 3000 + 360);
        assert!(second.blocks.is_empty());
        assert_eq!(document.sections[0].columns.count, 1);
        assert_eq!(linked_boxes_to_columns(&mut document, "columns-0"), Some(0));
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(
            document.sections[0].column_widths[0],
            ColumnWidth {
                width: 3000,
                space: 360
            }
        );
    }

    #[test]
    fn a_canvas_page_is_positioned_objects_and_one_anchor_paragraph() {
        let mut document = Document::new();
        let page = canvas_page_from_objects(
            PageSetup::default(),
            vec![
                CanvasObject::Shape(Shape::preset("rect", 0, 0, 12240, 2000, "1E3C6E")),
                CanvasObject::TextBox(TextBox::new(
                    720,
                    720,
                    5000,
                    800,
                    vec![Block::text("Title")],
                )),
            ],
        );
        append_canvas_page(&mut document, page);
        assert_eq!(document.blocks.len(), 3);
        assert!(document.sections[0].canvas);
        let Block::TextBox(title) = &document.blocks[1] else {
            panic!()
        };
        assert_eq!(title.z_order, Some(CANVAS_FIRST_Z + 1));
        let mut copy = document.clone();
        copy.sections[0].canvas = false;
        mark_canvas_sections(&mut copy);
        assert!(copy.sections[0].canvas);
    }
}
