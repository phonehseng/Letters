//! comparing the words of two matched pages, and describing what changed.
//!
//! the words of both pages are compared as lists (see diff.rs). the result
//! is a list of pieces: runs of the same words, and changed passages in
//! between. a changed passage that differs only in spacing or hyphenation
//! ("data base" and "database", "compar-ison" and "comparison") is set
//! apart, because it is not a change a reader cares about.

use std::collections::HashMap;
use std::ops::Range;

use crate::diff::{diff, Edit};
use crate::geometry::Rect;
use crate::result::{Category, Change, ChangeKind};
use crate::sentences;
use crate::words::PageText;

/// a same word this short, alone between two changes, is folded into them,
/// so "Changed “red fox” to “blue cat”" reads as one change rather than
/// two around a shared "the".
const SHORT_WORD_CHARS: usize = 3;

/// part of the comparison of two pages.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Piece {
    /// the same words on both pages
    Same {
        left: Range<usize>,
        right: Range<usize>,
    },
    /// words that changed. one side may be empty: then its range is empty
    /// and starts where the other side's words would go
    Changed {
        left: Range<usize>,
        right: Range<usize>,
    },
    /// words that differ only in spacing or hyphenation
    Spacing {
        left: Range<usize>,
        right: Range<usize>,
    },
}

/// compares the words of two pages.
pub(crate) fn compare_words(left: &PageText, right: &PageText) -> Vec<Piece> {
    let (left_ids, right_ids) = word_numbers(left, right);
    let pieces = pieces_from(diff(&left_ids, &right_ids));
    let pieces = fold_short_islands(pieces, left);
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Changed { left: a, right: b } if only_spacing_differs(left, right, &a, &b) => {
                Piece::Spacing { left: a, right: b }
            }
            other => other,
        })
        .collect()
}

/// gives each distinct word one number, the same on both pages.
fn word_numbers<'a>(left: &'a PageText, right: &'a PageText) -> (Vec<u32>, Vec<u32>) {
    let mut numbers: HashMap<&'a str, u32> = HashMap::new();
    let mut numbers_of = |page: &'a PageText| -> Vec<u32> {
        page.words
            .iter()
            .map(|word| {
                let next = numbers.len() as u32;
                *numbers.entry(word.key.as_str()).or_insert(next)
            })
            .collect()
    };
    let left_ids = numbers_of(left);
    let right_ids = numbers_of(right);
    (left_ids, right_ids)
}

/// turns edits into pieces, putting removals and additions that touch
/// into one changed passage.
fn pieces_from(edits: Vec<Edit>) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    for edit in edits {
        let (left, right) = match edit {
            Edit::Same { left, right } => {
                pieces.push(Piece::Same { left, right });
                continue;
            }
            Edit::Removed { left, right_at } => (left, right_at..right_at),
            Edit::Added { right, left_at } => (left_at..left_at, right),
        };
        match pieces.last_mut() {
            Some(Piece::Changed {
                left: last_left,
                right: last_right,
            }) => {
                *last_left = last_left.start.min(left.start)..last_left.end.max(left.end);
                *last_right = last_right.start.min(right.start)..last_right.end.max(right.end);
            }
            _ => pieces.push(Piece::Changed { left, right }),
        }
    }
    pieces
}

/// folds a single short same word between two changed passages into one.
fn fold_short_islands(pieces: Vec<Piece>, left: &PageText) -> Vec<Piece> {
    let mut folded: Vec<Piece> = Vec::with_capacity(pieces.len());
    let mut position = 0;
    while position < pieces.len() {
        let short_same = match &pieces[position] {
            Piece::Same { left: a, .. } => {
                a.len() == 1 && left.words[a.start].key.chars().count() <= SHORT_WORD_CHARS
            }
            _ => false,
        };
        let next = pieces.get(position + 1);
        if let (
            true,
            Some(Piece::Changed {
                left: next_left,
                right: next_right,
            }),
            Some(Piece::Changed {
                left: last_left,
                right: last_right,
            }),
        ) = (short_same, next, folded.last_mut())
        {
            last_left.end = next_left.end;
            last_right.end = next_right.end;
            position += 2;
        } else {
            folded.push(pieces[position].clone());
            position += 1;
        }
    }
    folded
}

/// true when both sides spell the same letters once spaces and hyphens go.
fn only_spacing_differs(
    left: &PageText,
    right: &PageText,
    a: &Range<usize>,
    b: &Range<usize>,
) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let squash = |page: &PageText, range: &Range<usize>| -> String {
        page.words[range.clone()]
            .iter()
            .flat_map(|word| word.key.chars())
            .filter(|ch| !matches!(ch, '-' | '\u{2010}' | '\u{ad}'))
            .collect()
    };
    squash(left, a) == squash(right, b)
}

