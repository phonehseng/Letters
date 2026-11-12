//! cell addresses: "B3", "A1:C10", column letters and the grid's limits.
//!
//! rows and columns are counted from zero inside the crate (row 0 is the
//! row people call 1). only text meant for people or files uses the
//! one-based a1 style.

use serde::{Deserialize, Serialize};

/// the largest grid excel and libreoffice both allow.
pub const MAX_ROWS: u32 = 1_048_576;
pub const MAX_COLS: u32 = 16_384;

/// one cell position, zero based.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct CellRef {
    pub row: u32,
    pub col: u32,
}

impl CellRef {
    pub fn new(row: u32, col: u32) -> CellRef {
        CellRef { row, col }
    }

    /// reads "B3" (dollar signs allowed and ignored).
    pub fn parse(text: &str) -> Option<CellRef> {
        let text = text.trim();
        let bytes = text.as_bytes();
        let mut index = 0;
        if bytes.first() == Some(&b'$') {
            index += 1;
        }
        let letters_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let col = column_index(&text[letters_start..index])?;
        if bytes.get(index) == Some(&b'$') {
            index += 1;
        }
        let digits = &text[index..];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let row: u32 = digits.parse().ok()?;
        if row == 0 || row > MAX_ROWS {
            return None;
        }
        Some(CellRef { row: row - 1, col })
    }

    /// "B3"
    pub fn to_a1(&self) -> String {
        format!("{}{}", column_name(self.col), self.row + 1)
    }
}

/// a rectangle of cells, both corners included, always with
/// `start <= end` in both directions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: CellRef,
    pub end: CellRef,
}

impl Range {
    /// a range from any two corners.
    pub fn new(a: CellRef, b: CellRef) -> Range {
        Range {
            start: CellRef::new(a.row.min(b.row), a.col.min(b.col)),
            end: CellRef::new(a.row.max(b.row), a.col.max(b.col)),
        }
    }

    pub fn cell(cell: CellRef) -> Range {
        Range {
            start: cell,
            end: cell,
        }
    }

    /// from zero-based row and column numbers, both ends included.
    pub fn from_bounds(row1: u32, col1: u32, row2: u32, col2: u32) -> Range {
        Range::new(CellRef::new(row1, col1), CellRef::new(row2, col2))
    }

    /// reads "A1:C10", "B3", "A:A" or "3:5".
    pub fn parse(text: &str) -> Option<Range> {
        let text = text.trim();
        match text.split_once(':') {
            None => CellRef::parse(text).map(Range::cell),
            Some((a, b)) => {
                if let (Some(a), Some(b)) = (CellRef::parse(a), CellRef::parse(b)) {
                    return Some(Range::new(a, b));
                }
                let clean = |s: &str| s.trim().trim_start_matches('$').to_string();
                let (a, b) = (clean(a), clean(b));
                if let (Some(c1), Some(c2)) = (column_index(&a), column_index(&b)) {
                    return Some(Range::from_bounds(0, c1, MAX_ROWS - 1, c2));
                }
                let r1: u32 = a.parse().ok()?;
                let r2: u32 = b.parse().ok()?;
                if r1 == 0 || r2 == 0 || r1 > MAX_ROWS || r2 > MAX_ROWS {
                    return None;
                }
                Some(Range::from_bounds(r1 - 1, 0, r2 - 1, MAX_COLS - 1))
            }
        }
    }

    /// a space separated list as excel writes it ("A1:B2 D4").
    pub fn parse_list(text: &str) -> Vec<Range> {
        text.split_whitespace().filter_map(Range::parse).collect()
    }

    pub fn list_to_a1(ranges: &[Range]) -> String {
        ranges
            .iter()
            .map(Range::to_a1)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// "A1:C10", or "B3" for one cell.
    pub fn to_a1(&self) -> String {
        if self.start == self.end {
            self.start.to_a1()
        } else if self.is_whole_columns() {
            format!(
                "{}:{}",
                column_name(self.start.col),
                column_name(self.end.col)
            )
        } else if self.is_whole_rows() {
            format!("{}:{}", self.start.row + 1, self.end.row + 1)
        } else {
            format!("{}:{}", self.start.to_a1(), self.end.to_a1())
        }
    }

    pub fn is_whole_columns(&self) -> bool {
        self.start.row == 0 && self.end.row == MAX_ROWS - 1
    }

    pub fn is_whole_rows(&self) -> bool {
        self.start.col == 0 && self.end.col == MAX_COLS - 1
    }

    pub fn rows(&self) -> u32 {
        self.end.row - self.start.row + 1
    }

    pub fn cols(&self) -> u32 {
        self.end.col - self.start.col + 1
    }

    pub fn cell_count(&self) -> u64 {
        u64::from(self.rows()) * u64::from(self.cols())
    }

    pub fn contains(&self, cell: CellRef) -> bool {
        cell.row >= self.start.row
            && cell.row <= self.end.row
            && cell.col >= self.start.col
            && cell.col <= self.end.col
    }

    pub fn contains_range(&self, other: &Range) -> bool {
        self.contains(other.start) && self.contains(other.end)
    }

    pub fn intersects(&self, other: &Range) -> bool {
        self.start.row <= other.end.row
            && other.start.row <= self.end.row
            && self.start.col <= other.end.col
            && other.start.col <= self.end.col
    }

    pub fn intersection(&self, other: &Range) -> Option<Range> {
        if !self.intersects(other) {
            return None;
        }
        Some(Range::from_bounds(
            self.start.row.max(other.start.row),
            self.start.col.max(other.start.col),
            self.end.row.min(other.end.row),
            self.end.col.min(other.end.col),
        ))
    }

    /// every cell, row by row.
    pub fn iter(&self) -> impl Iterator<Item = CellRef> + '_ {
        (self.start.row..=self.end.row).flat_map(move |row| {
            (self.start.col..=self.end.col).map(move |col| CellRef::new(row, col))
        })
    }
}

/// 0 -> "A", 25 -> "Z", 26 -> "AA".
pub fn column_name(col: u32) -> String {
    let mut n = col + 1;
    let mut letters = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        letters.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

/// "A" -> 0, "AA" -> 26; letters only, any case, within the grid.
pub fn column_index(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let mut n: u32 = 0;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1);
    }
    if n == 0 || n > MAX_COLS {
        return None;
    }
    Some(n - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_round_trip() {
        for (col, name) in [
            (0, "A"),
            (25, "Z"),
            (26, "AA"),
            (701, "ZZ"),
            (702, "AAA"),
            (16383, "XFD"),
        ] {
            assert_eq!(column_name(col), name);
            assert_eq!(column_index(name), Some(col));
        }
        assert_eq!(column_index("XFE"), None);
        assert_eq!(column_index("A1"), None);
    }

    #[test]
    fn ranges_parse_and_print() {
        let range = Range::parse("$C$10:a1").unwrap();
        assert_eq!(range.to_a1(), "A1:C10");
        assert_eq!(range.cell_count(), 30);
        assert_eq!(Range::parse("B:D").unwrap().to_a1(), "B:D");
        assert_eq!(Range::parse("3:5").unwrap().to_a1(), "3:5");
        assert!(Range::parse("A0").is_none());
        assert_eq!(Range::parse_list("A1:B2 D4").len(), 2);
        let a = Range::parse("A1:C3").unwrap();
        assert_eq!(
            a.intersection(&Range::parse("B2:D4").unwrap())
                .unwrap()
                .to_a1(),
            "B2:C3"
        );
    }
}
