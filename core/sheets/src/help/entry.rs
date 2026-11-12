//! helpers for typing formulas: which argument the cursor is in, putting
//! a clicked range into the formula, closing brackets and quotes, a
//! colour for each reference (as the grid outlines them), F4's cycling of
//! dollar signs, and turning references into table references and back.

use serde::Serialize;

use super::catalogue::{function_info, FunctionInfo};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgumentHint {
    pub function: FunctionInfo,
    /// which argument the cursor is in, 0 for the first
    pub argument: usize,
}

/// the innermost function call around `caret` (a character offset) and
/// the argument the cursor is in
pub fn argument_hint(formula: &str, caret: usize) -> Option<ArgumentHint> {
    let chars: Vec<char> = formula.chars().take(caret).collect();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut name = String::new();
    let mut quoted = false;
    let mut braces = 0;
    for c in chars {
        if quoted {
            quoted = c != '"';
            continue;
        }
        match c {
            '"' => quoted = true,
            '{' => braces += 1,
            '}' => braces -= 1,
            '(' => {
                stack.push((std::mem::take(&mut name).to_ascii_uppercase(), 0));
            }
            ')' => {
                stack.pop();
            }
            ',' if braces == 0 => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            c if c.is_alphanumeric() || c == '.' || c == '_' => name.push(c),
            _ => name.clear(),
        }
    }
    let (function, argument) = stack.into_iter().rev().find(|(name, _)| !name.is_empty())?;
    let function = function_info(&function)?;
    let last = function.arguments.len().saturating_sub(1);
    let repeats = function.arguments.last().is_some_and(|a| a.repeating);
    // a repeating last argument takes every position from there on
    let argument = if repeats {
        argument
    } else {
        argument.min(last)
    };
    Some(ArgumentHint { function, argument })
}

/// a reference's place in formula text (byte offsets)
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSpan {
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// the same reference written twice gets the same colour
    pub colour: usize,
}

/// excel's colours for references in the formula bar, in order
pub const REFERENCE_COLOURS: [&str; 8] = [
    "#0000FF", "#FF0000", "#7030A0", "#00B050", "#C65911", "#2F75B5", "#BF8F00", "#FF00FF",
];

fn cell_length(bytes: &[u8], at: usize) -> usize {
    let mut i = at;
    if bytes.get(i) == Some(&b'$') {
        i += 1;
    }
    let letters = bytes[i.min(bytes.len())..]
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    if letters == 0 || letters > 3 {
        return 0;
    }
    i += letters;
    if bytes.get(i) == Some(&b'$') {
        i += 1;
    }
    let digits = bytes[i.min(bytes.len())..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return 0;
    }
    i += digits;
    let next = bytes.get(i);
    if next.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'(') {
        return 0;
    }
    i - at
}

/// a sheet prefix ("Sheet2!" or "'My sheet'!") at `at`, by its length
fn sheet_prefix_length(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    if let Some(quoted) = rest.strip_prefix('\'') {
        if let Some(end) = quoted.find("'!") {
            return end + 3;
        }
        return 0;
    }
    let name = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .map(char::len_utf8)
        .sum::<usize>();
    if name > 0 && rest[name..].starts_with('!') {
        name + 1
    } else {
        0
    }
}

