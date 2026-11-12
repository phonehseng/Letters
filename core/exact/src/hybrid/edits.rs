//! the edit tests: what a person does in word to a converted document, and
//! whether only what should move moved.
//!
//! four edits, each exported through word next to the unedited document:
//! a sentence typed into the middle of a promoted paragraph, a sentence
//! deleted from one, text typed at the end of the document, and one char
//! typed and undone. the rules checked ("adding a line or two without
//! altering any spacing"):
//! - the edited paragraph re-wraps: its later lines change;
//! - the paragraphs below it in its shared frame move by whole line pitches
//!   of the edited paragraph (zero if it kept its line count), straight down;
//! - every other glyph stays exactly where it was.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use super::proof::{measured, GlyphKey};
use super::{HybridReport, PromotedParagraph};
use crate::bridge::{Edit, EditAction};
use crate::pdf::PageContent;
use crate::verify::matching::matched_pairs;
use crate::verify::placed_glyphs;

/// a glyph that moved less than this did not move (word prints to 0.001 pt).
pub const UNMOVED_PT: f64 = 0.002;
/// content below moves by the line pitch as word lays it out: line tops are
/// twips turned into whole 0.12 pt pixels (recipe 6.1), so the move is the
/// pitch rounded to a pixel, and printed on the 0.024 pt lattice with its
/// 0.144 pt steps (recipe 6.3).
pub const PITCH_MOVE_TOLERANCE_PT: f64 = 0.15;
/// the sentence typed into a paragraph: only lowercase letters, spaces and a
/// full stop, which every completed face has.
pub const INSERTED: &str =
    " and this sentence, typed into the paragraph in word, is long enough to need a line of its own.";
/// typed at the end of the document.
pub const APPENDED: &str = " More text typed at the end of the document.";

/// the kinds of edit the tests make.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EditKind {
    InsertSentence,
    DeleteSentence,
    TypeAtEnd,
    TypeAndUndo,
}

/// one planned edit and the promoted paragraph it touches.
#[derive(Clone, Debug)]
pub struct PlannedEdit {
    pub kind: EditKind,
    pub edit: Edit,
    /// index into `HybridReport::promoted`
    pub paragraph: usize,
}

/// the paragraph to edit: the promoted multi-line paragraph with the most
/// paragraphs below it in its frame, then the most lines.
fn paragraph_to_edit(report: &HybridReport) -> Option<usize> {
    let below = |p: &PromotedParagraph| {
        report
            .promoted
            .iter()
            .filter(|q| {
                q.page == p.page && q.stack == p.stack && q.place_in_stack > p.place_in_stack
            })
            .count()
    };
    (0..report.promoted.len())
        .filter(|&i| report.promoted[i].lines >= 2 && report.promoted[i].pins == 0)
        .max_by_key(|&i| (below(&report.promoted[i]), report.promoted[i].lines))
}

/// words of plain text (no private use chars, no line breaks): needles must
/// be typed as they are found.
fn plain(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| (c as u32) < 0x2000 && !c.is_control())
}

/// a run of whole words of `text` around `at` (a word index) that occurs
/// once in the document.
fn unique_needle(words: &[&str], at: usize, document: &str) -> Option<String> {
    for length in 3..=8 {
        let start = at.saturating_sub(length);
        let slice = &words[start..at.max(start + 1).min(words.len())];
        if slice.iter().any(|w| !plain(w)) {
            continue;
        }
        let needle = slice.join(" ");
        if needle.len() >= 12 && document.matches(&needle).count() == 1 {
            return Some(needle);
        }
    }
    None
}

/// the edits to make on a promoted document; empty when nothing is promoted.
pub fn plan_edits(report: &HybridReport, document_text: &str) -> Vec<PlannedEdit> {
    let mut edits = Vec::new();
    let Some(chosen) = paragraph_to_edit(report) else {
        return edits;
    };
    let words: Vec<&str> = report.promoted[chosen].text.split(' ').collect();
    let middle = words.len() / 2;
    if let Some(needle) = unique_needle(&words, middle, document_text) {
        let insert = |action, text: &str, kind| PlannedEdit {
            kind,
            edit: Edit {
                action,
                needle: needle.clone(),
                text: text.to_string(),
            },
            paragraph: chosen,
        };
        edits.push(insert(
            EditAction::Insert,
            INSERTED,
            EditKind::InsertSentence,
        ));
        edits.push(insert(
            EditAction::InsertAndUndo,
            "x",
            EditKind::TypeAndUndo,
        ));
    }
    if let Some(needle) = deleted_span(&words, document_text) {
        edits.push(PlannedEdit {
            kind: EditKind::DeleteSentence,
            edit: Edit {
                action: EditAction::Delete,
                needle,
                text: String::new(),
            },
            paragraph: chosen,
        });
    }
    if let Some(last) = report.promoted.len().checked_sub(1) {
        let words: Vec<&str> = report.promoted[last].text.split(' ').collect();
        if let Some(needle) = unique_needle(&words, words.len(), document_text) {
            edits.push(PlannedEdit {
                kind: EditKind::TypeAtEnd,
                edit: Edit {
                    action: EditAction::Insert,
                    needle,
                    text: APPENDED.to_string(),
                },
                paragraph: last,
            });
        }
    }
    edits
}

