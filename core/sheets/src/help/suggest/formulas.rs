//! tips about the selected formula: exact lookups, naming a repeated
//! number, bounded ranges in slow formulas, and IFS for nested IFs.

use super::*;

pub(super) fn selected_formula(place: &Place) -> Option<(CellRef, String, Expr)> {
    let at = place.selection.start;
    let text = cell(place, at.row, at.col)?.formula.as_ref()?.text.clone();
    let expr = formula::parse(&text).ok()?;
    Some((at, text, expr))
}

pub(super) fn exact_lookup(place: &Place) -> Vec<Suggestion> {
    let Some((at, _, expr)) = selected_formula(place) else {
        return Vec::new();
    };
    let mut unsorted = false;
    let rewritten = expr.transform(&mut |node| match node {
        Expr::Function { name, mut args }
            if (name == "VLOOKUP" || name == "HLOOKUP") && args.len() == 3 =>
        {
            if let Expr::Ref { reference } = &args[1] {
                if reference.sheet.is_none()
                    && !first_line_sorted(place, reference.area.to_range(), name == "HLOOKUP")
                {
                    unsorted = true;
                    args.push(Expr::Bool { value: false });
                }
            }
            Expr::Function { name, args }
        }
        other => other,
    });
    if !unsorted {
        return Vec::new();
    }
    let text = formula::to_string(&rewritten);
    vec![Suggestion {
        id: id("exact-lookup", place, at),
        title: "Use an exact match".into(),
        why: "Without FALSE the lookup assumes its first column is sorted, and it isn't: it can give a wrong answer with no error.".into(),
        preview: format!("={text}"),
        operations: vec![Operation::SetCell { sheet: place.sheet, row: at.row, col: at.col, input: format!("={text}") }],
    }]
}

pub(super) fn first_line_sorted(place: &Place, range: Range, horizontal: bool) -> bool {
    let used = place.book.sheets[place.sheet]
        .cells
        .used_range()
        .unwrap_or(range);
    let values: Vec<&Value> = if horizontal {
        (range.start.col..=range.end.col.min(used.end.col))
            .map(|c| value(place, range.start.row, c))
            .collect()
    } else {
        (range.start.row..=range.end.row.min(used.end.row))
            .map(|r| value(place, r, range.start.col))
            .collect()
    };
    let present: Vec<&Value> = values.into_iter().filter(|v| !v.is_blank()).collect();
    present
        .windows(2)
        .all(|w| crate::calc::value::compare(w[0], w[1]) != std::cmp::Ordering::Greater)
}

/// numbers too common to be worth a name
pub(super) const PLAIN_NUMBERS: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 10.0, 12.0, 24.0, 100.0];

pub(super) fn name_value(place: &Place) -> Vec<Suggestion> {
    let Some((_, _, expr)) = selected_formula(place) else {
        return Vec::new();
    };
    let mut numbers = Vec::new();
    expr.walk(&mut |node| {
        if let Expr::Number { value } = node {
            if !PLAIN_NUMBERS.contains(value) {
                numbers.push(*value);
            }
        }
    });
    for number in numbers {
        let uses = place
            .book
            .sheets
            .iter()
            .flat_map(|s| s.cells.iter())
            .filter(|(_, _, c)| {
                c.formula
                    .as_ref()
                    .and_then(|f| formula::parse(&f.text).ok())
                    .is_some_and(|e| {
                        let mut found = false;
                        e.walk(&mut |n| {
                            found |= matches!(n, Expr::Number { value } if *value == number)
                        });
                        found
                    })
            });
        if uses.count() >= 3 {
            let mut names = place.book.defined_names.clone();
            let name = (1..)
                .map(|n| format!("Value{n}"))
                .find(|n| place.book.find_name(n, None).is_none())
                .unwrap_or_default();
            names.push(crate::model::DefinedName {
                name: name.clone(),
                formula: crate::calc::value::general_text(number),
                ..Default::default()
            });
            return vec![Suggestion {
                id: format!("name-value:{}", crate::calc::value::general_text(number)),
                title: format!("Give {} a name", crate::calc::value::general_text(number)),
                why: "The same number is written into several formulas; a name (like TaxRate) changes them all at once and says what it means.".into(),
                preview: format!("{name} = {}", crate::calc::value::general_text(number)),
                operations: vec![Operation::RestoreNames { names }],
            }];
        }
    }
    Vec::new()
}

/// rows above which reading whole columns in a volatile formula is slow
pub(super) const LARGE_SHEET_ROWS: u32 = 10_000;

pub(super) fn bounded_ranges(place: &Place) -> Vec<Suggestion> {
    let Some((at, _, expr)) = selected_formula(place) else {
        return Vec::new();
    };
    let used_rows = place.book.sheets[place.sheet]
        .cells
        .used_range()
        .map(|r| r.end.row + 1)
        .unwrap_or(0);
    let volatile = expr
        .function_names()
        .iter()
        .any(|n| crate::calc::precedents::VOLATILE.contains(&n.as_str()));
    let mut whole = false;
    let rewritten = expr.transform(&mut |node| match node {
        Expr::Ref { mut reference } => {
            if let formula::Area::Columns { first, last, .. } = reference.area {
                whole = true;
                let corner = |row, col| formula::CellPos {
                    row,
                    col,
                    row_abs: true,
                    col_abs: true,
                };
                reference.area = formula::Area::Range {
                    start: corner(0, first),
                    end: corner(used_rows.saturating_sub(1), last),
                };
            }
            Expr::Ref { reference }
        }
        other => other,
    });
    if !volatile || !whole || used_rows < LARGE_SHEET_ROWS {
        return Vec::new();
    }
    let text = formula::to_string(&rewritten);
    vec![Suggestion {
        id: id("bounded-ranges", place, at),
        title: "Speed up this formula".into(),
        why: "It recalculates after every change and reads whole columns of a large sheet; reading only the rows with data is much faster.".into(),
        preview: format!("={text}"),
        operations: vec![Operation::SetCell { sheet: place.sheet, row: at.row, col: at.col, input: format!("={text}") }],
    }]
}

/// IF(a,x,IF(b,y,IF(c,z,w))) as the pairs of IFS and the final value
pub(super) fn flatten_ifs(expr: &Expr, pairs: &mut Vec<(Expr, Expr)>) -> Option<Expr> {
    match expr {
        Expr::Function { name, args } if name == "IF" && args.len() == 3 => {
            pairs.push((args[0].clone(), args[1].clone()));
            flatten_ifs(&args[2], pairs)
        }
        other => Some(other.clone()),
    }
}

pub(super) fn use_ifs(place: &Place) -> Vec<Suggestion> {
    let Some((at, _, expr)) = selected_formula(place) else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    let Some(last) = flatten_ifs(&expr, &mut pairs) else {
        return Vec::new();
    };
    if pairs.len() < 3 {
        return Vec::new();
    }
    let mut args: Vec<Expr> = pairs.into_iter().flat_map(|(c, v)| [c, v]).collect();
    args.push(Expr::Bool { value: true });
    args.push(last);
    let text = formula::to_string(&Expr::Function {
        name: "IFS".into(),
        args,
    });
    vec![Suggestion {
        id: id("use-ifs", place, at),
        title: "Make it easier to read with IFS".into(),
        why: "IFS lists each test and its answer in order, without IFs inside IFs.".into(),
        preview: format!("={text}"),
        operations: vec![Operation::SetCell {
            sheet: place.sheet,
            row: at.row,
            col: at.col,
            input: format!("={text}"),
        }],
    }]
}
