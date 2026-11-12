//! tables found in the pdf turned into real tables: the column widths
//! and row heights it had, merged cells, the header row repeated on each
//! page, shading, ruled edges and each cell's alignment.

use letters_convert::{
    CellAlign, CellBorders as FoundBorders, Table as FoundTable, TableCell as FoundCell,
};
use letters_office::model::{
    Alignment, Block, Border, CellBorders, Paragraph, ParagraphProps, Table, TableCell, TableRow,
};

use super::pages::twips;
use super::runs::{inlines, BodyText, RunContext};

/// the office table for a table the analysis found.
pub fn table(found: &FoundTable, body: &BodyText) -> Table {
    let rows = (0..found.rows)
        .map(|row| table_row(found, row, body))
        .collect();
    Table {
        column_widths: found
            .column_widths
            .iter()
            .map(|width| twips(*width))
            .collect(),
        width: Some(twips(found.column_widths.iter().sum())),
        rows,
        ..Table::default()
    }
}

fn table_row(found: &FoundTable, row: usize, body: &BodyText) -> TableRow {
    // a merged cell is listed once, at its top left, and the cells it
    // covers are left out, which is how the office model wants it too
    let mut cells: Vec<&FoundCell> = found.cells.iter().filter(|cell| cell.row == row).collect();
    cells.sort_by_key(|cell| cell.column);
    TableRow {
        height: found.row_heights.get(row).map(|height| twips(*height)),
        header: row < found.header_rows,
        cells: cells
            .into_iter()
            .map(|cell| table_cell(found, cell, body))
            .collect(),
        ..TableRow::default()
    }
}

fn table_cell(found: &FoundTable, cell: &FoundCell, body: &BodyText) -> TableCell {
    let spanned: f32 = found
        .column_widths
        .iter()
        .skip(cell.column)
        .take(cell.column_span.max(1))
        .sum();
    let paragraph = Paragraph {
        props: ParagraphProps {
            alignment: alignment(cell.align),
            ..ParagraphProps::default()
        },
        content: inlines(&cell.runs, body, RunContext::Body),
        ..Paragraph::default()
    };
    TableCell {
        col_span: cell.column_span.max(1) as u32,
        row_span: cell.row_span.max(1) as u32,
        width: Some(twips(spanned)),
        borders: borders(cell.borders),
        shading: cell.fill.map(|fill| fill.hex()),
        blocks: vec![Block::Paragraph(paragraph)],
        ..TableCell::default()
    }
}

/// left is the default, so it is not written.
fn alignment(align: CellAlign) -> Option<Alignment> {
    match align {
        CellAlign::Left => None,
        CellAlign::Center => Some(Alignment::Center),
        CellAlign::Right => Some(Alignment::Right),
    }
}

/// a thin single line on each edge the pdf drew one.
fn borders(found: FoundBorders) -> Option<CellBorders> {
    if found == FoundBorders::default() {
        return None;
    }
    let line = |drawn: bool| {
        drawn.then(|| Border {
            style: "single".into(),
            size: 4,
            color: None,
        })
    };
    Some(CellBorders {
        top: line(found.top),
        bottom: line(found.bottom),
        left: line(found.left),
        right: line(found.right),
        ..CellBorders::default()
    })
}