/// a sentence in the middle of the paragraph (from after a full stop to
/// its next full stop, with the space after it), else five middle words.
fn deleted_span(words: &[&str], document: &str) -> Option<String> {
    let ends: Vec<usize> = (0..words.len())
        .filter(|&i| words[i].ends_with('.'))
        .collect();
    let sentences = ends.windows(2).map(|w| (w[0] + 1, w[1]));
    let middle = words.len() / 2;
    let chosen = sentences
        .filter(|(start, end)| end > start && *end + 1 < words.len())
        .min_by_key(|(start, end)| ((start + end) / 2).abs_diff(middle));
    let fallback = (
        middle.saturating_sub(2),
        (middle + 2).min(words.len().saturating_sub(2)),
    );
    let (start, end) = chosen.unwrap_or(fallback);
    let span = &words[start..=end];
    if span.iter().any(|w| !plain(w)) {
        return None;
    }
    let needle = format!("{} ", span.join(" "));
    (needle.len() >= 12 && document.matches(&needle).count() == 1).then_some(needle)
}

/// what one edit did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    pub kind: Option<EditKind>,
    pub needle: String,
    /// the edited paragraph's change in lines (negative: shorter)
    pub growth_lines: i64,
    /// the edited paragraph's lines after the edit point changed
    pub rewrapped: bool,
    pub below_glyphs: usize,
    /// glyphs below that moved by exactly the growth, straight down
    pub below_moved_right: usize,
    /// how the first glyph below moved (x, y in points), for the report
    pub below_first_move: Option<(f64, f64)>,
    /// the smallest and largest vertical move below: one block moves together
    pub below_dy_range: Option<(f64, f64)>,
    pub outside_glyphs: usize,
    pub outside_moved: usize,
    /// glyphs in the edited export with no partner: the typed text
    pub new_glyphs: usize,
    /// fonts that drew the typed text (all embedded when no fallback was used)
    pub new_text_fonts: Vec<String>,
    pub pass: bool,
}

/// for every glyph of word's pdf, the source glyph it was paired with.
pub fn source_keys(source: &[PageContent], word: &[PageContent]) -> Vec<Option<GlyphKey>> {
    let keys: Vec<GlyphKey> = source
        .iter()
        .flat_map(|page| page.glyphs.iter())
        .filter(|g| measured(g))
        .map(|g| (g.page, g.sequence))
        .collect();
    let a = placed_glyphs(source);
    let b = placed_glyphs(word);
    let texts_a: Vec<&str> = a.iter().map(|g| g.text.as_str()).collect();
    let texts_b: Vec<&str> = b.iter().map(|g| g.text.as_str()).collect();
    let mut found = vec![None; b.len()];
    for (i, j) in matched_pairs(&texts_a, &texts_b) {
        if a[i].page == b[j].page {
            found[j] = Some(keys[i]);
        }
    }
    found
}

/// the source glyphs of the paragraphs below `edited` in its frame.
fn keys_below(report: &HybridReport, edited: usize) -> BTreeSet<GlyphKey> {
    let p = &report.promoted[edited];
    report
        .promoted
        .iter()
        .filter(|q| q.page == p.page && q.stack == p.stack && q.place_in_stack > p.place_in_stack)
        .flat_map(|q| q.glyphs.iter().flatten().copied())
        .collect()
}

/// compares the edited export with the unedited one.
pub fn judge_edit(
    planned: &PlannedEdit,
    report: &HybridReport,
    source: &[PageContent],
    before: &[PageContent],
    after: &[PageContent],
) -> EditOutcome {
    let paragraph = &report.promoted[planned.paragraph];
    let in_paragraph: BTreeSet<GlyphKey> = paragraph.glyphs.iter().flatten().copied().collect();
    let below = keys_below(report, planned.paragraph);
    let keys = source_keys(source, before);
    let a = placed_glyphs(before);
    let b = placed_glyphs(after);
    let texts_a: Vec<&str> = a.iter().map(|g| g.text.as_str()).collect();
    let texts_b: Vec<&str> = b.iter().map(|g| g.text.as_str()).collect();
    let pairs = matched_pairs(&texts_a, &texts_b);
    let mut outcome = EditOutcome {
        kind: Some(planned.kind),
        needle: planned.edit.needle.clone(),
        ..EditOutcome::default()
    };
    let pitch = paragraph.line_pitch_pt;
    let mut moves: HashMap<GlyphKey, (f64, f64)> = HashMap::new();
    for &(i, j) in &pairs {
        let Some(key) = keys[i] else { continue };
        let moved = if a[i].page == b[j].page {
            (b[j].x - a[i].x, b[j].y - a[i].y)
        } else {
            (f64::INFINITY, f64::INFINITY)
        };
        moves.insert(key, moved);
    }
    outcome.growth_lines = growth(paragraph, &moves, pitch);
    let expected_dy = -(outcome.growth_lines as f64) * pitch;
    for (key, (dx, dy)) in &moves {
        if in_paragraph.contains(key) {
            outcome.rewrapped |= dx.abs() > UNMOVED_PT || dy.abs() > UNMOVED_PT;
        } else if below.contains(key) {
            outcome.below_glyphs += 1;
            if outcome.below_first_move.is_none() {
                outcome.below_first_move = Some((*dx, *dy));
            }
            let (low, high) = outcome.below_dy_range.unwrap_or((*dy, *dy));
            outcome.below_dy_range = Some((low.min(*dy), high.max(*dy)));
            let right =
                dx.abs() <= UNMOVED_PT && (dy - expected_dy).abs() <= PITCH_MOVE_TOLERANCE_PT;
            outcome.below_moved_right += usize::from(right);
        } else {
            outcome.outside_glyphs += 1;
            outcome.outside_moved += usize::from(dx.abs() > UNMOVED_PT || dy.abs() > UNMOVED_PT);
        }
    }
    let paired: BTreeSet<usize> = pairs.iter().map(|&(_, j)| j).collect();
    let mut fonts = BTreeSet::new();
    for (j, glyph) in b.iter().enumerate() {
        if !paired.contains(&j) {
            outcome.new_glyphs += 1;
            fonts.insert(glyph.font.clone());
        }
    }
    outcome.new_text_fonts = fonts.into_iter().collect();
    outcome.pass = passes(&outcome, planned.kind);
    outcome
}

