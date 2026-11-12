//! finding tables: grids of drawn lines ("ruled" tables) and text lined
//! up in columns with only white space between them.

use super::fragments::{Fragment, Glyph};
use super::model::{CellAlign, CellBorders, Table, TableCell, TextRun};
use crate::content::{FilledBox, Rgb, RuleLine};
use crate::geometry::{union_all, Rect};

/// lines this close (in points) count as touching or as the same line
const TOUCH: f32 = 1.5;

/// a table found on a page, with the fragments it took its text from.
#[derive(Clone, Debug)]
pub(crate) struct FoundTable {
    pub table: Table,
    pub rect: Rect,
    pub confidence: f32,
    /// indexes into the page's fragments
    pub used: Vec<usize>,
}

// ---------------------------------------------------------------- ruled

/// grids of horizontal and vertical lines that cross each other.
pub(crate) fn ruled_tables(
    lines: &[RuleLine],
    fills: &[FilledBox],
    fragments: &[Fragment],
) -> Vec<FoundTable> {
    let horizontal = merge_lines(lines.iter().filter(|l| l.horizontal).copied().collect());
    let vertical = merge_lines(lines.iter().filter(|l| !l.horizontal).copied().collect());
    let mut tables = Vec::new();
    for (h_group, v_group) in connected_grids(&horizontal, &vertical) {
        let h: Vec<RuleLine> = h_group.iter().map(|i| horizontal[*i]).collect();
        let v: Vec<RuleLine> = v_group.iter().map(|i| vertical[*i]).collect();
        if let Some(table) = grid_table(&h, &v, fills, fragments) {
            tables.push(table);
        }
    }
    tables
}

/// joins pieces of one line drawn end to end (many programs draw a
/// table's lines cell by cell).
fn merge_lines(mut lines: Vec<RuleLine>) -> Vec<RuleLine> {
    lines.retain(|l| l.end - l.start >= 3.0);
    lines.sort_by(|a, b| {
        a.position
            .total_cmp(&b.position)
            .then(a.start.total_cmp(&b.start))
    });
    let mut merged: Vec<RuleLine> = Vec::new();
    for line in lines {
        if let Some(last) = merged.last_mut() {
            if (last.position - line.position).abs() <= TOUCH && line.start <= last.end + TOUCH {
                last.end = last.end.max(line.end);
                continue;
            }
        }
        merged.push(line);
    }
    merged
}

fn crosses(h: &RuleLine, v: &RuleLine) -> bool {
    v.position >= h.start - TOUCH
        && v.position <= h.end + TOUCH
        && h.position >= v.start - TOUCH
        && h.position <= v.end + TOUCH
}

