//! drawing a prepared table's rows: each cell's lines inside its insets,
//! shading and borders centred on the cell edges, and rows cut where a
//! page ends (CellCursor says where a split row goes on).

use letters_office::model::VerticalAlign;

use super::{CellBlock, PreparedCell, PreparedTable};
use crate::pages::emit::{emit_line, LinePlacement};
use crate::pages::PreparedParagraph;
use crate::pages::{Context, EmittedBlocks, FIT_TOLERANCE};
use crate::result::{Rect, RectKind};
/// the room a row split at a page end has.
#[derive(Clone, Copy, Debug)]
pub(super) struct RowLimit {
    pub height: f64,
    /// widow and orphan control may send all of a cell's lines on: true
    /// unless the row starts the column (it must place something there)
    pub orphans_may_move: bool,
}

/// where a cell's content has got to while a row is split across pages.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct CellCursor {
    block: usize,
    line: usize,
}

#[allow(clippy::too_many_arguments)]
/// draws one row (or the part of it from `cursors` that fits in
/// `limit`) at `top`; returns the cursors where the next part starts, or
/// `None` when the whole row was drawn.
pub(super) fn draw_row(
    context: &Context,
    table: &PreparedTable,
    row_index: usize,
    left: f64,
    top: f64,
    limit: Option<RowLimit>,
    cursors: &[CellCursor],
    out: &mut EmittedBlocks,
) -> (f64, Option<Vec<CellCursor>>) {
    let row = &table.rows[row_index];
    let margins = table.margins;
    let mut next = Vec::new();
    let mut unfinished = false;
    let mut used: f64 = 0.0;
    let orphans_may_move = limit.is_some_and(|limit| limit.orphans_may_move);
    let limit = limit.map(|limit| limit.height);
    for (index, cell) in row.cells.iter().enumerate() {
        let start = cursors.get(index).copied().unwrap_or_default();
        let room = limit.map(|limit| limit - margins.top - margins.bottom);
        let content_left = left + table.left + cell.x + cell.inset_left;
        let content_top = top + row.top_border + margins.top;
        let (height, cursor, done) = draw_cell_content(
            context,
            cell,
            start,
            content_left,
            content_top,
            CellRoom {
                height: room,
                orphans_may_move,
            },
            out,
            row.height - margins.top - margins.bottom - row.top_border,
        );
        let height = height + row.top_border;
        used = used.max(height + margins.top + margins.bottom);
        unfinished |= !done;
        next.push(cursor);
    }
    let height = if unfinished {
        limit.unwrap_or(used).max(used)
    } else {
        row_height_here(table, row_index, used, cursors)
    };
    for cell in &row.cells {
        let spanned = if cell.row_span > 1 && !unfinished {
            table.rows[row_index..(row_index + cell.row_span as usize).min(table.rows.len())]
                .iter()
                .map(|r| r.height)
                .sum()
        } else {
            height
        };
        draw_cell_frame(cell, left + table.left, top, spanned, out);
    }
    (height, unfinished.then_some(next))
}

/// a fresh row is its prepared height; a continued part only what is left.
fn row_height_here(
    table: &PreparedTable,
    row_index: usize,
    used: f64,
    cursors: &[CellCursor],
) -> f64 {
    if cursors.iter().all(|c| *c == CellCursor::default()) {
        table.rows[row_index].height
    } else {
        used
    }
}

