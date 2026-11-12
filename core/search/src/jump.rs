//! the page finder: turning what a reader types into places to jump to.
//!
//! readers say where they want to go in many ways: "12", "p 12",
//! "page xii" (the page printed as xii), "12/300", "50%", "ch 3",
//! "#methods", "last" (where they stopped reading), "+5" (five pages on),
//! and in a word document "w 5000" (the five thousandth word).
//! `resolve_jump` understands all of them and returns every place that
//! fits, best first, so the Go To field can list them as the reader types.
//!
//! the interface has a copy of these rules in `lib/reading/jump.ts`, so
//! the list can update on every key without a round trip; both have tests
//! for the same cases.

use serde::{Deserialize, Serialize};

use crate::normalize::fold;

/// a heading or a bookmark: something with a name and a place.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JumpEntry {
    pub title: String,
    /// counted from 0
    pub page: u32,
    pub level: u32,
    /// for word documents: the block the writer scrolls to
    pub block: Option<u32>,
}

/// what is known about the document being read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JumpDocument {
    pub page_count: u32,
    /// counted from 0
    pub current_page: u32,
    /// the printed page numbers of a pdf ("i", "ii", "1"...), one per
    /// page, or empty when it has none
    pub page_labels: Vec<String>,
    pub outline: Vec<JumpEntry>,
    pub bookmarks: Vec<JumpEntry>,
    pub last_position: Option<u32>,
    /// word documents: (block, words before it), in order
    pub word_starts: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JumpKind {
    Page,
    Label,
    Percent,
    Relative,
    Outline,
    Bookmark,
    LastPosition,
    Word,
}

/// one place the reader might mean.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpCandidate {
    pub kind: JumpKind,
    /// counted from 0
    pub page: u32,
    pub block: Option<u32>,
    /// what the list shows ("Page 12", "Chapter 3: Cells")
    pub label: String,
}

const MAX_NAME_MATCHES: usize = 8;

/// every place `typed` could mean in `document`, best first.
pub fn resolve_jump(document: &JumpDocument, typed: &str) -> Vec<JumpCandidate> {
    let text = fold(typed.trim());
    let mut found = Vec::new();
    if text.is_empty() || document.page_count == 0 {
        return found;
    }
    if let Some(name) = text.strip_prefix('#') {
        names(document, name.trim(), &mut found);
        return dedupe(found);
    }
    keywords(document, &text, &mut found);
    percent(document, &text, &mut found);
    relative(document, &text, &mut found);
    word_offset(document, &text, &mut found);
    pages(document, &text, &mut found);
    chapters(document, &text, &mut found);
    names(document, &text, &mut found);
    dedupe(found)
}

fn page_candidate(
    document: &JumpDocument,
    kind: JumpKind,
    page: u32,
    label: String,
) -> JumpCandidate {
    JumpCandidate {
        kind,
        page: page.min(document.page_count - 1),
        block: None,
        label,
    }
}

fn page_name(document: &JumpDocument, page: u32) -> String {
    match document.page_labels.get(page as usize) {
        Some(label) if *label != (page + 1).to_string() && !label.is_empty() => {
            format!("Page {} (printed {label})", page + 1)
        }
        _ => format!("Page {} of {}", page + 1, document.page_count),
    }
}

fn keywords(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    let last = document.page_count - 1;
    match text {
        "last" | "last position" | "where i left off" | "resume" | "continue" => {
            if let Some(page) = document.last_position {
                found.push(page_candidate(
                    document,
                    JumpKind::LastPosition,
                    page,
                    format!("Where You Left Off, page {}", page + 1),
                ));
            }
        }
        "start" | "beginning" | "first" | "first page" | "top" => {
            found.push(page_candidate(
                document,
                JumpKind::Page,
                0,
                page_name(document, 0),
            ));
        }
        "end" | "last page" | "bottom" => {
            found.push(page_candidate(
                document,
                JumpKind::Page,
                last,
                page_name(document, last),
            ));
        }
        _ => {}
    }
}

fn percent(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    let Some(number) = text.strip_suffix('%') else {
        return;
    };
    let Ok(share) = number.trim().parse::<f64>() else {
        return;
    };
    if !(0.0..=100.0).contains(&share) {
        return;
    }
    let page = ((share / 100.0) * f64::from(document.page_count)).floor() as u32;
    let page = page.min(document.page_count - 1);
    found.push(page_candidate(
        document,
        JumpKind::Percent,
        page,
        format!("{}% Through, page {}", trim_number(share), page + 1),
    ));
}

fn trim_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as u32)
    } else {
        format!("{value}")
    }
}

fn relative(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    let (sign, rest) = match text.chars().next() {
        Some('+') => (1i64, &text[1..]),
        Some('-') => (-1i64, &text[1..]),
        _ => return,
    };
    let Ok(steps) = rest.trim().parse::<i64>() else {
        return;
    };
    let target = i64::from(document.current_page) + sign * steps;
    let target = target.clamp(0, i64::from(document.page_count) - 1) as u32;
    let words = if sign > 0 { "On" } else { "Back" };
    found.push(page_candidate(
        document,
        JumpKind::Relative,
        target,
        format!("{steps} Pages {words}, page {}", target + 1),
    ));
}