/// the words of a range joined by spaces.
pub(crate) fn text_of(page: &PageText, range: &Range<usize>) -> String {
    page.words[range.clone()]
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// the comparison keys of a range, for finding moved passages.
pub(crate) fn keys_of(page: &PageText, range: &Range<usize>) -> Vec<String> {
    page.words[range.clone()]
        .iter()
        .map(|word| word.key.clone())
        .collect()
}

/// one rectangle per line covered by a run of words.
pub(crate) fn line_rects(page: &PageText, range: &Range<usize>) -> Vec<Rect> {
    let mut by_line: Vec<(usize, Rect)> = Vec::new();
    let mut add =
        |line: usize, rect: Rect| match by_line.iter_mut().find(|(known, _)| *known == line) {
            Some((_, sum)) => *sum = sum.union(&rect),
            None => by_line.push((line, rect)),
        };
    for word in &page.words[range.clone()] {
        for (part, rect) in word.rects.iter().enumerate() {
            // the second part of a hyphenated word is on the next line
            add(word.line + part, *rect);
        }
    }
    by_line.into_iter().map(|(_, rect)| rect).collect()
}

/// a thin mark where words would go on a page that doesn't have them,
/// so the ui can draw a connector to the right place.
pub(crate) fn caret(page: &PageText, at: usize) -> Vec<Rect> {
    if let Some(word) = page.words.get(at) {
        let rect = word.rects[0];
        return vec![Rect::new(rect.x, rect.y, 0.0, rect.height)];
    }
    if let Some(word) = at.checked_sub(1).and_then(|before| page.words.get(before)) {
        let rect = *word.rects.last().unwrap_or(&word.rects[0]);
        return vec![Rect::new(rect.right(), rect.y, 0.0, rect.height)];
    }
    Vec::new()
}

/// the changes a page pair's text pieces describe.
pub(crate) struct TextFindings {
    pub changes: Vec<Change>,
    /// removed passages, with the index of their change in `changes`
    pub removed: Vec<(usize, Vec<String>)>,
    pub added: Vec<(usize, Vec<String>)>,
    pub ignored: u32,
}

pub(crate) fn text_changes(
    pieces: &[Piece],
    left: &PageText,
    right: &PageText,
    pages: (u32, u32),
    report_spacing: bool,
) -> TextFindings {
    let mut findings = TextFindings {
        changes: Vec::new(),
        removed: Vec::new(),
        added: Vec::new(),
        ignored: 0,
    };
    for piece in pieces {
        match piece {
            Piece::Same { .. } => {}
            Piece::Spacing { left: a, right: b } => {
                if report_spacing {
                    let mut change = changed_text(left, right, a, b, pages);
                    change.detail = sentences::spacing_changed(&text_of(right, b));
                    findings.changes.push(change);
                } else {
                    findings.ignored += 1;
                }
            }
            Piece::Changed { left: a, right: b } => {
                let change = changed_text(left, right, a, b, pages);
                let index = findings.changes.len();
                match change.kind {
                    ChangeKind::Deleted => findings.removed.push((index, keys_of(left, a))),
                    ChangeKind::Inserted => findings.added.push((index, keys_of(right, b))),
                    _ => {}
                }
                findings.changes.push(change);
            }
        }
    }
    findings
}

fn changed_text(
    left: &PageText,
    right: &PageText,
    a: &Range<usize>,
    b: &Range<usize>,
    pages: (u32, u32),
) -> Change {
    let old_text = text_of(left, a);
    let new_text = text_of(right, b);
    let (kind, detail, left_rects, right_rects) = if a.is_empty() {
        (
            ChangeKind::Inserted,
            sentences::added(&new_text),
            caret(left, a.start),
            line_rects(right, b),
        )
    } else if b.is_empty() {
        (
            ChangeKind::Deleted,
            sentences::removed(&old_text),
            line_rects(left, a),
            caret(right, b.start),
        )
    } else {
        (
            ChangeKind::Replaced,
            sentences::replaced(&old_text, &new_text),
            line_rects(left, a),
            line_rects(right, b),
        )
    };
    Change {
        kind,
        category: Category::Text,
        left_page: Some(pages.0),
        right_page: Some(pages.1),
        left_rects,
        right_rects,
        old_text: (!a.is_empty()).then_some(old_text),
        new_text: (!b.is_empty()).then_some(new_text),
        detail,
        ..Change::default()
    }
}
