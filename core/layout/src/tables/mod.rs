//! tables made ready for placing: cell margins, column widths (widths.rs),
//! every cell's paragraphs broken at its width, row heights and borders.
//! draw.rs draws rows and place.rs flows them across pages.
//!
//! rows: a row is as tall as its tallest cell (content plus the top and
//! bottom cell margins), at least its stated height.

mod draw;
mod place;
mod widths;

pub use draw::draw_whole_table;
pub use widths::{column_content_widths, distribute};

use letters_office::model::{
    Alignment, Block, Border, CellBorders, Document, Table, TableCell, VerticalAlign,
};

use crate::pages::{Context, PreparedParagraph};
use crate::result::Story;
use crate::style::preserved;
use crate::text::FieldValues;
use crate::units::twips_to_points;
use widths::column_widths;

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
    /// where its text starts from its left edge, and the room it keeps at
    /// its right: the larger of the cell margin and the border on each side
    pub inset_left: f64,
    pub inset_right: f64,
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
        apply_margins_xml(xml, &mut margins);
    }
    // the table's own margins, as the office model reads them
    if let Some(own) = &table.cell_margins {
        apply_model_margins(own, &mut margins);
    }
    margins
}

/// a cell's own margins over the table's (w:tcMar).
fn margins_of_cell(table_margins: CellMargins, cell: &TableCell) -> CellMargins {
    let mut margins = table_margins;
    if let Some(own) = &cell.margins {
        apply_model_margins(own, &mut margins);
    }
    margins
}

/// the sides the office model gives, in points, over `margins`.
fn apply_model_margins(own: &letters_office::model::CellMargins, margins: &mut CellMargins) {
    let points = |side: Option<i32>| side.map(twips_to_points);
    margins.left = points(own.left).unwrap_or(margins.left);
    margins.right = points(own.right).unwrap_or(margins.right);
    margins.top = points(own.top).unwrap_or(margins.top);
    margins.bottom = points(own.bottom).unwrap_or(margins.bottom);
}

/// the sides a preserved `w:tblCellMar` gives, over `margins`.
fn apply_margins_xml(xml: &str, margins: &mut CellMargins) {
    if !xml.contains("tblCellMar") {
        return;
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

/// the table's borders: its own, else its table style's (word's "table
/// grid" style draws every line; the tables word makes keep their borders
/// only there, so without this they had none: w_tables_aptos drew no rule
/// that word drew)
pub(crate) fn table_borders(document: &Document, table: &Table) -> CellBorders {
    if let Some(own) = &table.borders {
        return own.clone();
    }
    let Some(id) = crate::style::table_style_id(document, table.style_id.as_deref()) else {
        return CellBorders::default();
    };
    let mut found = CellBorders::default();
    for style in letters_office::styles::style_chain(document, &id)
        .iter()
        .rev()
    {
        for xml in &style.preserved.children {
            if let Some(borders) = borders_in_xml(xml) {
                found = borders;
            }
        }
    }
    found
}

/// the borders of a w:tblBorders element inside preserved xml.
fn borders_in_xml(xml: &str) -> Option<CellBorders> {
    let start = xml.find("<w:tblBorders")?;
    let end = xml[start..]
        .find("</w:tblBorders>")
        .map_or(xml.len(), |e| start + e);
    let mut borders = CellBorders::default();
    for tag in preserved::tags(&xml[start..end]) {
        let border = Border {
            style: tag.attr("w:val").unwrap_or("none").to_string(),
            size: tag.attr("w:sz").and_then(|s| s.parse().ok()).unwrap_or(4),
            color: tag.attr("w:color").map(str::to_string),
        };
        match tag.name.as_str() {
            "w:top" => borders.top = Some(border),
            "w:left" | "w:start" => borders.left = Some(border),
            "w:bottom" => borders.bottom = Some(border),
            "w:right" | "w:end" => borders.right = Some(border),
            "w:insideH" => borders.inside_horizontal = Some(border),
            "w:insideV" => borders.inside_vertical = Some(border),
            _ => {}
        }
    }
    Some(borders)
}

/// the widths of the table's vertical borders: its left edge, the lines
/// between columns and its right edge (0 where there is none).
fn vertical_borders(document: &Document, table: &Table) -> (f64, f64, f64) {
    let borders = table_borders(document, table);
    let width = |border: &Option<Border>| {
        border
            .as_ref()
            .and_then(border_value)
            .map_or(0.0, |(width, _, _)| width)
    };
    (
        width(&borders.left),
        width(&borders.inside_vertical),
        width(&borders.right),
    )
}

/// the table indent: the office model's, else tblInd kept as xml.
fn table_indent(table: &Table) -> f64 {
    if let Some(indent) = table.indent {
        return twips_to_points(indent);
    }
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
    // the outer border sits inside the margin plus the table indent: both
    // applications draw the left rule's centre half its width right of the
    // margin (table probes: 72.26 pt with a half-point border on a 72 pt
    // margin in word, 72.3 pt in libreoffice)
    let (left_border, _, _) = vertical_borders(context.document, table);
    let indent = table_indent(table) + left_border / 2.0;
    let left = match table.alignment {
        Some(Alignment::Center) => (available - total) / 2.0,
        Some(Alignment::Right) => available - total,
        _ => indent,
    };
    let table_borders = table_borders(context.document, table);
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
            let borders = resolve_borders(
                &table_borders,
                cell,
                r,
                column,
                span,
                table.rows.len(),
                column_count,
            );
            let border_width = |edge: usize| borders[edge].map_or(0.0, |(width, _, _)| width);
            let cell_margins = margins_of_cell(margins, cell);
            let inset_left = cell_margins.left.max(border_width(1));
            let inset_right = cell_margins.right.max(border_width(3));
            let blocks = prepare_cell_blocks(
                context,
                cell,
                story,
                &cell_path,
                (width - inset_left - inset_right).max(1.0),
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
                inset_left,
                inset_right,
                blocks,
                content_height,
                borders,
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
        // an exact height is the row's height whatever its text needs
        // (ecma-376 17.4.80 hRule "exact"; the text below it is cut off)
        let exact =
            row.height_rule == Some(letters_office::model::HeightRule::Exact) && stated > 0.0;
        let natural = (content + top_border + bottom_border).max(stated).max(1.0);
        rows.push(PreparedRow {
            cells,
            height: if exact { stated } else { natural },
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

/// each cell edge's border: the cell's own, else the table's outer or
/// inside border for that edge. (width, colour, dotted)
fn resolve_borders(
    table_borders: &CellBorders,
    cell: &TableCell,
    row: usize,
    column: usize,
    span: usize,
    rows: usize,
    columns: usize,
) -> [Option<(f64, [u8; 3], bool)>; 4] {
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