/// groups of lines that touch, each with at least two lines each way.
fn connected_grids(
    horizontal: &[RuleLine],
    vertical: &[RuleLine],
) -> Vec<(Vec<usize>, Vec<usize>)> {
    // union-find over all lines: horizontal ones first, then vertical
    let count = horizontal.len() + vertical.len();
    let mut parent: Vec<usize> = (0..count).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (hi, h) in horizontal.iter().enumerate() {
        for (vi, v) in vertical.iter().enumerate() {
            if crosses(h, v) {
                let (a, b) = (
                    root(&mut parent, hi),
                    root(&mut parent, horizontal.len() + vi),
                );
                parent[a] = b;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>, Vec<usize>)> = Vec::new();
    for i in 0..count {
        let r = root(&mut parent, i);
        let slot = match groups.iter().position(|(key, _, _)| *key == r) {
            Some(slot) => slot,
            None => {
                groups.push((r, Vec::new(), Vec::new()));
                groups.len() - 1
            }
        };
        if i < horizontal.len() {
            groups[slot].1.push(i);
        } else {
            groups[slot].2.push(i - horizontal.len());
        }
    }
    groups
        .into_iter()
        .filter(|(_, h, v)| h.len() >= 2 && v.len() >= 2)
        .map(|(_, h, v)| (h, v))
        .collect()
}

/// positions closer than `TOUCH` become one, in order.
fn cluster_positions(mut positions: Vec<f32>) -> Vec<f32> {
    positions.sort_by(f32::total_cmp);
    let mut clusters: Vec<(f32, usize)> = Vec::new();
    for position in positions {
        match clusters.last_mut() {
            Some((sum, count)) if position - *sum / *count as f32 <= TOUCH => {
                *sum += position;
                *count += 1;
            }
            _ => clusters.push((position, 1)),
        }
    }
    clusters
        .iter()
        .map(|(sum, count)| sum / *count as f32)
        .collect()
}

/// whether some line at `position` covers most of `from..to`.
fn has_line(lines: &[RuleLine], position: f32, from: f32, to: f32) -> bool {
    let length = to - from;
    lines.iter().any(|line| {
        (line.position - position).abs() <= TOUCH
            && line.end.min(to) - line.start.max(from) >= length * 0.8
    })
}

fn grid_table(
    h: &[RuleLine],
    v: &[RuleLine],
    fills: &[FilledBox],
    fragments: &[Fragment],
) -> Option<FoundTable> {
    let ys = cluster_positions(h.iter().map(|l| l.position).collect());
    let xs = cluster_positions(v.iter().map(|l| l.position).collect());
    let (rows, columns) = (ys.len().checked_sub(1)?, xs.len().checked_sub(1)?);
    if rows * columns < 2 || rows == 0 || columns == 0 {
        return None;
    }
    let rect = Rect::from_corners(xs[0], ys[0], xs[columns], ys[rows]);

    // cells next to each other with no line between them are one cell
    let mut group: Vec<usize> = (0..rows * columns).collect();
    let mut rectangular = true;
    for row in 0..rows {
        for column in 0..columns {
            let here = row * columns + column;
            if column + 1 < columns && !has_line(v, xs[column + 1], ys[row], ys[row + 1]) {
                join(&mut group, here, here + 1);
            }
            if row + 1 < rows && !has_line(h, ys[row + 1], xs[column], xs[column + 1]) {
                join(&mut group, here, here + columns);
            }
        }
    }
    let mut cells = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let here = row * columns + column;
            let key = find(&mut group, here);
            let members: Vec<usize> = (0..rows * columns)
                .filter(|i| find(&mut group, *i) == key)
                .collect();
            if members[0] != here {
                continue;
            }
            let last_row = members.iter().map(|m| m / columns).max().unwrap_or(row);
            let last_column = members.iter().map(|m| m % columns).max().unwrap_or(column);
            let row_span = last_row - row + 1;
            let column_span = last_column - column + 1;
            rectangular &= members.len() == row_span * column_span;
            let cell_rect =
                Rect::from_corners(xs[column], ys[row], xs[last_column + 1], ys[last_row + 1]);
            cells.push(TableCell {
                row,
                column,
                row_span,
                column_span,
                runs: Vec::new(),
                rect: cell_rect,
                fill: cell_fill(fills, &cell_rect),
                borders: CellBorders {
                    top: has_line(h, cell_rect.y, cell_rect.x, cell_rect.right()),
                    bottom: has_line(h, cell_rect.bottom(), cell_rect.x, cell_rect.right()),
                    left: has_line(v, cell_rect.x, cell_rect.y, cell_rect.bottom()),
                    right: has_line(v, cell_rect.right(), cell_rect.y, cell_rect.bottom()),
                },
                align: CellAlign::Left,
            });
        }
    }
    let used = fill_cells(&mut cells, fragments, &rect);
    let mut table = Table {
        rows,
        columns,
        column_widths: xs.windows(2).map(|w| w[1] - w[0]).collect(),
        row_heights: ys.windows(2).map(|w| w[1] - w[0]).collect(),
        cells,
        header_rows: 0,
        header_confidence: 0.0,
        ruled: true,
        caption: None,
    };
    guess_header(&mut table);
    Some(FoundTable {
        table,
        rect,
        confidence: if rectangular { 0.9 } else { 0.6 },
        used,
    })
}

fn find(group: &mut [usize], mut i: usize) -> usize {
    while group[i] != i {
        group[i] = group[group[i]];
        i = group[i];
    }
    i
}

fn join(group: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(group, a), find(group, b));
    // the smaller index stays the root, so a merged cell is named by its
    // top left part
    let (low, high) = (ra.min(rb), ra.max(rb));
    group[high] = low;
}

/// the shading drawn over most of a cell, the last one drawn winning
/// (later paint is on top). white is no shading.
fn cell_fill(fills: &[FilledBox], cell: &Rect) -> Option<Rgb> {
    let area = cell.width * cell.height;
    fills
        .iter()
        .rev()
        .find(|fill| {
            let overlap = fill.rect.horizontal_overlap(cell).max(0.0)
                * fill.rect.vertical_overlap(cell).max(0.0);
            overlap >= area * 0.8
        })
        .map(|fill| fill.color)
        .filter(|color| *color != Rgb(255, 255, 255))
}