fn word_offset(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    if document.word_starts.is_empty() {
        return;
    }
    let Some(rest) = strip_word_prefix(text, &["words", "word", "w"]) else {
        return;
    };
    let Ok(word) = rest.parse::<u32>() else {
        return;
    };
    let block = document
        .word_starts
        .iter()
        .take_while(|(_, before)| *before < word.max(1))
        .last()
        .or(document.word_starts.first());
    if let Some((block, _)) = block {
        found.push(JumpCandidate {
            kind: JumpKind::Word,
            page: 0,
            block: Some(*block),
            label: format!("Word {word}"),
        });
    }
}

/// "p 12", "page12", "pg. 12" -> "12"; none when `text` has no such start.
fn strip_word_prefix<'t>(text: &'t str, prefixes: &[&str]) -> Option<&'t str> {
    for prefix in prefixes {
        if let Some(after) = text.strip_prefix(prefix) {
            // "photo" must not read as "p" + "hoto": a word prefix needs a
            // space or a dot after it, or a number straight after ("p12")
            let separated = after.starts_with(|c: char| c == '.' || c.is_whitespace());
            let rest = after.trim_start_matches('.').trim();
            if !rest.is_empty() && (separated || rest.starts_with(|c: char| c.is_ascii_digit())) {
                return Some(rest);
            }
        }
    }
    None
}

fn pages(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    // "+5" is a move, not page 5 (rust reads "+5" as a number)
    if text.starts_with(['+', '-']) {
        return;
    }
    let rest = strip_word_prefix(text, &["pages", "page", "pg", "pp", "p"]).unwrap_or(text);
    // "12/300" and "12 of 300" both mean page 12
    let first = rest.split(['/', ' ']).next().unwrap_or(rest).trim();
    let label_matches = label_pages(document, first);
    if let Ok(number) = first.parse::<u32>() {
        // a printed number that differs from the count from the cover is offered first
        for page in label_matches.iter().filter(|p| **p + 1 != number) {
            found.push(page_candidate(
                document,
                JumpKind::Label,
                *page,
                page_name(document, *page),
            ));
        }
        if (1..=document.page_count).contains(&number) {
            let page = number - 1;
            found.push(page_candidate(
                document,
                JumpKind::Page,
                page,
                page_name(document, page),
            ));
        }
        return;
    }
    for page in label_matches {
        found.push(page_candidate(
            document,
            JumpKind::Label,
            page,
            page_name(document, page),
        ));
    }
}

fn label_pages(document: &JumpDocument, text: &str) -> Vec<u32> {
    document
        .page_labels
        .iter()
        .enumerate()
        .filter(|(_, label)| !label.is_empty() && fold(label) == text)
        .map(|(page, _)| page as u32)
        .collect()
}

fn chapters(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    let Some(rest) = strip_word_prefix(
        text,
        &[
            "chapter", "chap", "ch", "section", "sect", "sec", "part", "pt",
        ],
    ) else {
        return;
    };
    let rest = rest.trim();
    for entry in &document.outline {
        let title = fold(&entry.title);
        if starts_with_number(&title, rest) {
            found.push(entry_candidate(entry, JumpKind::Outline));
        }
    }
    // "ch 3" in a book whose chapters are only named: the third top-level entry
    if let Ok(number) = rest.parse::<usize>() {
        let top_level: Vec<&JumpEntry> = document.outline.iter().filter(|e| e.level == 0).collect();
        if let Some(entry) = number.checked_sub(1).and_then(|i| top_level.get(i)) {
            found.push(entry_candidate(entry, JumpKind::Outline));
        }
    }
}

/// "chapter 3 cells", "3. cells", "3 cells", "part iii" all start with "3" or "iii".
fn starts_with_number(title: &str, number: &str) -> bool {
    let words: Vec<&str> = title
        .split(|c: char| c.is_whitespace() || c == '.' || c == ':')
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [first, ..] if *first == number => true,
        [_, second, ..] => *second == number,
        _ => false,
    }
}

fn entry_candidate(entry: &JumpEntry, kind: JumpKind) -> JumpCandidate {
    JumpCandidate {
        kind,
        page: entry.page,
        block: entry.block,
        label: entry.title.trim().to_string(),
    }
}

/// headings and bookmarks whose name holds the text, those where a word
/// starts with it first.
fn names(document: &JumpDocument, text: &str, found: &mut Vec<JumpCandidate>) {
    if text.chars().count() < 2 {
        return;
    }
    let mut scored: Vec<(u8, JumpCandidate)> = Vec::new();
    let sources = document
        .bookmarks
        .iter()
        .map(|e| (e, JumpKind::Bookmark))
        .chain(document.outline.iter().map(|e| (e, JumpKind::Outline)));
    for (entry, kind) in sources {
        let title = fold(&entry.title);
        let rank = if title == text {
            0
        } else if title.starts_with(text) {
            1
        } else if title.split_whitespace().any(|word| word.starts_with(text)) {
            2
        } else if title.contains(text) {
            3
        } else {
            continue;
        };
        scored.push((rank, entry_candidate(entry, kind)));
    }
    scored.sort_by_key(|(rank, _)| *rank);
    found.extend(scored.into_iter().take(MAX_NAME_MATCHES).map(|(_, c)| c));
}