/// every reference in a formula, with its colour
pub fn reference_spans(formula: &str) -> Vec<ReferenceSpan> {
    let bytes = formula.as_bytes();
    let mut spans: Vec<ReferenceSpan> = Vec::new();
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        let b = bytes[i];
        if quoted {
            quoted = b != b'"';
            i += 1;
            continue;
        }
        if b == b'"' {
            quoted = true;
            i += 1;
            continue;
        }
        let starts_token = i == 0
            || !(bytes[i - 1].is_ascii_alphanumeric()
                || bytes[i - 1] == b'_'
                || bytes[i - 1] == b'.');
        if !starts_token || !formula.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let prefix = sheet_prefix_length(formula, i);
        let first = cell_length(bytes, i + prefix);
        if first == 0 {
            i += 1;
            continue;
        }
        let mut end = i + prefix + first;
        if bytes.get(end) == Some(&b':') {
            let second = cell_length(bytes, end + 1);
            if second > 0 {
                end += 1 + second;
            }
        }
        let text = formula[i..end].to_string();
        let colour = spans
            .iter()
            .find(|s| s.text.eq_ignore_ascii_case(&text))
            .map(|s| s.colour)
            .unwrap_or_else(|| {
                spans.iter().map(|s| s.colour + 1).max().unwrap_or(0) % REFERENCE_COLOURS.len()
            });
        spans.push(ReferenceSpan {
            start: i,
            end,
            text,
            colour,
        });
        i = end;
    }
    spans
}

/// a clicked range put into the formula at the cursor: it replaces the
/// reference the cursor is on or just after (clicking again moves it),
/// or goes in where a value can start. returns the new text and cursor
pub fn insert_reference(formula: &str, caret: usize, reference: &str) -> (String, usize) {
    let caret = caret.min(formula.len());
    if let Some(span) = reference_spans(formula)
        .into_iter()
        .find(|s| s.start <= caret && caret <= s.end)
    {
        let text = format!(
            "{}{}{}",
            &formula[..span.start],
            reference,
            &formula[span.end..]
        );
        return (text, span.start + reference.len());
    }
    let text = format!("{}{}{}", &formula[..caret], reference, &formula[caret..]);
    (text, caret + reference.len())
}

/// true when a reference could be typed or clicked at the cursor: right
/// after "=", an operator, "(" or ","
pub fn expects_reference(formula: &str, caret: usize) -> bool {
    let before = formula[..caret.min(formula.len())].trim_end();
    before.ends_with([
        '=', '(', ',', '+', '-', '*', '/', '^', '&', '<', '>', ':', ' ',
    ]) || before.is_empty()
}