/// puts each character inside the table into the cell under its middle,
/// and returns the fragments that were used.
fn fill_cells(cells: &mut [TableCell], fragments: &[Fragment], table: &Rect) -> Vec<usize> {
    let mut used = Vec::new();
    // per cell: its lines, each a list of glyphs with the baseline
    let mut lines: Vec<Vec<(f32, f32, Vec<Glyph>)>> = vec![Vec::new(); cells.len()];
    for (index, fragment) in fragments.iter().enumerate() {
        if !table.contains_point(fragment.rect.center_x(), fragment.rect.center_y()) {
            continue;
        }
        used.push(index);
        for glyph in &fragment.glyphs {
            let (x, y) = (glyph.rect.center_x(), glyph.rect.center_y());
            let Some(cell) = cells.iter().position(|c| c.rect.contains_point(x, y)) else {
                continue;
            };
            let cell_lines = &mut lines[cell];
            match cell_lines
                .iter_mut()
                .find(|(baseline, _, _)| (*baseline - fragment.baseline).abs() < 0.5)
            {
                Some((_, _, glyphs)) => glyphs.push(glyph.clone()),
                None => cell_lines.push((fragment.baseline, fragment.size, vec![glyph.clone()])),
            }
        }
    }
    for (cell, cell_lines) in cells.iter_mut().zip(lines) {
        let fragments: Vec<Fragment> = cell_lines
            .into_iter()
            .map(|(baseline, size, glyphs)| glyphs_fragment(glyphs, baseline, size))
            .collect();
        cell.align = alignment(&cell.rect, &fragments);
        cell.runs = joined_lines(fragments);
    }
    used
}

pub(crate) fn glyphs_fragment(mut glyphs: Vec<Glyph>, baseline: f32, size: f32) -> Fragment {
    glyphs.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    let rect = union_all(glyphs.iter().map(|g| &g.rect)).unwrap_or_default();
    Fragment {
        rect,
        baseline,
        size,
        origin: (rect.x, baseline),
        glyphs,
        rotated: false,
        angle: 0.0,
        invisible: false,
    }
}

/// lines of one cell, top to bottom, joined into one stretch of text.
fn joined_lines(mut fragments: Vec<Fragment>) -> Vec<TextRun> {
    fragments.sort_by(|a, b| a.baseline.total_cmp(&b.baseline));
    let lines: Vec<Vec<TextRun>> = fragments.iter().map(Fragment::runs).collect();
    super::blocks::join_lines(lines, true)
}

/// left, centred or right, from the space the text leaves either side.
fn alignment(cell: &Rect, fragments: &[Fragment]) -> CellAlign {
    let Some(text) = union_all(fragments.iter().map(|f| &f.rect)) else {
        return CellAlign::Left;
    };
    let left = text.x - cell.x;
    let right = cell.right() - text.right();
    if left > 2.0 && right > 2.0 && (left - right).abs() <= (0.15 * (left + right)).max(2.0) {
        CellAlign::Center
    } else if right + 2.0 < left && right < cell.width * 0.25 {
        CellAlign::Right
    } else {
        CellAlign::Left
    }
}

// ---------------------------------------------------------- white space

/// text lined up in columns: at least three rows one after the other,
/// each with pieces of text in at least two of the same columns.
pub(crate) fn whitespace_tables(fragments: &[Fragment], taken: &[bool]) -> Vec<FoundTable> {
    let rows = text_rows(fragments, taken);
    let mut tables = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let mut end = start;
        while end < rows.len()
            && rows[end].len() >= 2
            && (end == start || close_rows(fragments, &rows[end - 1], &rows[end]))
        {
            end += 1;
        }
        if end - start >= 3 {
            if let Some(table) = aligned_table(fragments, &rows[start..end]) {
                tables.push(table);
            }
        }
        start = end.max(start + 1);
    }
    tables
}

