//! comparing how unchanged text looks: font, size, bold, italic and colour.
//!
//! only words that are the same on both pages are compared, one pair at a
//! time. neighbouring words with the same difference in the same block are
//! reported together as one run, so a heading made bigger is one change and
//! not one per word.

use crate::result::{Category, Change, ChangeKind, StyleChange, TextStyle};
use crate::sentences::{capitalised, colour_name, join_parts, points, quote};
use crate::text_diff::{line_rects, text_of, Piece};
use crate::words::PageText;

/// sizes closer than this count as the same; files round sizes differently.
const SIZE_TOLERANCE: f32 = 0.3;

/// colours closer than this (in any one channel, 0 to 255) count as the same.
const COLOUR_TOLERANCE: u8 = 24;

/// how the style of one word changed. two words with an equal difference
/// belong to the same run.
#[derive(Clone, Debug, Default, PartialEq)]
struct StyleDifference {
    font: Option<(String, String)>,
    size: Option<(f32, f32)>,
    bold: Option<bool>,
    italic: Option<bool>,
    colour: Option<(crate::source::Colour, crate::source::Colour)>,
}

impl StyleDifference {
    fn between(old: &TextStyle, new: &TextStyle) -> Option<StyleDifference> {
        let difference = StyleDifference {
            font: (!old.font.eq_ignore_ascii_case(&new.font))
                .then(|| (old.font.clone(), new.font.clone())),
            size: ((old.size - new.size).abs() >= SIZE_TOLERANCE).then_some((old.size, new.size)),
            bold: (old.bold != new.bold).then_some(new.bold),
            italic: (old.italic != new.italic).then_some(new.italic),
            colour: (old.colour.distance(&new.colour) > COLOUR_TOLERANCE)
                .then_some((old.colour, new.colour)),
        };
        (difference != StyleDifference::default()).then_some(difference)
    }

    /// the difference in words, without the text it applies to.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some((old, new)) = &self.font {
            parts.push(format!("font changed from {old} to {new}"));
        }
        if let Some((old, new)) = self.size {
            parts.push(format!(
                "font size changed from {} to {}",
                points(old),
                points(new)
            ));
        }
        match self.bold {
            Some(true) => parts.push("made bold".to_owned()),
            Some(false) => parts.push("no longer bold".to_owned()),
            None => {}
        }
        match self.italic {
            Some(true) => parts.push("made italic".to_owned()),
            Some(false) => parts.push("no longer italic".to_owned()),
            None => {}
        }
        if let Some((old, new)) = self.colour {
            parts.push(format!(
                "colour changed from {} to {}",
                colour_name(old),
                colour_name(new)
            ));
        }
        capitalised(&join_parts(&parts))
    }
}

/// a run of neighbouring words with the same style difference.
struct Run {
    difference: StyleDifference,
    left: std::ops::Range<usize>,
    right: std::ops::Range<usize>,
}

/// the formatting changes on a page pair, and which old words were restyled
/// (layout uses that: a bigger font explains text moving down).
pub(crate) fn formatting_changes(
    pieces: &[Piece],
    left: &PageText,
    right: &PageText,
    pages: (u32, u32),
) -> (Vec<Change>, Vec<bool>) {
    let mut restyled = vec![false; left.words.len()];
    let mut runs: Vec<Run> = Vec::new();
    for piece in pieces {
        let Piece::Same { left: a, right: b } = piece else {
            continue;
        };
        for (old_index, new_index) in a.clone().zip(b.clone()) {
            let (old, new) = (&left.words[old_index], &right.words[new_index]);
            let Some(difference) = StyleDifference::between(&old.style, &new.style) else {
                continue;
            };
            restyled[old_index] = true;
            let continues = runs.last().is_some_and(|run| {
                run.difference == difference
                    && run.left.end == old_index
                    && run.right.end == new_index
                    && left.words[run.left.start].block == old.block
            });
            if continues {
                let run = runs.last_mut().unwrap();
                run.left.end += 1;
                run.right.end += 1;
            } else {
                runs.push(Run {
                    difference,
                    left: old_index..old_index + 1,
                    right: new_index..new_index + 1,
                });
            }
        }
    }
    let changes = runs
        .iter()
        .map(|run| run_change(run, left, right, pages))
        .collect();
    (changes, restyled)
}

fn run_change(run: &Run, left: &PageText, right: &PageText, pages: (u32, u32)) -> Change {
    let text = text_of(right, &run.right);
    Change {
        kind: ChangeKind::Restyled,
        category: Category::Formatting,
        left_page: Some(pages.0),
        right_page: Some(pages.1),
        left_rects: line_rects(left, &run.left),
        right_rects: line_rects(right, &run.right),
        old_text: Some(text_of(left, &run.left)),
        new_text: Some(text.clone()),
        detail: format!("{} in {}", run.difference.describe(), quote(&text)),
        style: Some(StyleChange {
            old: left.words[run.left.start].style.clone(),
            new: right.words[run.right.start].style.clone(),
        }),
        ..Change::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Colour;

    fn style(size: f32, bold: bool, colour: Colour) -> TextStyle {
        TextStyle {
            font: "Helvetica".into(),
            size,
            bold,
            italic: false,
            colour,
        }
    }

    #[test]
    fn a_size_change_reads_as_a_sentence() {
        let difference = StyleDifference::between(
            &style(11.0, false, Colour::BLACK),
            &style(12.0, false, Colour::BLACK),
        )
        .unwrap();
        assert_eq!(difference.describe(), "Font size changed from 11 to 12");
    }

    #[test]
    fn several_differences_are_joined() {
        let difference = StyleDifference::between(
            &style(11.0, false, Colour::BLACK),
            &style(11.0, true, Colour::new(255, 0, 0)),
        )
        .unwrap();
        assert_eq!(
            difference.describe(),
            "Made bold and colour changed from black to red"
        );
    }

    #[test]
    fn tiny_differences_are_not_changes() {
        assert!(StyleDifference::between(
            &style(11.0, false, Colour::BLACK),
            &style(11.1, false, Colour::new(10, 10, 10))
        )
        .is_none());
    }
}
