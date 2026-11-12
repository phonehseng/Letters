//! tips about a block of data: freezing its header, making it a table,
//! sorting, removing repeated rows and summarising it by category.

use super::*;

/// the block of data around the selection: the rectangle of filled cells
/// reached by moving from the selection's corner while cells continue
pub(super) fn data_block(place: &Place) -> Option<Range> {
    let start = place.selection.start;
    let filled = |r: u32, c: u32| !value(place, r, c).is_blank();
    if !filled(start.row, start.col) {
        return None;
    }
    let mut top = start.row;
    while top > 0 && filled(top - 1, start.col) {
        top -= 1;
    }
    let mut left = start.col;
    while left > 0 && filled(top, left - 1) {
        left -= 1;
    }
    let mut right = start.col;
    while filled(top, right + 1) {
        right += 1;
    }
    let mut bottom = start.row;
    while (left..=right).any(|c| filled(bottom + 1, c)) {
        bottom += 1;
    }
    Some(Range::from_bounds(top, left, bottom, right))
}

/// the block's first row is all text and the row below holds a number
pub(super) fn has_header(place: &Place, block: Range) -> bool {
    let header_is_text = (block.start.col..=block.end.col)
        .all(|c| matches!(value(place, block.start.row, c), Value::Text(_)));
    let next = block.start.row + 1;
    let data_differs = (block.start.col..=block.end.col)
        .any(|c| !matches!(value(place, next, c), Value::Text(_) | Value::Blank));
    header_is_text && block.rows() >= 2 && data_differs
}

pub(super) fn freeze_header(place: &Place) -> Vec<Suggestion> {
    let sheet = &place.book.sheets[place.sheet];
    let Some(block) = data_block(place) else {
        return Vec::new();
    };
    if sheet.view.frozen.is_some()
        || block.rows() < 21
        || !has_header(place, block)
        || block.start.row > 2
    {
        return Vec::new();
    }
    vec![Suggestion {
        id: id("freeze-header", place, block.start),
        title: "Freeze the header row".into(),
        why: "The headings stay in view while you scroll through the data.".into(),
        preview: format!("rows 1 to {} stay at the top", block.start.row + 1),
        operations: vec![Operation::Freeze {
            sheet: place.sheet,
            rows: block.start.row + 1,
            cols: 0,
        }],
    }]
}

pub(super) fn make_table(place: &Place) -> Vec<Suggestion> {
    let sheet = &place.book.sheets[place.sheet];
    let Some(block) = data_block(place) else {
        return Vec::new();
    };
    let in_table = sheet.tables.iter().any(|t| t.range.intersects(&block));
    if in_table
        || block.rows() < 4
        || block.cols() < 2
        || !has_header(place, block)
        || sheet.merged.iter().any(|m| m.intersects(&block))
    {
        return Vec::new();
    }
    vec![Suggestion {
        id: id("make-table", place, block.start),
        title: "Turn into a table".into(),
        why: "Tables add filter buttons and banded rows, grow as you add rows, and let formulas say Table1[Price] (Ctrl+T).".into(),
        preview: block.to_a1(),
        operations: vec![Operation::ToTable { sheet: place.sheet, range: block, has_header: true }],
    }]
}

pub(super) fn sort_column(place: &Place) -> Vec<Suggestion> {
    let Some(block) = data_block(place) else {
        return Vec::new();
    };
    let at = place.selection.start;
    if place.selection.cell_count() != 1
        || at.row != block.start.row
        || block.rows() < 3
        || !has_header(place, block)
    {
        return Vec::new();
    }
    let column = at.col - block.start.col;
    let name = text_of(place, at.row, at.col).unwrap_or_default();
    vec![Suggestion {
        id: id("sort", place, at),
        title: format!("Sort by {name}"),
        why: "Sorts the rows below the header by this column, keeping each row together.".into(),
        preview: "A to Z".into(),
        operations: vec![Operation::Sort {
            sheet: place.sheet,
            range: block,
            keys: vec![SortKey {
                column,
                descending: false,
            }],
            has_header: true,
        }],
    }]
}