/// the fragments not yet taken, grouped by baseline, top to bottom.
fn text_rows(fragments: &[Fragment], taken: &[bool]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..fragments.len())
        .filter(|i| !taken[*i] && !fragments[*i].rotated)
        .collect();
    order.sort_by(|a, b| fragments[*a].baseline.total_cmp(&fragments[*b].baseline));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for index in order {
        let fragment = &fragments[index];
        let same_row = rows.last().is_some_and(|row| {
            let first = &fragments[row[0]];
            (first.baseline - fragment.baseline).abs() <= 0.35 * first.size.min(fragment.size)
        });
        if same_row {
            rows.last_mut().expect("checked").push(index);
        } else {
            rows.push(vec![index]);
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| fragments[*a].rect.x.total_cmp(&fragments[*b].rect.x));
    }
    rows
}

fn close_rows(fragments: &[Fragment], above: &[usize], below: &[usize]) -> bool {
    let size = fragments[above[0]].size.max(fragments[below[0]].size);
    fragments[below[0]].baseline - fragments[above[0]].baseline <= size * 2.6
}

/// x ranges that overlap (or nearly) become one column, left to right.
fn column_ranges(ranges: &mut [(f32, f32)]) -> Vec<(f32, f32)> {
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns: Vec<(f32, f32)> = Vec::new();
    for (start, end) in ranges.iter() {
        match columns.last_mut() {
            Some(last) if *start <= last.1 + 2.0 => last.1 = last.1.max(*end),
            _ => columns.push((*start, *end)),
        }
    }
    columns
}

fn aligned_table(fragments: &[Fragment], rows: &[Vec<usize>]) -> Option<FoundTable> {
    // the columns come from the rows below the first, so a heading that
    // spans two columns doesn't join them into one
    let mut ranges: Vec<(f32, f32)> = rows[1..]
        .iter()
        .flatten()
        .map(|i| (fragments[*i].rect.x, fragments[*i].rect.right()))
        .collect();
    let columns = column_ranges(&mut ranges);
    if columns.len() < 2 {
        return None;
    }
    let pieces: Vec<&Fragment> = rows.iter().flatten().map(|i| &fragments[*i]).collect();
    let mean_chars =
        pieces.iter().map(|f| f.char_count()).sum::<usize>() as f32 / pieces.len() as f32;
    // two columns of long lines are a page set in two columns, not a
    // table; so are two columns of lines that each fill their column
    if columns.len() == 2
        && (mean_chars > 25.0 || (mean_chars > 15.0 && columns_are_filled(&pieces, &columns)))
    {
        return None;
    }
    if mean_chars > 40.0 {
        return None;
    }
    let column_of = |fragment: &Fragment| -> (usize, usize) {
        let touched: Vec<usize> = (0..columns.len())
            .filter(|c| {
                fragment.rect.right().min(columns[*c].1) - fragment.rect.x.max(columns[*c].0) > -1.0
            })
            .collect();
        match (touched.first(), touched.last()) {
            (Some(first), Some(last)) => (*first, *last),
            _ => {
                let nearest = (0..columns.len())
                    .min_by(|a, b| {
                        let da = (columns[*a].0 - fragment.rect.x).abs();
                        let db = (columns[*b].0 - fragment.rect.x).abs();
                        da.total_cmp(&db)
                    })
                    .unwrap_or(0);
                (nearest, nearest)
            }
        }
    };

    let row_tops: Vec<f32> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|i| fragments[*i].rect.y)
                .fold(f32::INFINITY, f32::min)
        })
        .collect();
    let row_bottoms: Vec<f32> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|i| fragments[*i].rect.bottom())
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .collect();
    let mut ys = vec![row_tops[0] - 2.0];
    for r in 1..rows.len() {
        ys.push((row_bottoms[r - 1] + row_tops[r]) / 2.0);
    }
    ys.push(row_bottoms[rows.len() - 1] + 2.0);
    let mut xs = vec![columns[0].0 - 2.0];
    for c in 1..columns.len() {
        xs.push((columns[c - 1].1 + columns[c].0) / 2.0);
    }
    xs.push(columns[columns.len() - 1].1 + 2.0);

    let mut cells: Vec<TableCell> = Vec::new();
    for (row, members) in rows.iter().enumerate() {
        for index in members {
            let fragment = &fragments[*index];
            let (first, last) = column_of(fragment);
            match cells.iter_mut().find(|c| c.row == row && c.column == first) {
                Some(cell) => {
                    let mut runs = std::mem::take(&mut cell.runs);
                    runs.push(space_like(&runs));
                    runs.extend(fragment.runs());
                    cell.runs = runs;
                }
                None => {
                    let rect = Rect::from_corners(xs[first], ys[row], xs[last + 1], ys[row + 1]);
                    cells.push(TableCell {
                        row,
                        column: first,
                        row_span: 1,
                        column_span: last - first + 1,
                        runs: fragment.runs(),
                        rect,
                        fill: None,
                        borders: CellBorders::default(),
                        align: alignment(&rect, std::slice::from_ref(fragment)),
                    });
                }
            }
        }
    }
    cells.sort_by_key(|c| (c.row, c.column));
    let rect = Rect::from_corners(xs[0], ys[0], xs[columns.len()], ys[rows.len()]);
    let mut table = Table {
        rows: rows.len(),
        columns: columns.len(),
        column_widths: xs.windows(2).map(|w| w[1] - w[0]).collect(),
        row_heights: ys.windows(2).map(|w| w[1] - w[0]).collect(),
        cells,
        header_rows: 0,
        header_confidence: 0.0,
        ruled: false,
        caption: None,
    };
    guess_header(&mut table);
    let confidence =
        (0.45 + 0.08 * rows.len() as f32 + if columns.len() > 2 { 0.1 } else { 0.0 }).min(0.8);
    Some(FoundTable {
        table,
        rect,
        confidence,
        used: rows.iter().flatten().copied().collect(),
    })
}