/// draws a cell's lines from `start`; returns the height used, where it
/// stopped, and whether it finished.
#[allow(clippy::too_many_arguments)]
fn draw_cell_content(
    context: &Context,
    cell: &PreparedCell,
    start: CellCursor,
    left: f64,
    top: f64,
    cell_room: CellRoom,
    out: &mut EmittedBlocks,
    row_content_height: f64,
) -> (f64, CellCursor, bool) {
    let room = cell_room.height;
    // vertical alignment moves the whole content down within the row
    let fresh = start == CellCursor::default();
    let offset = if fresh && room.is_none() {
        match cell.vertical_align {
            VerticalAlign::Center => ((row_content_height - cell.content_height) / 2.0).max(0.0),
            VerticalAlign::Bottom => (row_content_height - cell.content_height).max(0.0),
            VerticalAlign::Top => 0.0,
        }
    } else {
        0.0
    };
    let mut y = top + offset;
    let mut cursor = start;
    while cursor.block < cell.blocks.len() {
        match &cell.blocks[cursor.block] {
            CellBlock::Paragraph { prepared, before } => {
                let lines = &prepared.layout.lines;
                if cursor.line == 0 {
                    if room.is_some_and(|room| y + before - top > room + FIT_TOLERANCE) {
                        return (y - top, cursor, false);
                    }
                    y += before;
                }
                let last = match room {
                    Some(room) => lines_to_draw(prepared, cursor.line, y - top, room, cell_room),
                    None => lines.len(),
                };
                while cursor.line < last {
                    let line = &lines[cursor.line];
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
                    cursor.line += 1;
                }
                if cursor.line < lines.len() {
                    return (y - top, cursor, false);
                }
            }
            CellBlock::Table(nested) => {
                let height: f64 = nested.rows.iter().map(|r| r.height).sum();
                if room.is_some_and(|room| y + height - top > room + FIT_TOLERANCE) && y > top {
                    return (y - top, cursor, false);
                }
                y = draw_whole_table(context, nested, left - nested.margins.left.min(0.0), y, out);
            }
        }
        cursor = CellCursor {
            block: cursor.block + 1,
            line: 0,
        };
    }
    (y - top, cursor, true)
}

/// the room of one cell while its row is split at a page end.
#[derive(Clone, Copy, Debug)]
struct CellRoom {
    height: Option<f64>,
    orphans_may_move: bool,
}

/// how far through a cell paragraph's lines (from `first`) a split row
/// draws, `used` points into the cell part of `room`. widow and orphan
/// control hold inside cells as anywhere: word moved whole rows to the
/// next page rather than leave one line of a cell paragraph behind
/// (w_tables_aptos: rows of two-line cells never split one and one).
fn lines_to_draw(
    prepared: &PreparedParagraph,
    first: usize,
    used: f64,
    room: f64,
    cell_room: CellRoom,
) -> usize {
    let lines = &prepared.layout.lines;
    let mut y = used;
    let mut fit = 0;
    for line in &lines[first..] {
        let at_top = y <= 0.0 && fit == 0;
        if y + line.vertical.height > room + FIT_TOLERANCE && !at_top {
            break;
        }
        y += line.vertical.height;
        fit += 1;
    }
    let rest = lines.len() - first;
    if fit < rest && prepared.style.widow_control && cell_room.orphans_may_move {
        fit = crate::pages::widow_orphan(fit, first, lines.len());
    }
    first + fit
}

/// shading behind the cell and its four borders, centred on its edges.
fn draw_cell_frame(
    cell: &PreparedCell,
    table_left: f64,
    top: f64,
    height: f64,
    out: &mut EmittedBlocks,
) {
    let x = table_left + cell.x;
    if let Some(color) = cell.shading {
        out.rects.insert(
            0,
            Rect {
                kind: RectKind::Shading,
                x,
                y: top,
                width: cell.width,
                height,
                color,
                dotted: false,
            },
        );
    }
    let edges = [
        (x, top, cell.width, true),
        (x, top, height, false),
        (x, top + height, cell.width, true),
        (x + cell.width, top, height, false),
    ];
    for (border, (ex, ey, length, horizontal)) in cell.borders.iter().zip(edges) {
        let Some((width, color, dotted)) = border else {
            continue;
        };
        let rect = if horizontal {
            Rect {
                kind: RectKind::Border,
                x: ex - width / 2.0,
                y: ey - width / 2.0,
                width: length + width,
                height: *width,
                color: *color,
                dotted: *dotted,
            }
        } else {
            Rect {
                kind: RectKind::Border,
                x: ex - width / 2.0,
                y: ey - width / 2.0,
                width: *width,
                height: length + width,
                color: *color,
                dotted: *dotted,
            }
        };
        out.rects.push(rect);
    }
}

/// draws a whole table without page breaks (inside headers, cells and
/// text boxes); returns the y below it.
pub fn draw_whole_table(
    context: &Context,
    table: &PreparedTable,
    left: f64,
    top: f64,
    out: &mut EmittedBlocks,
) -> f64 {
    let mut y = top;
    for index in 0..table.rows.len() {
        let (height, _) = draw_row(context, table, index, left, y, None, &[], out);
        y += height;
    }
    y
}