fn dedupe(found: Vec<JumpCandidate>) -> Vec<JumpCandidate> {
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .filter(|c| seen.insert((c.kind, c.page, c.block, c.label.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> JumpDocument {
        let mut labels: Vec<String> = [
            "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        labels.extend((1..=288).map(|n| n.to_string()));
        labels[299] = "A-3".into();
        let entry = |title: &str, page: u32, level: u32| JumpEntry {
            title: title.into(),
            page,
            level,
            block: None,
        };
        JumpDocument {
            page_count: 300,
            current_page: 40,
            page_labels: labels,
            outline: vec![
                entry("Preface", 2, 0),
                entry("Chapter 1: Cells", 12, 0),
                entry("Methods", 20, 1),
                entry("Chapter 2: Energy", 40, 0),
                entry("3. Photosynthesis", 80, 0),
            ],
            bookmarks: vec![entry("Exam formula", 150, 0)],
            last_position: Some(77),
            word_starts: vec![],
        }
    }

    fn pages_of(found: &[JumpCandidate]) -> Vec<(JumpKind, u32)> {
        found.iter().map(|c| (c.kind, c.page)).collect()
    }

    #[test]
    fn plain_numbers_and_page_prefixes() {
        let doc = book();
        for typed in [
            "12",
            "p 12",
            "p12",
            "page 12",
            "pg. 12",
            "12/300",
            "12 of 300",
            "  P 12 ",
        ] {
            let found = resolve_jump(&doc, typed);
            // printed page 12 is the 24th sheet; the 12th sheet is offered too
            assert_eq!(
                pages_of(&found),
                vec![(JumpKind::Label, 23), (JumpKind::Page, 11)],
                "{typed}"
            );
        }
        assert_eq!(found_label(&doc, "12"), "Page 24 (printed 12)");
        assert!(resolve_jump(&doc, "301").is_empty());
    }

    fn found_label(doc: &JumpDocument, typed: &str) -> String {
        resolve_jump(doc, typed)[0].label.clone()
    }

    #[test]
    fn printed_labels_like_roman_numerals() {
        let doc = book();
        assert_eq!(
            pages_of(&resolve_jump(&doc, "page xii")),
            vec![(JumpKind::Label, 11)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "iv")),
            vec![(JumpKind::Label, 3)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "a-3")),
            vec![(JumpKind::Label, 299)]
        );
    }

    #[test]
    fn percentages_relative_moves_and_keywords() {
        let doc = book();
        assert_eq!(
            pages_of(&resolve_jump(&doc, "50%")),
            vec![(JumpKind::Percent, 150)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "100%")),
            vec![(JumpKind::Percent, 299)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "+5")),
            vec![(JumpKind::Relative, 45)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "-100")),
            vec![(JumpKind::Relative, 0)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "last")),
            vec![(JumpKind::LastPosition, 77)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "end")),
            vec![(JumpKind::Page, 299)]
        );
        assert!(resolve_jump(&doc, "150%").is_empty());
    }

    #[test]
    fn chapters_by_number() {
        let doc = book();
        let found = resolve_jump(&doc, "ch 2");
        assert_eq!(found[0].label, "Chapter 2: Energy");
        assert_eq!(found[0].page, 40);
        // "3. Photosynthesis" is numbered in its title
        assert_eq!(resolve_jump(&doc, "chapter 3")[0].page, 80);
    }

    #[test]
    fn names_of_headings_and_bookmarks() {
        let doc = book();
        assert_eq!(
            pages_of(&resolve_jump(&doc, "#methods")),
            vec![(JumpKind::Outline, 20)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "photo")),
            vec![(JumpKind::Outline, 80)]
        );
        assert_eq!(
            pages_of(&resolve_jump(&doc, "exam")),
            vec![(JumpKind::Bookmark, 150)]
        );
        // accents and case do not matter
        assert_eq!(
            pages_of(&resolve_jump(&doc, "PRÉFACE")),
            vec![(JumpKind::Outline, 2)]
        );
        assert!(resolve_jump(&doc, "zebra").is_empty());
        assert!(resolve_jump(&doc, "").is_empty());
    }

    #[test]
    fn word_offsets_in_word_documents() {
        let doc = JumpDocument {
            page_count: 10,
            word_starts: vec![(0, 0), (4, 120), (9, 4000), (15, 6000)],
            ..JumpDocument::default()
        };
        let found = resolve_jump(&doc, "w 5000");
        assert_eq!(found[0].block, Some(9));
        assert_eq!(resolve_jump(&doc, "word 1")[0].block, Some(0));
    }
}