/// whether most lines in every column are nearly as wide as the column,
/// the way running text fills its column (table cells vary much more).
fn columns_are_filled(pieces: &[&Fragment], columns: &[(f32, f32)]) -> bool {
    columns.iter().all(|(start, end)| {
        let widths: Vec<f32> = pieces
            .iter()
            .filter(|f| f.rect.center_x() >= *start && f.rect.center_x() <= *end)
            .map(|f| f.rect.width)
            .collect();
        let widest = widths.iter().copied().fold(0.0, f32::max);
        let filled = widths.iter().filter(|w| **w >= widest * 0.6).count();
        !widths.is_empty() && filled as f32 >= widths.len() as f32 * 0.7
    })
}

fn space_like(runs: &[TextRun]) -> TextRun {
    let mut space = runs
        .last()
        .cloned()
        .expect("a cell has text before more is added");
    space.text = " ".into();
    space
}

// --------------------------------------------------------------- header

/// guesses whether the first row holds the column headings: it is bold,
/// or shaded unlike the rows below, or it is words over columns of numbers.
fn guess_header(table: &mut Table) {
    if table.rows < 2 {
        return;
    }
    let with_text = |first_row: bool| -> Vec<&TableCell> {
        table
            .cells
            .iter()
            .filter(|c| (c.row == 0) == first_row && !c.text().trim().is_empty())
            .collect()
    };
    let (first, rest) = (with_text(true), with_text(false));
    let is_bold = |cell: &TableCell| {
        cell.runs
            .iter()
            .filter(|r| !r.text.trim().is_empty())
            .all(|r| r.bold)
    };
    let all_bold = !first.is_empty() && first.iter().all(|c| is_bold(c));
    let first_fill = table.cells.iter().find(|c| c.row == 0).and_then(|c| c.fill);
    let shaded = first_fill.is_some()
        && table
            .cells
            .iter()
            .filter(|c| c.row > 0)
            .all(|c| c.fill != first_fill);
    let numbers_below = rest.iter().filter(|c| looks_numeric(&c.text())).count();
    let words_over_numbers =
        first.iter().all(|c| !looks_numeric(&c.text())) && numbers_below * 2 >= rest.len().max(1);
    let confidence: f32 = if all_bold {
        0.85
    } else if shaded {
        0.8
    } else if words_over_numbers {
        0.65
    } else {
        0.3
    };
    table.header_confidence = confidence;
    table.header_rows = usize::from(confidence >= 0.5);
}

