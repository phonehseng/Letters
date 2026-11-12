//! table references: "Table1[Price]", "Table1[#All]", "[@Price]",
//! "Table1[[#Headers],[Price]:[Tax]]". inside brackets a "'" escapes the
//! next character, as excel's documentation describes.

use super::super::ast::{Expr, StructuredRef, TableItem};
use super::{ParseError, Parser};

impl Parser {
    /// the cursor is on "[" after the table name (or with no name)
    pub(super) fn structured(&mut self, table: Option<String>) -> Result<Expr, ParseError> {
        let start = self.pos;
        self.skip_bracket_group()?;
        let inner: String = self.chars[start + 1..self.pos - 1].iter().collect();
        let failed = ParseError {
            message: "this table reference can't be read".into(),
            position: start,
        };
        let reference = parse_inside(table, &inner).ok_or(failed)?;
        Ok(Expr::Structured {
            reference: Box::new(reference),
        })
    }

    /// moves past "[...]" including nested brackets
    fn skip_bracket_group(&mut self) -> Result<(), ParseError> {
        let mut depth = 0;
        loop {
            match self.peek() {
                None => return Err(self.error("a \"]\" is missing")),
                Some('\'') => self.pos += 2,
                Some('[') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(']') => {
                    depth -= 1;
                    self.pos += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(_) => self.pos += 1,
            }
        }
    }
}

fn table_item(text: &str) -> Option<TableItem> {
    match text.trim().to_ascii_lowercase().as_str() {
        "#all" => Some(TableItem::All),
        "#data" => Some(TableItem::Data),
        "#headers" => Some(TableItem::Headers),
        "#totals" => Some(TableItem::Totals),
        "#this row" | "@" => Some(TableItem::ThisRow),
        _ => None,
    }
}

/// a column name with its "'" escapes removed
fn unescape_column(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_inside(table: Option<String>, inner: &str) -> Option<StructuredRef> {
    let mut reference = StructuredRef {
        table,
        items: Vec::new(),
        first_column: None,
        last_column: None,
    };
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Some(reference);
    }
    if let Some(rest) = trimmed.strip_prefix('@') {
        reference.items.push(TableItem::ThisRow);
        let rest = rest.trim();
        if !rest.is_empty() {
            let column = rest
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .unwrap_or(rest);
            reference.first_column = Some(unescape_column(column));
        }
        return Some(reference);
    }
    if !trimmed.starts_with('[') {
        match table_item(trimmed) {
            Some(item) => reference.items.push(item),
            None => reference.first_column = Some(unescape_column(trimmed)),
        }
        return Some(reference);
    }
    let mut columns = Vec::new();
    for part in bracketed_parts(trimmed)? {
        match table_item(&part) {
            Some(item) => reference.items.push(item),
            None => columns.push(unescape_column(&part)),
        }
    }
    let mut columns = columns.into_iter();
    reference.first_column = columns.next();
    reference.last_column = columns.next();
    Some(reference)
}

/// "[#Headers],[Price]:[Tax]" -> ["#Headers", "Price", "Tax"]; none when
/// something other than brackets, commas and colons sits between them
fn bracketed_parts(text: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                current.push(c);
                current.extend(chars.next());
            }
            '[' => {
                depth += 1;
                if depth > 1 {
                    current.push(c);
                }
            }
            ']' => {
                depth -= 1;
                if depth > 0 {
                    current.push(c);
                } else {
                    parts.push(std::mem::take(&mut current));
                }
            }
            ',' | ':' if depth == 0 => {}
            c if depth == 0 && c.is_whitespace() => {}
            _ if depth == 0 => return None,
            c => current.push(c),
        }
    }
    Some(parts)
}
