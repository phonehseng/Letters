//! tables: column widths, cells laid out inside their margins, row
//! heights, borders, and rows flowing across pages.
//!
//! column widths: a table whose grid gives widths keeps them (word's grid
//! is what it last computed). a table with no widths is fitted to its
//! content the way word's "autofit to contents" does it: each column's
//! narrowest possible width (its longest word) and its natural width (its
//! text on one line) are measured, and the room is shared between them in
//! proportion to how much more each would like (the classic automatic
//! table algorithm; word's own is not published and was only approximated
//! against the corpus).
//!
//! rows: a row is as tall as its tallest cell (content plus the top and
//! bottom cell margins), at least its stated height. a row that does not
//! fit at the bottom of a page goes to the next page; one allowed to split
//! is split between lines, and header rows repeat at the top of every
//! page the table continues on.

use letters_office::model::{Alignment, Block, Border, Table, TableCell, VerticalAlign};

use crate::pages::emit::{emit_line, LinePlacement};
use crate::pages::{
    Context, EmittedBlocks, Flow, PreparedParagraph, PreviousParagraph, FIT_TOLERANCE,
};
use crate::result::{Rect, RectKind, Story};
use crate::style::preserved;
use crate::text::FieldValues;
use crate::units::twips_to_points;

/// libreoffice's cell margins when no table style sets any (0.19 cm).
/// word's "table normal" style sets 0.075 inch (108 twips) in every file
/// word writes; a file with no table style at all gets none in word
/// (tables_fixed in the corpus: with 5.4 pt margins 59 % of its lines
/// broke like word's, with 0 pt 92 %).
const LIBREOFFICE_SIDE_MARGIN: f64 = 5.4;
/// tables nested deeper than this are not laid out
const MAX_DEPTH: usize = 8;

#[derive(Clone, Debug)]
pub enum CellBlock {
    Paragraph {
        prepared: Box<PreparedParagraph>,
        /// space above it (after the block before plus its own before)
        before: f64,
    },
    Table(Box<PreparedTable>),
}

