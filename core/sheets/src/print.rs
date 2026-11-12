//! the print layout of a sheet: which rows and columns go on each page,
//! with excel's paper sizes, margins, orientation, scaling (a percentage
//! or "fit to w pages wide by h tall"), manual page breaks and rows or
//! columns repeated on every page. a renderer draws the pages from this;
//! the app can also print exactly through excel or libreoffice.

use serde::Serialize;

use crate::address::Range;
use crate::model::{Orientation, Sheet};

/// excel's paper size numbers: (number, width, height) in points
const PAPERS: [(u32, f64, f64); 6] = [
    (1, 612.0, 792.0),  // letter
    (5, 612.0, 1008.0), // legal
    (8, 841.9, 1190.6), // a3
    (9, 595.3, 841.9),  // a4
    (11, 419.5, 595.3), // a5
    (3, 792.0, 1224.0), // tabloid
];

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub number: u32,
    /// rows and columns of the sheet on this page (both ends included)
    pub rows: (u32, u32),
    pub cols: (u32, u32),
    /// title rows and columns printed again at the top and left
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_rows: Option<(u32, u32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_cols: Option<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLayout {
    /// page size in points, after orientation
    pub page_width: f64,
    pub page_height: f64,
    /// margins in points: left, right, top, bottom
    pub margins: (f64, f64, f64, f64),
    /// the scale every page is drawn at, 1.0 being 100%
    pub scale: f64,
    pub area: Range,
    pub pages: Vec<Page>,
}

/// a column's width in points: excel's w * 7 + 5 pixels at 96 per inch
pub fn column_points(sheet: &Sheet, col: u32) -> f64 {
    if sheet.column_hidden(col) {
        return 0.0;
    }
    (sheet.column_width(col) * 7.0 + 5.0).trunc() * 0.75
}

fn row_points(sheet: &Sheet, row: u32) -> f64 {
    if sheet.row_hidden(row) {
        0.0
    } else {
        sheet.row_height(row)
    }
}

pub fn layout(sheet: &Sheet) -> PrintLayout {
    let print = &sheet.print;
    let (mut width, mut height) = PAPERS
        .iter()
        .find(|(number, _, _)| Some(*number) == print.paper_size)
        .map(|(_, w, h)| (*w, *h))
        .unwrap_or((612.0, 792.0));
    if print.orientation == Orientation::Landscape {
        std::mem::swap(&mut width, &mut height);
    }
    let margins = print.margins.unwrap_or_default();
    let inch = 72.0;
    let edges = (
        margins.left * inch,
        margins.right * inch,
        margins.top * inch,
        margins.bottom * inch,
    );
    let usable_width = width - edges.0 - edges.1;
    let usable_height = height - edges.2 - edges.3;
    // without a print area excel prints from A1 to the last used cell
    let used_from_a1 = sheet
        .cells
        .used_range()
        .map(|used| Range::from_bounds(0, 0, used.end.row, used.end.col));
    let area = print
        .area
        .first()
        .copied()
        .or(used_from_a1)
        .unwrap_or_default();
    let scale = scale_for(sheet, area, usable_width, usable_height);
    let col_breaks = breaks(
        area.start.col,
        area.end.col,
        &print.col_breaks,
        usable_width / scale,
        &|c| column_points(sheet, c),
        print.title_cols,
        &|c| column_points(sheet, c),
    );
    let row_breaks = breaks(
        area.start.row,
        area.end.row,
        &print.row_breaks,
        usable_height / scale,
        &|r| row_points(sheet, r),
        print.title_rows,
        &|r| row_points(sheet, r),
    );
    let mut pages = Vec::new();
    // excel's default order: down, then over
    for cols in &col_breaks {
        for rows in &row_breaks {
            pages.push(Page {
                number: pages.len() as u32 + 1,
                rows: *rows,
                cols: *cols,
                repeat_rows: print.title_rows.filter(|(_, last)| *last < rows.0),
                repeat_cols: print.title_cols.filter(|(_, last)| *last < cols.0),
            });
        }
    }
    PrintLayout {
        page_width: width,
        page_height: height,
        margins: edges,
        scale,
        area,
        pages,
    }
}

/// a set percentage, or the largest scale (at most 100%, at least 10%,
/// excel's limits) that fits the area in the pages asked for
fn scale_for(sheet: &Sheet, area: Range, usable_width: f64, usable_height: f64) -> f64 {
    if let Some((wide, tall)) = sheet.print.fit_to {
        let total_width: f64 = (area.start.col..=area.end.col)
            .map(|c| column_points(sheet, c))
            .sum();
        let total_height: f64 = (area.start.row..=area.end.row)
            .map(|r| row_points(sheet, r))
            .sum();
        let by_width = if wide > 0 {
            usable_width * f64::from(wide) / total_width.max(1.0)
        } else {
            1.0
        };
        let by_height = if tall > 0 {
            usable_height * f64::from(tall) / total_height.max(1.0)
        } else {
            1.0
        };
        return by_width.min(by_height).clamp(0.1, 1.0);
    }
    f64::from(sheet.print.scale.unwrap_or(100)).clamp(10.0, 400.0) / 100.0
}

/// splits first..=last into runs that fit `room`, starting a new run at
/// every manual break; repeated titles take room on every page after the first
#[allow(clippy::too_many_arguments)]
fn breaks(
    first: u32,
    last: u32,
    manual: &[u32],
    room: f64,
    size: &dyn Fn(u32) -> f64,
    titles: Option<(u32, u32)>,
    title_size: &dyn Fn(u32) -> f64,
) -> Vec<(u32, u32)> {
    let title_room: f64 = titles
        .map(|(a, b)| (a..=b).map(title_size).sum())
        .unwrap_or(0.0);
    let mut runs = Vec::new();
    let mut start = first;
    let mut used = 0.0;
    for index in first..=last {
        let room_here = if runs.is_empty() {
            room
        } else {
            room - title_room
        };
        let item = size(index);
        let manual_break = manual.contains(&index) && index != start;
        if index != start && (used + item > room_here || manual_break) {
            runs.push((start, index - 1));
            start = index;
            used = 0.0;
        }
        used += item;
    }
    runs.push((start, last));
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cell, Value};

    fn sheet_with(rows: u32, cols: u32) -> Sheet {
        let mut sheet = Sheet::new("S");
        sheet
            .cells
            .set(rows - 1, cols - 1, Cell::value(Value::Number(1.0)));
        sheet
    }

    #[test]
    fn pages_follow_size_breaks_and_scaling() {
        let sheet = sheet_with(100, 5);
        let pages = layout(&sheet).pages;
        // 15 point rows on letter paper with excel's normal margins: 45 per page
        assert_eq!(pages[0].rows, (0, 44));
        assert_eq!(pages.len(), 3);
        let mut broken = sheet_with(100, 5);
        broken.print.row_breaks = vec![10];
        assert_eq!(layout(&broken).pages[0].rows, (0, 9));
        let mut fitted = sheet_with(100, 30);
        fitted.print.fit_to = Some((1, 0));
        let fitted_layout = layout(&fitted);
        assert!(fitted_layout.scale < 1.0);
        assert!(fitted_layout.pages.iter().all(|p| p.cols == (0, 29)));
        let mut titled = sheet_with(100, 5);
        titled.print.title_rows = Some((0, 0));
        let titled_pages = layout(&titled).pages;
        assert_eq!(titled_pages[1].repeat_rows, Some((0, 0)));
        assert_eq!(
            titled_pages[1].rows.1 - titled_pages[1].rows.0,
            43,
            "one row fewer to leave room for the title"
        );
    }
}
