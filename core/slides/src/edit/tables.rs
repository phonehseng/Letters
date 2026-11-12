//! table edits: a cell's text, and inserting or deleting rows and columns
//! (merged cells are kept whole: a span shrinks or grows around the change).

use super::text::set_paragraphs;
use super::{object_index, slide_mut, snapshot, Applied, Operation};
use crate::error::SlidesError;
use crate::model::{GraphicKind, Object, Presentation, Table, TableCell, TableRow, TextBody};

pub fn apply_table(
    presentation: &mut Presentation,
    operation: &Operation,
) -> Result<Applied, SlidesError> {
    let (slide_id, object_id) = match operation {
        Operation::SetCellText {
            slide_id,
            object_id,
            ..
        }
        | Operation::InsertRow {
            slide_id,
            object_id,
            ..
        }
        | Operation::DeleteRow {
            slide_id,
            object_id,
            ..
        }
        | Operation::InsertColumn {
            slide_id,
            object_id,
            ..
        }
        | Operation::DeleteColumn {
            slide_id,
            object_id,
            ..
        } => (slide_id.clone(), *object_id),
        _ => return Err(SlidesError::InvalidEdit("not a table edit".into())),
    };
    let inverse = snapshot(presentation, &slide_id)?;
    let slide = slide_mut(presentation, &slide_id)?;
    let index = object_index(slide, object_id)?;
    let Object::Frame(frame) = &mut slide.objects[index] else {
        return Err(SlidesError::InvalidEdit(
            "that object is not a table".into(),
        ));
    };
    let GraphicKind::Table(table) = &mut frame.kind else {
        return Err(SlidesError::InvalidEdit(
            "that object is not a table".into(),
        ));
    };
    match operation {
        Operation::SetCellText {
            row, column, text, ..
        } => {
            let cell = table
                .rows
                .get_mut(*row)
                .and_then(|r| r.cells.get_mut(*column))
                .ok_or_else(|| SlidesError::InvalidEdit("there is no such cell".into()))?;
            let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
            set_paragraphs(&mut cell.text, &lines);
        }
        Operation::InsertRow { at, .. } => insert_row(table, *at),
        Operation::DeleteRow { at, .. } => delete_row(table, *at)?,
        Operation::InsertColumn { at, .. } => insert_column(table, *at),
        Operation::DeleteColumn { at, .. } => delete_column(table, *at)?,
        _ => {}
    }
    Ok(Applied::slide(&slide_id, inverse))
}

/// an empty cell formatted like `like`.
fn empty_like(like: Option<&TableCell>) -> TableCell {
    let mut cell = like.cloned().unwrap_or_default();
    let template = cell.text.paragraphs.first().cloned().unwrap_or_default();
    cell.text = TextBody {
        paragraphs: vec![crate::model::TextParagraph {
            runs: Vec::new(),
            source: None,
            ..template
        }],
        ..cell.text
    };
    cell.grid_span = 1;
    cell.row_span = 1;
    cell.h_merge = false;
    cell.v_merge = false;
    cell.source = None;
    cell
}

/// a new row before `at`, as tall and formatted as the row above it.
fn insert_row(table: &mut Table, at: usize) {
    let at = at.min(table.rows.len());
    let model = table
        .rows
        .get(at.saturating_sub(1))
        .cloned()
        .unwrap_or_default();
    let cells = (0..table.columns.len())
        .map(|column| empty_like(model.cells.get(column)))
        .collect();
    table.rows.insert(
        at,
        TableRow {
            height: model.height,
            cells,
            source: None,
        },
    );
}

fn delete_row(table: &mut Table, at: usize) -> Result<(), SlidesError> {
    if at >= table.rows.len() || table.rows.len() == 1 {
        return Err(SlidesError::InvalidEdit(
            "that row cannot be deleted".into(),
        ));
    }
    // a vertical merge starting here hands its start to the next row
    let removed = table.rows.remove(at);
    for (column, cell) in removed.cells.iter().enumerate() {
        if cell.row_span > 1 {
            if let Some(next) = table.rows.get_mut(at).and_then(|r| r.cells.get_mut(column)) {
                next.v_merge = false;
                next.row_span = cell.row_span - 1;
                next.source = None;
            }
        }
    }
    Ok(())
}

/// a new column before `at`, as wide as the column left of it.
fn insert_column(table: &mut Table, at: usize) {
    let at = at.min(table.columns.len());
    let width = table
        .columns
        .get(at.saturating_sub(1))
        .copied()
        .unwrap_or(914_400);
    table.columns.insert(at, width);
    for row in &mut table.rows {
        let like = row.cells.get(at.saturating_sub(1)).cloned();
        row.cells
            .insert(at.min(row.cells.len()), empty_like(like.as_ref()));
        row.source = None;
    }
}

fn delete_column(table: &mut Table, at: usize) -> Result<(), SlidesError> {
    if at >= table.columns.len() || table.columns.len() == 1 {
        return Err(SlidesError::InvalidEdit(
            "that column cannot be deleted".into(),
        ));
    }
    table.columns.remove(at);
    for row in &mut table.rows {
        if at < row.cells.len() {
            let removed = row.cells.remove(at);
            if removed.grid_span > 1 {
                if let Some(next) = row.cells.get_mut(at) {
                    next.h_merge = false;
                    next.grid_span = removed.grid_span - 1;
                    next.text = removed.text.clone();
                }
            }
        }
        row.source = None;
    }
    Ok(())
}
