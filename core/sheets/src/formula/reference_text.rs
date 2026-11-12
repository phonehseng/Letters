//! references written as text: sheet names quoted when they must be,
//! a1 and r1c1 areas, openformula's bracketed form and table references.

use crate::address::{column_name, CellRef};

use super::ast::*;

/// a sheet name, quoted when excel needs quotes.
pub fn quote_sheet(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !looks_like_reference(name)
        && !name.eq_ignore_ascii_case("TRUE")
        && !name.eq_ignore_ascii_case("FALSE");
    if plain {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// "A1", "XFD3" or "R1C1" style names must be quoted
fn looks_like_reference(name: &str) -> bool {
    let letters: String = name
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let rest = &name[letters.len()..];
    let a1 = !letters.is_empty()
        && crate::address::column_index(&letters).is_some()
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit());
    let upper = name.to_ascii_uppercase();
    let r1c1 = upper.starts_with('R') || upper.starts_with('C');
    let r1c1 = r1c1
        && upper
            .chars()
            .skip(1)
            .all(|c| c.is_ascii_digit() || c == 'C');
    a1 || r1c1
}

pub fn sheet_text(sheet: &SheetRef) -> String {
    match sheet {
        SheetRef::One { name } => quote_sheet(name),
        SheetRef::Span { first, last } => {
            let joined = format!("{first}:{last}");
            if quote_sheet(first) == *first && quote_sheet(last) == *last {
                joined
            } else {
                format!("'{}'", joined.replace('\'', "''"))
            }
        }
        SheetRef::External { workbook, name } => {
            let inner = format!("[{workbook}]{name}");
            if quote_sheet(name) == *name {
                inner
            } else {
                format!("'{}'", inner.replace('\'', "''"))
            }
        }
    }
}

fn corner(at: &CellPos) -> String {
    format!(
        "{}{}{}{}",
        if at.col_abs { "$" } else { "" },
        column_name(at.col),
        if at.row_abs { "$" } else { "" },
        at.row + 1
    )
}

pub fn area_text(area: &Area) -> String {
    match area {
        Area::Cell { at } => corner(at),
        Area::Range { start, end } => format!("{}:{}", corner(start), corner(end)),
        Area::Columns {
            first,
            last,
            first_abs,
            last_abs,
        } => format!(
            "{}{}:{}{}",
            if *first_abs { "$" } else { "" },
            column_name(*first),
            if *last_abs { "$" } else { "" },
            column_name(*last)
        ),
        Area::Rows {
            first,
            last,
            first_abs,
            last_abs,
        } => format!(
            "{}{}:{}{}",
            if *first_abs { "$" } else { "" },
            first + 1,
            if *last_abs { "$" } else { "" },
            last + 1
        ),
    }
}

fn r1c1_part(letter: char, value: u32, absolute: bool, base: u32) -> String {
    if absolute {
        format!("{letter}{}", value + 1)
    } else {
        let offset = i64::from(value) - i64::from(base);
        if offset == 0 {
            letter.to_string()
        } else {
            format!("{letter}[{offset}]")
        }
    }
}

pub(super) fn r1c1_area(area: &Area, at: CellRef) -> String {
    let cell = |p: &CellPos| {
        format!(
            "{}{}",
            r1c1_part('R', p.row, p.row_abs, at.row),
            r1c1_part('C', p.col, p.col_abs, at.col)
        )
    };
    match area {
        Area::Cell { at: p } => cell(p),
        Area::Range { start, end } => format!("{}:{}", cell(start), cell(end)),
        Area::Columns {
            first,
            last,
            first_abs,
            last_abs,
        } => format!(
            "{}:{}",
            r1c1_part('C', *first, *first_abs, at.col),
            r1c1_part('C', *last, *last_abs, at.col)
        ),
        Area::Rows {
            first,
            last,
            first_abs,
            last_abs,
        } => format!(
            "{}:{}",
            r1c1_part('R', *first, *first_abs, at.row),
            r1c1_part('R', *last, *last_abs, at.row)
        ),
    }
}

fn open_sheet(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty();
    if plain {
        format!("${name}")
    } else {
        format!("$'{}'", name.replace('\'', "''"))
    }
}

pub(super) fn open_reference(reference: &Reference) -> String {
    let (first_sheet, last_sheet) = match &reference.sheet {
        None => (String::new(), String::new()),
        Some(SheetRef::One { name }) => (open_sheet(name), String::new()),
        Some(SheetRef::Span { first, last }) => (open_sheet(first), open_sheet(last)),
        Some(SheetRef::External { workbook, name }) => {
            (format!("'{workbook}'#{}", open_sheet(name)), String::new())
        }
    };
    let text = area_text(&reference.area);
    match text.split_once(':') {
        None => format!("[{first_sheet}.{text}]"),
        Some((a, b)) => format!("[{first_sheet}.{a}:{last_sheet}.{b}]"),
    }
}

fn escape_column(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

pub fn structured_text(reference: &StructuredRef) -> String {
    let table = reference.table.clone().unwrap_or_default();
    let item_text = |item: &TableItem| match item {
        TableItem::All => "#All",
        TableItem::Data => "#Data",
        TableItem::Headers => "#Headers",
        TableItem::Totals => "#Totals",
        TableItem::ThisRow => "#This Row",
    };
    let this_row_only = reference.items == [TableItem::ThisRow];
    let simple_column = |name: &str| !name.chars().any(|c| " []#'@,:".contains(c));
    let inside = match (&reference.first_column, &reference.last_column) {
        (None, None) if reference.items.len() == 1 && !this_row_only => {
            item_text(&reference.items[0]).to_string()
        }
        (None, None) if this_row_only => "@".to_string(),
        (Some(column), None) if reference.items.is_empty() && simple_column(column) => {
            escape_column(column)
        }
        (Some(column), None) if this_row_only => {
            if simple_column(column) {
                format!("@{}", escape_column(column))
            } else {
                format!("@[{}]", escape_column(column))
            }
        }
        _ => {
            let mut parts: Vec<String> = reference
                .items
                .iter()
                .map(|item| format!("[{}]", item_text(item)))
                .collect();
            match (&reference.first_column, &reference.last_column) {
                (Some(a), Some(b)) => {
                    parts.push(format!("[{}]:[{}]", escape_column(a), escape_column(b)))
                }
                (Some(a), None) => parts.push(format!("[{}]", escape_column(a))),
                _ => {}
            }
            parts.join(",")
        }
    };
    format!("{table}[{inside}]")
}
