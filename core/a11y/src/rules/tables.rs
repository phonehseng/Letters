//! tables: header cells with a scope, rows of equal length, and tags in
//! the shape Table › TR › TH/TD.

use std::collections::HashSet;

use crate::objects::ObjectRef;
use crate::report::FixKind;
use crate::rules::{Collector, Context};
use crate::structure::StructTree;

const ROW_GROUPS: &[&str] = &["THead", "TBody", "TFoot"];

pub(crate) fn check(ctx: &Context, out: &mut Collector) {
    let ids = [
        "table-missing-headers",
        "table-header-missing-scope",
        "table-irregular-rows",
        "table-structure-invalid",
    ];
    let (true, Some(tree)) = (ctx.tagged, &ctx.tree) else {
        super::skip_untagged(out, &ids);
        return;
    };
    let tables: Vec<usize> = (0..tree.elements.len())
        .filter(|&i| tree.elements[i].role == "Table")
        .collect();
    if tables.is_empty()
        && !tree
            .elements
            .iter()
            .any(|e| ["TR", "TH", "TD"].contains(&e.role.as_str()))
    {
        for id in ids {
            out.not_applicable(id, "The document has no tables.");
        }
        return;
    }
    for &table in &tables {
        check_table(ctx, tree, table, out);
    }
    check_shape(ctx, tree, out);
}

/// the rows of a table, looking inside THead, TBody and TFoot.
pub(crate) fn table_rows(tree: &StructTree, table: usize) -> Vec<usize> {
    let mut rows = Vec::new();
    for child in tree.children(table) {
        let role = tree.elements[child].role.as_str();
        if role == "TR" {
            rows.push(child);
        } else if ROW_GROUPS.contains(&role) {
            rows.extend(
                tree.children(child)
                    .filter(|&r| tree.elements[r].role == "TR"),
            );
        }
    }
    rows
}

pub(crate) fn row_cells(tree: &StructTree, row: usize) -> Vec<usize> {
    tree.children(row)
        .filter(|&c| matches!(tree.elements[c].role.as_str(), "TH" | "TD"))
        .collect()
}

fn check_table(ctx: &Context, tree: &StructTree, table: usize, out: &mut Collector) {
    let rows = table_rows(tree, table);
    let cells: Vec<usize> = rows.iter().flat_map(|&row| row_cells(tree, row)).collect();
    let headers: Vec<usize> = cells
        .iter()
        .copied()
        .filter(|&c| tree.elements[c].role == "TH")
        .collect();
    let fix = tree.elements[table].id.map(|id| FixKind::SetTableHeaders {
        table: ObjectRef::from(id),
        suggested_header_rows: 1,
        suggested_header_columns: 0,
    });
    if !cells.is_empty() && headers.is_empty() {
        out.report(
            "table-missing-headers",
            format!("A table with {} rows has no TH cells.", rows.len()),
            ctx.element_location(table),
            fix.clone(),
        );
    }
    let uses_header_ids = cells.iter().any(|&c| tree.elements[c].has_headers);
    let unscoped = headers
        .iter()
        .filter(|&&h| tree.elements[h].scope.is_none())
        .count();
    if unscoped > 0 && !uses_header_ids {
        out.report(
            "table-header-missing-scope",
            format!(
                "{unscoped} of the table’s {} header cells have no Scope.",
                headers.len()
            ),
            ctx.element_location(table),
            fix,
        );
    }
    let widths = row_widths(tree, &rows);
    if let (Some(min), Some(max)) = (widths.iter().min(), widths.iter().max()) {
        if min != max {
            out.report(
                "table-irregular-rows",
                format!("Rows have between {min} and {max} cells, counting merged cells."),
                ctx.element_location(table),
                None,
            );
        }
    }
}

/// how many grid columns each row covers, placing merged cells the way a
/// screen reader does: a cell spanning rows takes the same columns in the
/// rows below it.
fn row_widths(tree: &StructTree, rows: &[usize]) -> Vec<usize> {
    // taken[row] holds the columns already covered from rows above
    let mut taken: Vec<HashSet<usize>> = vec![HashSet::new(); rows.len()];
    for (row_number, &row) in rows.iter().enumerate() {
        let mut column = 0;
        for cell in row_cells(tree, row) {
            while taken[row_number].contains(&column) {
                column += 1;
            }
            let element = &tree.elements[cell];
            let row_span = element.row_span.max(1) as usize;
            let col_span = element.col_span.max(1) as usize;
            let last_row = (row_number + row_span).min(rows.len());
            for covered_row in &mut taken[row_number..last_row] {
                covered_row.extend(column..column + col_span);
            }
            column += col_span;
        }
    }
    taken
        .iter()
        .map(|columns| columns.iter().max().map_or(0, |last| last + 1))
        .collect()
}

/// table tags must nest Table › (THead|TBody|TFoot) › TR › TH|TD.
fn check_shape(ctx: &Context, tree: &StructTree, out: &mut Collector) {
    let mut reported = 0;
    for (index, element) in tree.elements.iter().enumerate() {
        let parent_role = element.parent.map(|p| tree.elements[p].role.as_str());
        let problem = match element.role.as_str() {
            "TR" if !matches!(parent_role, Some("Table" | "THead" | "TBody" | "TFoot")) => {
                Some("A table row (TR) isn’t inside a Table.")
            }
            "TH" | "TD" if parent_role != Some("TR") => {
                Some("A table cell isn’t inside a row (TR).")
            }
            "THead" | "TBody" | "TFoot" if parent_role != Some("Table") => {
                Some("A table section isn’t inside a Table.")
            }
            "Table" => tree
                .children(index)
                .any(|c| {
                    !matches!(
                        tree.elements[c].role.as_str(),
                        "TR" | "THead" | "TBody" | "TFoot" | "Caption"
                    )
                })
                .then_some("A Table holds something other than rows."),
            "TR" => tree
                .children(index)
                .any(|c| !matches!(tree.elements[c].role.as_str(), "TH" | "TD"))
                .then_some("A table row holds something other than cells."),
            _ => None,
        };
        if let Some(problem) = problem {
            reported += 1;
            if reported <= 50 {
                out.report(
                    "table-structure-invalid",
                    problem,
                    ctx.element_location(index),
                    None,
                );
            }
        }
    }
}