/// lines the paragraph grew by, from where its last glyph went.
fn growth(paragraph: &PromotedParagraph, moves: &HashMap<GlyphKey, (f64, f64)>, pitch: f64) -> i64 {
    let last = paragraph.glyphs.iter().flatten().last();
    match last.and_then(|key| moves.get(key)) {
        Some((_, dy)) if dy.is_finite() && pitch > 0.0 => (-dy / pitch).round() as i64,
        _ => 0,
    }
}

/// the verdict: nothing outside moved; what is below moved right; typed
/// text was drawn in embedded fonts; insert and delete re-wrapped, and
/// typing then undoing left everything (paragraph included) in place.
fn passes(outcome: &EditOutcome, kind: EditKind) -> bool {
    let outside = outcome.outside_moved == 0;
    // everything below moved together: each line top moved by the same
    // twips, which word turns into the same pixels give or take one
    let together = outcome
        .below_dy_range
        .is_none_or(|(low, high)| high - low <= PITCH_MOVE_TOLERANCE_PT);
    let below = outcome.below_moved_right == outcome.below_glyphs && together;
    let fonts = outcome
        .new_text_fonts
        .iter()
        .all(|f| f.contains("WRD_EMBED"));
    match kind {
        EditKind::TypeAndUndo => outside && below && !outcome.rewrapped && outcome.new_glyphs == 0,
        EditKind::TypeAtEnd => outside && below && fonts,
        EditKind::InsertSentence | EditKind::DeleteSentence => {
            outside && below && fonts && outcome.rewrapped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promoted(text: &str, stack: usize, place: usize) -> PromotedParagraph {
        PromotedParagraph {
            page: 0,
            lines: 3,
            level: "everyGlyph".into(),
            stack,
            place_in_stack: place,
            line_pitch_pt: 12.0,
            text: text.into(),
            ..PromotedParagraph::default()
        }
    }

    #[test]
    fn the_edited_paragraph_has_the_most_paragraphs_below_it() {
        let report = HybridReport {
            promoted: vec![
                promoted("alone in its frame", 1, 0),
                promoted("first of three", 2, 0),
                promoted("second of three", 2, 1),
                promoted("third of three", 2, 2),
            ],
            ..HybridReport::default()
        };
        assert_eq!(paragraph_to_edit(&report), Some(1));
    }

    #[test]
    fn needles_are_unique_plain_words() {
        let text = "the first sample shows the drift. the second sample shows the noise.";
        let words: Vec<&str> = text.split(' ').collect();
        let needle = unique_needle(&words, 8, text).unwrap();
        assert_eq!(text.matches(&needle).count(), 1);
        assert!(needle.len() >= 12);
        let deleted = deleted_span(
            &["one.", "two", "three", "four.", "five", "six."],
            "one. two three four. five six.",
        )
        .unwrap();
        assert_eq!(deleted, "two three four. ");
    }

    #[test]
    fn a_pass_needs_nothing_outside_to_move() {
        let mut outcome = EditOutcome {
            rewrapped: true,
            below_glyphs: 10,
            below_moved_right: 10,
            outside_glyphs: 100,
            new_text_fonts: vec!["___WRD_EMBED_SUB_45".into()],
            ..EditOutcome::default()
        };
        assert!(passes(&outcome, EditKind::InsertSentence));
        outcome.outside_moved = 1;
        assert!(!passes(&outcome, EditKind::InsertSentence));
        outcome.outside_moved = 0;
        outcome.new_text_fonts.push("Calibri".into());
        assert!(!passes(&outcome, EditKind::InsertSentence));
    }
}