/// a number, maybe with a currency sign, thousands separators, a percent
/// sign or brackets for a negative amount.
pub(crate) fn looks_numeric(text: &str) -> bool {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '$' | '€' | '£' | '¥' | ',' | '%' | '(' | ')' | ' ' | '+' | '-' | '\u{a0}'
            )
        })
        .collect();
    !cleaned.is_empty() && cleaned.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::fragments::tests::fragment;
    use super::*;
    use crate::content::Rgb;

    fn rule(horizontal: bool, position: f32, start: f32, end: f32) -> RuleLine {
        RuleLine {
            horizontal,
            position,
            start,
            end,
            width: 1.0,
            color: Rgb::BLACK,
        }
    }

    /// a 3 by 2 grid from x 100 to 300 and y 100 to 160, the middle
    /// vertical line missing in the first row (a merged heading).
    fn grid_lines() -> Vec<RuleLine> {
        vec![
            rule(true, 100.0, 100.0, 300.0),
            rule(true, 120.0, 100.0, 300.0),
            rule(true, 140.0, 100.0, 300.0),
            rule(true, 160.0, 100.0, 300.0),
            rule(false, 100.0, 100.0, 160.0),
            rule(false, 200.0, 120.0, 160.0),
            rule(false, 300.0, 100.0, 160.0),
        ]
    }

    #[test]
    fn ruled_grids_become_cells_with_spans() {
        let fragments = vec![
            fragment("Results", 180.0, 114.0, 10.0, true),
            fragment("Name", 105.0, 134.0, 10.0, false),
            fragment("12", 280.0, 134.0, 10.0, false),
            fragment("Ana", 105.0, 154.0, 10.0, false),
            fragment("7", 285.0, 154.0, 10.0, false),
            fragment("Outside the table", 105.0, 300.0, 10.0, false),
        ];
        let fills = vec![FilledBox {
            rect: Rect::new(100.0, 100.0, 200.0, 20.0),
            color: Rgb(200, 200, 200),
        }];
        let tables = ruled_tables(&grid_lines(), &fills, &fragments);
        assert_eq!(tables.len(), 1);
        let found = &tables[0];
        let table = &found.table;
        assert_eq!((table.rows, table.columns), (3, 2));
        assert_eq!(table.column_widths, vec![100.0, 100.0]);
        assert_eq!(table.cells.len(), 5);
        let heading = table.cell(0, 0).unwrap();
        assert_eq!(heading.column_span, 2);
        assert_eq!(heading.text(), "Results");
        assert_eq!(heading.fill, Some(Rgb(200, 200, 200)));
        assert_eq!(heading.align, CellAlign::Center);
        assert_eq!(table.cell(1, 1).unwrap().text(), "12");
        assert_eq!(table.cell(1, 1).unwrap().align, CellAlign::Right);
        assert!(table.cell(2, 0).unwrap().borders.bottom);
        assert_eq!(table.header_rows, 1);
        assert_eq!(found.used, vec![0, 1, 2, 3, 4]);
        assert!(found.confidence >= 0.9);
    }

    #[test]
    fn lined_up_text_is_a_table() {
        let mut fragments = Vec::new();
        let rows = [
            ("Item", "Qty", "Price"),
            ("Pens", "4", "$2.00"),
            ("Paper", "10", "$5.50"),
            ("Ink", "1", "$9.99"),
        ];
        for (row, (a, b, c)) in rows.iter().enumerate() {
            let y = 100.0 + row as f32 * 16.0;
            fragments.push(fragment(a, 72.0, y, 10.0, row == 0));
            fragments.push(fragment(b, 200.0, y, 10.0, row == 0));
            fragments.push(fragment(c, 300.0, y, 10.0, row == 0));
        }
        fragments.push(fragment(
            "A sentence after the table.",
            72.0,
            200.0,
            10.0,
            false,
        ));
        let taken = vec![false; fragments.len()];
        let tables = whitespace_tables(&fragments, &taken);
        assert_eq!(tables.len(), 1);
        let table = &tables[0].table;
        assert_eq!((table.rows, table.columns), (4, 3));
        assert_eq!(table.cell(2, 2).unwrap().text(), "$5.50");
        assert_eq!(table.header_rows, 1);
        assert!(!table.ruled);
        assert_eq!(tables[0].used.len(), 12);
    }

    #[test]
    fn two_columns_of_prose_are_not_a_table() {
        let mut fragments = Vec::new();
        for row in 0..6 {
            let y = 100.0 + row as f32 * 14.0;
            fragments.push(fragment(
                "the quick brown fox jumps over the dog",
                72.0,
                y,
                10.0,
                false,
            ));
            fragments.push(fragment(
                "and then the lazy dog sleeps all day",
                320.0,
                y,
                10.0,
                false,
            ));
        }
        let taken = vec![false; fragments.len()];
        assert!(whitespace_tables(&fragments, &taken).is_empty());
    }

    #[test]
    fn numbers_are_recognised() {
        assert!(looks_numeric("$1,234.50"));
        assert!(looks_numeric("(12)"));
        assert!(looks_numeric("45%"));
        assert!(!looks_numeric("Total"));
        assert!(!looks_numeric(""));
    }
}