/// closes quotes and brackets left open, as excel does when you press enter
pub fn auto_close(formula: &str) -> String {
    let mut out = formula.to_string();
    let mut quoted = false;
    let mut open: Vec<char> = Vec::new();
    for c in formula.chars() {
        if quoted {
            quoted = c != '"';
            continue;
        }
        match c {
            '"' => quoted = true,
            '(' => open.push(')'),
            '{' => open.push('}'),
            ')' | '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    if quoted {
        out.push('"');
    }
    while let Some(close) = open.pop() {
        out.push(close);
    }
    out
}

/// F4: the reference at the cursor cycles A1 -> $A$1 -> A$1 -> $A1 -> A1,
/// excel's order. returns the new text and cursor
pub fn cycle_reference(formula: &str, caret: usize) -> (String, usize) {
    let Some(span) = reference_spans(formula)
        .into_iter()
        .find(|s| s.start <= caret && caret <= s.end)
    else {
        return (formula.to_string(), caret);
    };
    let (prefix, body) = match span.text.rfind('!') {
        Some(at) => (&span.text[..=at], &span.text[at + 1..]),
        None => ("", span.text.as_str()),
    };
    let cycled: Vec<String> = body.split(':').map(next_dollars).collect();
    let replacement = format!("{prefix}{}", cycled.join(":"));
    let text = format!(
        "{}{}{}",
        &formula[..span.start],
        replacement,
        &formula[span.end..]
    );
    (text, span.start + replacement.len())
}

fn next_dollars(cell: &str) -> String {
    let col_abs = cell.starts_with('$');
    let plain = cell.replace('$', "");
    let letters: String = plain
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let digits = &plain[letters.len()..];
    let row_abs = cell[usize::from(col_abs)..].contains('$');
    match (col_abs, row_abs) {
        (false, false) => format!("${letters}${digits}"),
        (true, true) => format!("{letters}${digits}"),
        (false, true) => format!("${letters}{digits}"),
        (true, false) => format!("{letters}{digits}"),
    }
}

/// references that cover a table column become table references
/// ("SUM(B2:B10)" -> "SUM(Sales[Price])"); others are left alone
pub fn to_structured(book: &crate::model::Workbook, sheet: usize, formula: &str) -> String {
    let Ok(expr) = crate::formula::parse(formula) else {
        return formula.to_string();
    };
    let changed = expr.transform(&mut |node| {
        let crate::formula::Expr::Ref { reference } = &node else {
            return node;
        };
        let target = match &reference.sheet {
            None => Some(sheet),
            Some(crate::formula::SheetRef::One { name }) => book.sheet_index(name),
            _ => None,
        };
        let range = reference.area.to_range();
        let column = target.and_then(|s| table_column_for(book, s, range));
        match column {
            Some((table, column)) => crate::formula::Expr::Structured {
                reference: Box::new(crate::formula::StructuredRef {
                    table: Some(table),
                    items: Vec::new(),
                    first_column: Some(column),
                    last_column: None,
                }),
            },
            None => node,
        }
    });
    crate::formula::to_string(&changed)
}

fn table_column_for(
    book: &crate::model::Workbook,
    sheet: usize,
    range: crate::address::Range,
) -> Option<(String, String)> {
    let table = book
        .sheets
        .get(sheet)?
        .tables
        .iter()
        .find(|t| t.data_range().is_some_and(|d| d.contains_range(&range)))?;
    let data = table.data_range()?;
    let whole_column =
        range.cols() == 1 && range.start.row == data.start.row && range.end.row == data.end.row;
    whole_column.then(|| {
        (
            table.name.clone(),
            table.columns[(range.start.col - table.range.start.col) as usize]
                .name
                .clone(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_follow_the_cursor() {
        let hint = argument_hint("=VLOOKUP(A2,Prices!A:C,", 23).unwrap();
        assert_eq!(hint.function.name, "VLOOKUP");
        assert_eq!(hint.argument, 2);
        let nested = argument_hint("=IF(SUM(A1,", 11).unwrap();
        assert_eq!((nested.function.name.as_str(), nested.argument), ("SUM", 1));
        assert_eq!(argument_hint("=IF(\"a,b\",", 10).unwrap().argument, 1);
        assert!(argument_hint("=1+2", 4).is_none());
    }

    #[test]
    fn references_are_found_coloured_and_cycled() {
        let spans = reference_spans("=A1+'My s'!B2:C3*A1+SUM(Sheet2!D4)+\"E5\"");
        let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["A1", "'My s'!B2:C3", "A1", "Sheet2!D4"]);
        assert_eq!(spans[0].colour, spans[2].colour);
        assert_ne!(spans[0].colour, spans[1].colour);
        assert!(reference_spans("=LOG10(5)").is_empty());
        let (text, _) = cycle_reference("=A1+B2", 2);
        assert_eq!(text, "=$A$1+B2");
        let (text, _) = cycle_reference(&text, 3);
        assert_eq!(text, "=A$1+B2");
        let (text, _) = cycle_reference(&text, 3);
        assert_eq!(text, "=$A1+B2");
        let (text, _) = cycle_reference(&text, 3);
        assert_eq!(text, "=A1+B2");
    }

    #[test]
    fn picking_ranges_and_closing() {
        assert_eq!(
            insert_reference("=SUM(", 5, "A1:A9"),
            ("=SUM(A1:A9".to_string(), 10)
        );
        assert_eq!(insert_reference("=SUM(A1:A9", 10, "B1:B9").0, "=SUM(B1:B9");
        assert!(expects_reference("=SUM(", 5));
        assert!(!expects_reference("=SUM(A1", 7));
        assert_eq!(auto_close("=IF(A1>0,\"yes"), "=IF(A1>0,\"yes\")");
        assert_eq!(auto_close("=SUM(ROUND(A1,2"), "=SUM(ROUND(A1,2))");
    }
}
