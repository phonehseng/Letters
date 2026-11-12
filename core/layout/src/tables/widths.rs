//! column widths: the file's grid, or the columns fitted to their content
//! the way word's "autofit to contents" fits them (measured with the table
//! probes, tests/oracle/probes.rs and `fit_table_probes`).

use letters_office::model::{Block, Table, TableCell};

use super::{cell_margins, vertical_borders, CellMargins};
use crate::pages::Context;
use crate::result::Story;
use crate::style::preserved;
use crate::text::FieldValues;
use crate::units::twips_to_points;

/// the widest a column's natural width counts in word's autofit: 22 inches,
/// word's largest page width. measured on the table probes: columns whose
/// text on one line is longer share the room as if it were 1584 pt
/// (probe 13: 163.35 and 304.25 pt for natural widths of 706 and 5302 pt,
/// within 0.1 pt of the cap, 141 pt off without it)
const NATURAL_WIDTH_CAP: f64 = 1584.0;

/// column widths: the grid when it holds, else fitted to content.
#[allow(clippy::too_many_arguments)]
pub(super) fn column_widths(
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
    let grid_is_whole = grid.len() == count && grid.iter().all(|w| *w > 0.0);
    if grid_is_whole && keeps_grid(context, table) {
        return grid;
    }
    let (narrowest, natural) = measure_columns(context, table, story, path, count, margins, fields);
    let (left_border, _, right_border) = vertical_borders(context.document, table);
    // word measures the room between the outer borders' centres: the
    // probes' full-width tables span 467.6 pt between their rule centres
    // on a 468 pt measure with half-point borders
    let room = available - (left_border + right_border) / 2.0;
    let target = table
        .width
        .map(twips_to_points)
        .unwrap_or(room)
        .min(room.max(1.0));
    distribute(&narrowest, &natural, target, table.width.is_some())
}

/// whether the application lays the table out on the file's grid as it is.
///
/// libreoffice: always, for docx (all 23 table probes had the grid's equal
/// columns although their texts differed). word keeps the grid when the
/// layout is fixed or every cell says how wide it wants to be; otherwise it
/// fits the columns to their content again (tables_auto and tables_long in
/// the corpus, where cells without widths made word ignore the grid).
fn keeps_grid(context: &Context, table: &Table) -> bool {
    if context.flavor == crate::Flavor::LibreOffice {
        return true;
    }
    // the office model reads w:tblLayout now; files read before kept it as xml
    let fixed_layout = table.fixed_layout == Some(true)
        || preserved::find(&table.preserved.children, "w:tblLayout")
            .is_some_and(|tag| tag.attr("w:type") == Some("fixed"));
    let every_cell_sized = table.rows.iter().all(|row| {
        row.cells
            .iter()
            .all(|cell| cell.width.is_some_and(|w| w > 0))
    });
    fixed_layout || every_cell_sized
}

/// the space a column keeps beside its text: on each side the larger of
/// the cell margin and the border. word's probes without cell margins were
/// 1.0 pt wider than their text with half-point borders, those with 5.4 pt
/// margins exactly 10.8 pt wider (the borders sit inside the margins).
pub(super) fn side_space(margins: CellMargins, left_border: f64, right_border: f64) -> f64 {
    margins.left.max(left_border) + margins.right.max(right_border)
}

/// narrowest and natural widths of each column from its single cells,
/// side space included.
#[allow(clippy::too_many_arguments)]
fn measure_columns(
    context: &mut Context,
    table: &Table,
    story: &Story,
    path: &[u32],
    count: usize,
    margins: CellMargins,
    fields: FieldValues,
) -> (Vec<f64>, Vec<f64>) {
    let (left_edge, inside, right_edge) = vertical_borders(context.document, table);
    let space = |column: usize| {
        let left = if column == 0 { left_edge } else { inside };
        let right = if column + 1 == count {
            right_edge
        } else {
            inside
        };
        side_space(margins, left, right)
    };
    let mut narrowest: Vec<f64> = (0..count).map(space).collect();
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
                    .unwrap_or(max + space(column));
                narrowest[column] = narrowest[column].max(min + space(column));
                natural[column] = natural[column].max(max.min(NATURAL_WIDTH_CAP));
            }
            column += cell.col_span.max(1) as usize;
        }
    }
    (narrowest, natural)
}

/// the narrowest and natural width of each column of the table at body
/// block `block`, as the autofit rule sees them (side space included):
/// for measuring an application's autofit against.
pub fn column_content_widths(
    document: &letters_office::model::Document,
    block: usize,
    flavor: crate::Flavor,
) -> Vec<(f64, f64)> {
    let Some(Block::Table(table)) = document.blocks.get(block) else {
        return Vec::new();
    };
    let options = crate::LayoutOptions::flavor(flavor);
    let fonts = crate::fonts::FontLibrary::new(crate::engine::font_source(document, &options));
    let mut context = crate::engine::new_context(document, &options, fonts);
    let margins = cell_margins(&context, table);
    context.table_style = Some(crate::style::table_style_props(
        document,
        table.style_id.as_deref(),
    ));
    let count = letters_office::tables::grid(table)
        .first()
        .map_or(0, Vec::len)
        .max(table.column_widths.len())
        .max(1);
    let (narrowest, natural) = measure_columns(
        &mut context,
        table,
        &Story::Body,
        &[block as u32],
        count,
        margins,
        FieldValues::default(),
    );
    narrowest.into_iter().zip(natural).collect()
}

/// shares `target` between columns from their narrowest and natural
/// widths: each gets its narrowest, and the rest of the room in proportion
/// to how much wider it would like to be. the table probes matched this
/// within 0.1 pt in 19 of 19 tables wider than the measure, once natural
/// widths are capped (`NATURAL_WIDTH_CAP`). a table that fits naturally
/// keeps its natural widths (it does not stretch unless its width is set).
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

    #[test]
    fn a_probe_wider_than_the_page_shares_like_word() {
        // table probe 11 (no cell margins): natural widths of 1110.5 and
        // 1949.0 pt, word gave 202.59 and 265.01 pt
        let narrowest = [65.47, 65.94];
        let natural = [
            1111.54_f64.min(NATURAL_WIDTH_CAP),
            1948.99_f64.min(NATURAL_WIDTH_CAP),
        ];
        let widths = distribute(&narrowest, &natural, 467.6, false);
        assert!((widths[0] - 202.59).abs() < 0.1, "{widths:?}");
        assert!((widths[1] - 265.01).abs() < 0.1, "{widths:?}");
    }

    #[test]
    fn borders_count_where_margins_are_narrower() {
        let none = CellMargins {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
        assert_eq!(side_space(none, 0.5, 0.5), 1.0);
        let word = CellMargins {
            left: 5.4,
            right: 5.4,
            ..none
        };
        assert_eq!(side_space(word, 0.5, 0.5), 10.8);
    }
}