pub(super) fn row_key(place: &Place, row: u32, block: Range) -> String {
    (block.start.col..=block.end.col)
        .map(|c| format!("{:?}", value(place, row, c)))
        .collect::<Vec<_>>()
        .join("\u{1}")
        .to_lowercase()
}

pub(super) fn remove_duplicates(place: &Place) -> Vec<Suggestion> {
    let Some(block) = data_block(place) else {
        return Vec::new();
    };
    let header = has_header(place, block);
    let first = block.start.row + u32::from(header);
    let mut seen = std::collections::HashSet::new();
    let repeats = (first..=block.end.row)
        .filter(|r| !seen.insert(row_key(place, *r, block)))
        .count();
    if repeats == 0 || block.rows() > 100_000 {
        return Vec::new();
    }
    vec![Suggestion {
        id: id("remove-duplicates", place, block.start),
        title: "Remove duplicate rows".into(),
        why: format!(
            "{repeats} row{} repeat{} an earlier row exactly.",
            if repeats == 1 { "" } else { "s" },
            if repeats == 1 { "s" } else { "" }
        ),
        preview: format!(
            "{} rows left",
            block.rows() - u32::from(header) - repeats as u32
        ),
        operations: vec![Operation::RemoveDuplicates {
            sheet: place.sheet,
            range: block,
            columns: Vec::new(),
            has_header: header,
        }],
    }]
}

pub(super) fn summarise(place: &Place) -> Vec<Suggestion> {
    let Some(block) = data_block(place) else {
        return Vec::new();
    };
    if !has_header(place, block) || block.rows() < 6 {
        return Vec::new();
    }
    let first = block.start.row + 1;
    let column_kind = |col: u32, number: bool| {
        (first..=block.end.row).all(|r| {
            matches!(value(place, r, col), Value::Number(_)) == number
                && !value(place, r, col).is_blank()
        })
    };
    let category = (block.start.col..=block.end.col).find(|c| column_kind(*c, false));
    let amount = (block.start.col..=block.end.col).find(|c| column_kind(*c, true));
    let (Some(category), Some(amount)) = (category, amount) else {
        return Vec::new();
    };
    let mut totals: Vec<(String, f64)> = Vec::new();
    for row in first..=block.end.row {
        let key = text_of(place, row, category).unwrap_or_default();
        let n = value(place, row, amount).as_number().unwrap_or(0.0);
        match totals
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(&key))
        {
            Some((_, total)) => *total += n,
            None => totals.push((key, n)),
        }
    }
    if totals.len() < 2 || totals.len() * 2 > (block.rows() - 1) as usize {
        return Vec::new();
    }
    let name = place.book.unused_sheet_name("Summary");
    let index = place.book.sheets.len();
    let header = |c| text_of(place, block.start.row, c).unwrap_or_default();
    let mut cells = vec![
        (0, 0, Some(Cell::value(Value::text(header(category))))),
        (
            0,
            1,
            Some(Cell::value(Value::text(format!(
                "Total {}",
                header(amount)
            )))),
        ),
    ];
    for (i, (key, total)) in totals.iter().enumerate() {
        cells.push((i as u32 + 1, 0, Some(Cell::value(Value::text(key.clone())))));
        cells.push((i as u32 + 1, 1, Some(Cell::value(Value::Number(*total)))));
    }
    vec![Suggestion {
        id: id("summarise", place, block.start),
        title: format!("Summarise {} by {}", header(amount), header(category)),
        why: "Adds up the numbers for each category on a new sheet, like a simple pivot table."
            .into(),
        preview: totals
            .iter()
            .take(3)
            .map(|(k, t)| format!("{k}: {}", crate::calc::value::general_text(*t)))
            .collect::<Vec<_>>()
            .join(", "),
        operations: vec![
            Operation::AddSheet { index, name },
            Operation::PutCells {
                sheet: index,
                cells,
            },
        ],
    }]
}