impl CellBlock {
    fn height(&self) -> f64 {
        match self {
            CellBlock::Paragraph { prepared, before } => before + prepared.layout.height(),
            CellBlock::Table(table) => table.rows.iter().map(|row| row.height).sum(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreparedCell {
    pub column: usize,
    pub span: usize,
    pub row_span: u32,
    /// x from the table's left edge
    pub x: f64,
    pub width: f64,
    pub blocks: Vec<CellBlock>,
    pub content_height: f64,
    pub borders: [Option<(f64, [u8; 3], bool)>; 4],
    pub shading: Option<[u8; 3]>,
    pub vertical_align: VerticalAlign,
}

#[derive(Clone, Debug)]
pub struct PreparedRow {
    pub cells: Vec<PreparedCell>,
    pub height: f64,
    pub header: bool,
    pub cant_split: bool,
    /// the widest top border of the row's cells, above the content
    pub top_border: f64,
}

/// the widest border on one edge (0 top, 1 left, 2 bottom, 3 right).
fn edge_width(cells: &[PreparedCell], edge: usize) -> f64 {
    cells
        .iter()
        .filter_map(|cell| cell.borders[edge].map(|(width, _, _)| width))
        .fold(0.0, f64::max)
}

#[derive(Clone, Debug)]
pub struct PreparedTable {
    pub story: Story,
    pub path: Vec<u32>,
    pub columns: Vec<f64>,
    /// the table's left edge from the column's left
    pub left: f64,
    pub rows: Vec<PreparedRow>,
    pub margins: CellMargins,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMargins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// the cell margins: the table style's (and the styles it is based on),
/// then the table's own, both kept as preserved xml.
fn cell_margins(context: &Context, table: &Table) -> CellMargins {
    let side = match context.flavor {
        crate::Flavor::Word => 0.0,
        crate::Flavor::LibreOffice => LIBREOFFICE_SIDE_MARGIN,
    };
    let mut margins = CellMargins {
        left: side,
        right: side,
        top: 0.0,
        bottom: 0.0,
    };
    let document = context.document;
    let mut sources: Vec<&String> = Vec::new();
    if let Some(id) = crate::style::table_style_id(document, table.style_id.as_deref()) {
        for style in letters_office::styles::style_chain(document, &id)
            .iter()
            .rev()
        {
            sources.extend(style.preserved.children.iter());
        }
    }
    sources.extend(table.preserved.children.iter());
    for xml in sources {
        if !xml.contains("tblCellMar") {
            continue;
        }
        for tag in preserved::tags(xml) {
            let Some(width) = tag.attr("w:w").and_then(|w| w.parse::<i32>().ok()) else {
                continue;
            };
            let value = twips_to_points(width);
            match tag.name.as_str() {
                "w:left" | "w:start" => margins.left = value,
                "w:right" | "w:end" => margins.right = value,
                "w:top" => margins.top = value,
                "w:bottom" => margins.bottom = value,
                _ => {}
            }
        }
    }
    margins
}

/// the table indent from tblInd, kept as preserved xml.
fn table_indent(table: &Table) -> f64 {
    preserved::find(&table.preserved.children, "w:tblInd")
        .and_then(|tag| tag.attr("w:w").and_then(|w| w.parse::<i32>().ok()))
        .map(twips_to_points)
        .unwrap_or(0.0)
}

/// gives a prepared table (and everything in it) a new block index, when
/// the incremental engine reuses it at another place.
pub fn set_first_index(table: &mut PreparedTable, index: u32) {
    if let Some(first) = table.path.first_mut() {
        *first = index;
    }
    for row in &mut table.rows {
        for cell in &mut row.cells {
            for block in &mut cell.blocks {
                match block {
                    CellBlock::Paragraph { prepared, .. } => {
                        if let Some(first) = prepared.path.first_mut() {
                            *first = index;
                        }
                    }
                    CellBlock::Table(nested) => set_first_index(nested, index),
                }
            }
        }
    }
}

/// shapes and breaks every cell of a table that may be `available` wide.
pub fn prepare_table(
    context: &mut Context,
    table: &Table,
    story: &Story,
    path: &[u32],
    available: f64,
    fields: FieldValues,
    depth: usize,
) -> PreparedTable {
    let margins = cell_margins(context, table);
    let table_style = crate::style::table_style_props(context.document, table.style_id.as_deref());
    let outer_style = context.table_style.replace(table_style);
    let grid = letters_office::tables::grid(table);
    let column_count = grid
        .first()
        .map_or(0, Vec::len)
        .max(table.column_widths.len())
        .max(1);
    let columns = column_widths(
        context,
        table,
        story,
        path,
        available,
        column_count,
        margins,
        fields,
    );
    let total: f64 = columns.iter().sum();
    // word (compat 15) puts the left border at the margin plus the table
    // indent; libreoffice draws the same for docx
    let indent = table_indent(table);
    let left = match table.alignment {
        Some(Alignment::Center) => (available - total) / 2.0,
        Some(Alignment::Right) => available - total,
        _ => indent,
    };
    let mut rows = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for (c, cell) in row.cells.iter().enumerate() {
            // covered cells of a vertical merge are left out of the model;
            // the office crate's grid says which column each cell starts in
            let column = start_column(&grid, r, c);
            let span =
                (cell.col_span.max(1) as usize).min(column_count.saturating_sub(column).max(1));
            let x: f64 = columns[..column.min(columns.len())].iter().sum();
            let width: f64 = columns[column.min(columns.len())..(column + span).min(columns.len())]
                .iter()
                .sum();
            let mut cell_path = path.to_vec();
            cell_path.extend([r as u32, c as u32]);
            let blocks = prepare_cell_blocks(
                context,
                cell,
                story,
                &cell_path,
                (width - margins.left - margins.right).max(1.0),
                fields,
                depth,
            );
            let content_height = blocks.iter().map(CellBlock::height).sum();
            cells.push(PreparedCell {
                column,
                span,
                row_span: cell.row_span.max(1),
                x,
                width,
                blocks,
                content_height,
                borders: resolve_borders(
                    table,
                    cell,
                    r,
                    column,
                    span,
                    table.rows.len(),
                    column_count,
                ),
                shading: cell
                    .shading
                    .as_deref()
                    .and_then(crate::style::parse_hex_color),
                vertical_align: cell.vertical_align.unwrap_or(VerticalAlign::Top),
            });
        }
        let content = cells
            .iter()
            .filter(|cell| cell.row_span == 1)
            .map(|cell| cell.content_height + margins.top + margins.bottom)
            .fold(0.0, f64::max);
        let stated = row.height.map(twips_to_points).unwrap_or(0.0);
        // a row reserves room for its top border above its content, and
        // the last row for its bottom border too: word's rows of a table
        // with half-point borders are each 0.5 pt taller than their text
        // (tables_fixed in the corpus)
        let top_border = edge_width(&cells, 0);
        let bottom_border = if r + 1 == table.rows.len() {
            edge_width(&cells, 2)
        } else {
            0.0
        };
        rows.push(PreparedRow {
            cells,
            height: (content + top_border + bottom_border).max(stated).max(1.0),
            header: row.header,
            cant_split: row.cant_split,
            top_border,
        });
    }
    grow_rows_for_merges(&mut rows, margins);
    context.table_style = outer_style;
    PreparedTable {
        story: story.clone(),
        path: path.to_vec(),
        columns,
        left,
        rows,
        margins,
    }
}

fn start_column(grid: &[Vec<letters_office::tables::Slot>], row: usize, cell: usize) -> usize {
    grid.get(row)
        .and_then(|slots| {
            slots
                .iter()
                .position(|slot| *slot == letters_office::tables::Slot::Cell { cell, first: true })
        })
        .unwrap_or(0)
}

/// a merged cell taller than the rows it spans makes the last one taller.
fn grow_rows_for_merges(rows: &mut [PreparedRow], margins: CellMargins) {
    for start in 0..rows.len() {
        for cell_index in 0..rows[start].cells.len() {
            let span = rows[start].cells[cell_index].row_span as usize;
            if span < 2 {
                continue;
            }
            let end = (start + span).min(rows.len());
            let needed =
                rows[start].cells[cell_index].content_height + margins.top + margins.bottom;
            let have: f64 = rows[start..end].iter().map(|row| row.height).sum();
            if needed > have {
                rows[end - 1].height += needed - have;
            }
        }
    }
}

fn prepare_cell_blocks(
    context: &mut Context,
    cell: &TableCell,
    story: &Story,
    path: &[u32],
    width: f64,
    fields: FieldValues,
    depth: usize,
) -> Vec<CellBlock> {
    let mut blocks = Vec::new();
    let mut previous_after: Option<f64> = None;
    for (index, block) in cell.blocks.iter().enumerate() {
        let mut block_path = path.to_vec();
        block_path.push(index as u32);
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                let paragraph = block.paragraph().expect("paragraph block");
                let prepared =
                    context.prepare_paragraph(paragraph, story.clone(), block_path, width, fields);
                // the first paragraph of a cell keeps its space before
                let before = previous_after.unwrap_or(0.0) + prepared.style.space_before;
                previous_after = Some(prepared.style.space_after);
                blocks.push(CellBlock::Paragraph {
                    prepared: Box::new(prepared),
                    before,
                });
            }
            Block::Table(table) if depth < MAX_DEPTH => {
                let nested =
                    prepare_table(context, table, story, &block_path, width, fields, depth + 1);
                blocks.push(CellBlock::Table(Box::new(nested)));
                previous_after = Some(0.0);
            }
            Block::Table(_) => {
                let at = crate::pages::location(story, &block_path);
                context
                    .unsupported
                    .push(crate::unsupported::Unsupported::new(
                        crate::unsupported::UnsupportedKind::DeepNesting,
                        &at,
                    ));
            }
            _ => {}
        }
    }
    // the last paragraph's space after counts inside the cell (word)
    if let Some(after) = previous_after.filter(|after| *after > 0.0) {
        blocks.push(CellBlock::Paragraph {
            prepared: Box::new(empty_spacer()),
            before: after,
        });
    }
    blocks
}

/// an invisible zero-height block that only carries spacing.
fn empty_spacer() -> PreparedParagraph {
    PreparedParagraph {
        story: Story::Body,
        path: Vec::new(),
        style: crate::style::para_style(
            &letters_office::model::Document::new(),
            &letters_office::model::Paragraph::default(),
            crate::Flavor::Word,
        ),
        text: Default::default(),
        layout: Default::default(),
        width: 0.0,
    }
}

/// column widths: the grid when it has them, else fitted to content.
#[allow(clippy::too_many_arguments)]
fn column_widths(
    context: &mut Context,
    table: &Table,
    story: &Story,
    path: &[u32],
    available: f64,
    count: usize,
    margins: CellMargins,
    fields: FieldValues,
) -> Vec<f64> {
    let grid: Vec<f64> = table
        .column_widths
        .iter()
        .map(|w| twips_to_points(*w))
        .collect();
    // word keeps the grid when the layout is fixed or every cell says how
    // wide it wants to be; otherwise it fits the columns to their content
    // again (tables_auto and tables_long in the corpus, where cells
    // without widths made word ignore the grid entirely)
    let fixed_layout = preserved::find(&table.preserved.children, "w:tblLayout")
        .is_some_and(|tag| tag.attr("w:type") == Some("fixed"));
    let every_cell_sized = table.rows.iter().all(|row| {
        row.cells
            .iter()
            .all(|cell| cell.width.is_some_and(|w| w > 0))
    });
    if grid.len() == count && grid.iter().all(|w| *w > 0.0) && (fixed_layout || every_cell_sized) {
        return grid;
    }
    // natural and narrowest widths of each column from its single cells
    let mut narrowest = vec![margins.left + margins.right; count];
    let mut natural = narrowest.clone();
    for row in &table.rows {
        let mut column = 0;
        for cell in &row.cells {
            if cell.col_span <= 1 && column < count {
                let (min, max) = content_widths(context, cell, story, path, fields);
                // a cell's own width stands in for its natural width
                let max = cell
                    .width
                    .filter(|w| *w > 0)
                    .map(twips_to_points)
                    .unwrap_or(max + margins.left + margins.right);
                narrowest[column] = narrowest[column].max(min + margins.left + margins.right);
                natural[column] = natural[column].max(max);
            }
            column += cell.col_span.max(1) as usize;
        }
    }
    let target = table
        .width
        .map(twips_to_points)
        .unwrap_or(available)
        .min(available.max(1.0));
    distribute(&narrowest, &natural, target, table.width.is_some())
}

/// shares `target` between columns from their narrowest and natural
/// widths. a table that fits naturally keeps its natural widths (it does
/// not stretch unless its width is fixed).
pub fn distribute(narrowest: &[f64], natural: &[f64], target: f64, fixed_width: bool) -> Vec<f64> {
    let min_total: f64 = narrowest.iter().sum();
    let max_total: f64 = natural.iter().sum();
    if max_total <= target {
        if !fixed_width || max_total <= 0.0 {
            return natural.to_vec();
        }
        let scale = target / max_total;
        return natural.iter().map(|w| w * scale).collect();
    }
    if min_total >= target {
        return narrowest.to_vec();
    }
    let share = (target - min_total) / (max_total - min_total);
    narrowest
        .iter()
        .zip(natural)
        .map(|(min, max)| min + (max - min) * share)
        .collect()
}

/// a cell's narrowest width (its longest unbreakable piece) and natural
/// width (its widest paragraph on one line).
fn content_widths(
    context: &mut Context,
    cell: &TableCell,
    story: &Story,
    path: &[u32],
    fields: FieldValues,
) -> (f64, f64) {
    let mut min: f64 = 0.0;
    let mut max: f64 = 0.0;
    for block in &cell.blocks {
        let Some(paragraph) = block.paragraph() else {
            continue;
        };
        let prepared =
            context.prepare_paragraph(paragraph, story.clone(), path.to_vec(), 1.0e6, fields);
        let indent =
            prepared.style.left + prepared.style.right + prepared.style.first_line.max(0.0);
        let mut word: f64 = 0.0;
        let mut line: f64 = 0.0;
        for cluster in &prepared.text.clusters {
            if cluster.break_before != crate::text::breaks::Break::None {
                min = min.max(word);
                word = 0.0;
            }
            if cluster.kind != crate::text::ClusterKind::Space {
                word += cluster.width;
            }
            line += cluster.width;
        }
        min = min.max(word) + indent;
        max = max.max(line + indent);
    }
    (min, max)
}

/// each cell edge's border: the cell's own, else the table's outer or
/// inside border for that edge. (width, colour, dotted)
fn resolve_borders(
    table: &Table,
    cell: &TableCell,
    row: usize,
    column: usize,
    span: usize,
    rows: usize,
    columns: usize,
) -> [Option<(f64, [u8; 3], bool)>; 4] {
    let table_borders = table.borders.clone().unwrap_or_default();
    let own = cell.borders.clone().unwrap_or_default();
    let pick =
        |own: &Option<Border>, outer: &Option<Border>, inside: &Option<Border>, is_outer: bool| {
            let chosen = own.as_ref().or(if is_outer {
                outer.as_ref()
            } else {
                inside.as_ref()
            });
            chosen.and_then(border_value)
        };
    let last_row = row + cell.row_span.max(1) as usize >= rows;
    [
        pick(
            &own.top,
            &table_borders.top,
            &table_borders.inside_horizontal,
            row == 0,
        ),
        pick(
            &own.left,
            &table_borders.left,
            &table_borders.inside_vertical,
            column == 0,
        ),
        pick(
            &own.bottom,
            &table_borders.bottom,
            &table_borders.inside_horizontal,
            last_row,
        ),
        pick(
            &own.right,
            &table_borders.right,
            &table_borders.inside_vertical,
            column + span >= columns,
        ),
    ]
}

fn border_value(border: &Border) -> Option<(f64, [u8; 3], bool)> {
    if matches!(border.style.as_str(), "none" | "nil" | "") {
        return None;
    }
    let color = border
        .color
        .as_deref()
        .and_then(crate::style::parse_hex_color)
        .unwrap_or([0, 0, 0]);
    let width = (f64::from(border.size.max(2)) / 8.0).max(0.25);
    let dotted = border.style.contains("dot") || border.style.contains("dash");
    Some((width, color, dotted))
}

/// where a cell's content has got to while a row is split across pages.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CellCursor {
    block: usize,
    line: usize,
}

#[allow(clippy::too_many_arguments)]
/// draws one row (or the part of it from `cursors` that fits in
/// `limit`) at `top`; returns the cursors where the next part starts, or
/// `None` when the whole row was drawn.
fn draw_row(
    context: &Context,
    table: &PreparedTable,
    row_index: usize,
    left: f64,
    top: f64,
    limit: Option<f64>,
    cursors: &[CellCursor],
    out: &mut EmittedBlocks,
) -> (f64, Option<Vec<CellCursor>>) {
    let row = &table.rows[row_index];
    let margins = table.margins;
    let mut next = Vec::new();
    let mut unfinished = false;
    let mut used: f64 = 0.0;
    for (index, cell) in row.cells.iter().enumerate() {
        let start = cursors.get(index).copied().unwrap_or_default();
        let room = limit.map(|limit| limit - margins.top - margins.bottom);
        // the content starts inside the cell's left border
        let left_border = cell.borders[1].map_or(0.0, |(width, _, _)| width);
        let content_left = left + table.left + cell.x + margins.left + left_border;
        let content_top = top + row.top_border + margins.top;
        let (height, cursor, done) = draw_cell_content(
            context,
            cell,
            start,
            content_left,
            content_top,
            room,
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
    room: Option<f64>,
    out: &mut EmittedBlocks,
    row_content_height: f64,
) -> (f64, CellCursor, bool) {
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
                while cursor.line < lines.len() {
                    let line = &lines[cursor.line];
                    if room
                        .is_some_and(|room| y + line.vertical.height - top > room + FIT_TOLERANCE)
                        && y > top
                    {
                        return (y - top, cursor, false);
                    }
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

impl Flow<'_, '_> {
    /// places a table's rows, moving or splitting rows at page ends and
    /// repeating header rows.
    pub fn place_table(&mut self, table: &PreparedTable) {
        if let Some(previous) = self.previous.take() {
            if !self.column_empty {
                self.y += previous.space_after;
            }
        }
        let headers = table.rows.iter().take_while(|row| row.header).count();
        let mut index = 0;
        let mut cursors: Vec<CellCursor> = Vec::new();
        while index < table.rows.len() {
            let row = &table.rows[index];
            let fresh = cursors.is_empty();
            let fits = self.y + row.height <= self.bottom() + FIT_TOLERANCE;
            if fresh && fits {
                self.draw_table_row(table, index, None, &[]);
                index += 1;
                continue;
            }
            let room = self.bottom() - self.y;
            let can_split = !row.cant_split && room > 6.0;
            if !self.column_empty && !can_split {
                self.continue_table_on_next_page(table, headers, index);
                continue;
            }
            match self.draw_table_row(table, index, Some(room.max(1.0)), &cursors) {
                Some(rest) => {
                    cursors = rest;
                    self.continue_table_on_next_page(table, headers, index);
                }
                None => {
                    cursors.clear();
                    index += 1;
                }
            }
        }
        self.previous = Some(PreviousParagraph {
            style_id: None,
            space_after: 0.0,
            contextual: false,
        });
    }

    fn continue_table_on_next_page(&mut self, table: &PreparedTable, headers: usize, index: usize) {
        self.next_column();
        if index >= headers {
            for header in 0..headers {
                self.draw_table_row(table, header, None, &[]);
            }
        }
    }

    fn draw_table_row(
        &mut self,
        table: &PreparedTable,
        index: usize,
        limit: Option<f64>,
        cursors: &[CellCursor],
    ) -> Option<Vec<CellCursor>> {
        let (left, _) = self.column_box();
        let mut out = EmittedBlocks::default();
        let (height, rest) = draw_row(
            self.context,
            table,
            index,
            left,
            self.y,
            limit,
            cursors,
            &mut out,
        );
        out.apply_to(&mut self.page().page);
        self.y += height;
        self.column_empty = false;
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_widths_when_they_fit() {
        assert_eq!(
            distribute(&[10.0, 20.0], &[50.0, 100.0], 468.0, false),
            vec![50.0, 100.0]
        );
    }

    #[test]
    fn room_is_shared_by_what_each_column_wants() {
        // 100 to share beyond the minimums; column b wants 3 times more
        let widths = distribute(&[50.0, 50.0], &[100.0, 200.0], 200.0, false);
        assert!(
            (widths[0] - 75.0).abs() < 1e-9 && (widths[1] - 125.0).abs() < 1e-9,
            "{widths:?}"
        );
    }

    #[test]
    fn minimums_win_when_nothing_fits() {
        assert_eq!(
            distribute(&[300.0, 300.0], &[400.0, 400.0], 468.0, false),
            vec![300.0, 300.0]
        );
    }

    #[test]
    fn fixed_width_tables_stretch() {
        assert_eq!(
            distribute(&[10.0, 10.0], &[50.0, 50.0], 200.0, true),
            vec![100.0, 100.0]
        );
    }
}
