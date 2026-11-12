//! paragraph detection on the planned lines of a page.
//!
//! the planned lines are round one's (recipe 5): one per run of glyphs on
//! a baseline. consecutive lines become one paragraph when they look like
//! the lines of one paragraph set by a word processor: the same font and
//! size, a steady line pitch, the same left edge (the first line may be
//! indented or hanging), and every break where a greedy line breaker would
//! have put it (the next line's first word did not fit on the line).
//! everything here is pure, so it is tested without word.

use serde::{Deserialize, Serialize};

/// a line start counts as the same when within this distance: word-made
/// pdfs put line starts on 0.12 pt pixels, so a pixel and a little slack.
pub const SAME_EDGE_PT: f64 = 0.15;
/// a steady pitch varies by at most one pixel (word sources quantise
/// baselines to 0.12 pt pixels, so their pitch alternates by a pixel).
pub const PITCH_TOLERANCE_PT: f64 = 0.15;
/// the pitch of a paragraph's lines, in ems of its size.
pub const PITCH_RANGE_EM: (f64, f64) = (0.95, 2.2);
/// a first-line indent or hanging indent is at most this many ems.
pub const MAX_INDENT_EM: f64 = 6.0;
/// lines of a justified paragraph end within this of the same right edge.
pub const JUSTIFIED_EDGE_PT: f64 = 0.3;

/// what paragraph detection needs to know about one planned line.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSummary {
    /// pdf x of the line start, points
    pub x: f64,
    /// pdf x of the end of the last glyph
    pub right: f64,
    /// pdf y of the baseline (points up from the page bottom)
    pub y: f64,
    pub size: f64,
    /// the grid family most of the line's glyphs use
    pub family: String,
    /// width of the line's first word, points
    pub first_word: f64,
    /// width of a space in the line's font, points
    pub space: f64,
    /// some space is wider than the font's: the source stretched the line
    pub stretched: bool,
    pub ends_with_hyphen: bool,
    /// starts with a list marker (a bullet, `1.`, `a)`): a list item is a paragraph of its own
    pub starts_with_marker: bool,
    /// rotated, hidden, or otherwise never part of a paragraph
    pub excluded: bool,
}

/// lines whose left edge is this close belong to the same column when the
/// column's right edge is looked for.
pub const COLUMN_LEFT_PT: f64 = 24.0;

/// the right edge of the column of line `index`: the widest line of the
/// page that starts near it (lines of other columns start far away).
pub fn column_right(lines: &[LineSummary], index: usize) -> f64 {
    let line = &lines[index];
    lines
        .iter()
        .filter(|other| !other.excluded && (other.x - line.x).abs() <= COLUMN_LEFT_PT)
        .map(|other| other.right)
        .fold(line.right, f64::max)
}

/// a bullet, or a number or letter followed by a full stop or a bracket.
pub fn starts_with_marker(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if matches!(
        first,
        '\u{2022}'
            | '\u{25E6}'
            | '\u{25AA}'
            | '\u{25CF}'
            | '\u{2013}'
            | '\u{00B7}'
            | '\u{F0B7}'
            | '*'
    ) {
        return true;
    }
    let head: String = text.chars().take_while(|c| !c.is_whitespace()).collect();
    let Some(last) = head.chars().last() else {
        return false;
    };
    let body = &head[..head.len() - last.len_utf8()];
    matches!(last, '.' | ')')
        && !body.is_empty()
        && ((body.len() <= 3 && body.chars().all(|c| c.is_ascii_digit()))
            || (body.len() == 1 && body.chars().all(|c| c.is_ascii_lowercase())))
}

/// how a paragraph's lines sit between its edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    Left,
    Justified,
}

/// consecutive planned lines that form one paragraph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedParagraph {
    /// indexes into the page's lines, in order
    pub lines: Vec<usize>,
    pub alignment: Alignment,
    /// the mean baseline pitch in points (0 for a single line)
    pub pitch: f64,
}

/// groups a page's lines into paragraphs. excluded lines are left out
/// (they stay positioned lines); every other line is in exactly one
/// paragraph, possibly of one line.
pub fn detect_paragraphs(lines: &[LineSummary]) -> Vec<DetectedParagraph> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.excluded {
            continue;
        }
        let joins = groups.last().is_some_and(|group| {
            *group.last().unwrap() + 1 == index && continues(lines, group, index)
        });
        if joins {
            groups.last_mut().unwrap().push(index);
        } else {
            groups.push(vec![index]);
        }
    }
    groups
        .into_iter()
        .map(|group| DetectedParagraph {
            alignment: alignment_of(lines, &group),
            pitch: mean_pitch(lines, &group),
            lines: group,
        })
        .collect()
}

/// does line `next` continue the paragraph `group`?
fn continues(lines: &[LineSummary], group: &[usize], next: usize) -> bool {
    let last = &lines[*group.last().unwrap()];
    let line = &lines[next];
    if line.starts_with_marker {
        return false;
    }
    if line.family != last.family || (line.size - last.size).abs() > 0.01 {
        return false;
    }
    let pitch = last.y - line.y;
    let size = line.size;
    if pitch < PITCH_RANGE_EM.0 * size || pitch > PITCH_RANGE_EM.1 * size {
        return false;
    }
    if group.len() >= 2 {
        let first_pitch = lines[group[0]].y - lines[group[1]].y;
        if (pitch - first_pitch).abs() > PITCH_TOLERANCE_PT {
            return false;
        }
    }
    if !same_left_edge(lines, group, line) {
        return false;
    }
    !broke_early(lines, group, line)
}

/// the second line sets the paragraph's left edge (the first may be
/// indented or hanging); every later line must start there.
fn same_left_edge(lines: &[LineSummary], group: &[usize], line: &LineSummary) -> bool {
    if group.len() == 1 {
        let first = &lines[group[0]];
        return (first.x - line.x).abs() <= MAX_INDENT_EM * line.size;
    }
    (lines[group[1]].x - line.x).abs() <= SAME_EDGE_PT
}

/// the previous line ended although the next line's first word would have
/// fitted on it: a paragraph ends there (a greedy breaker never breaks
/// early). the column's right edge is the widest line that starts near it.
fn broke_early(lines: &[LineSummary], group: &[usize], line: &LineSummary) -> bool {
    let last_index = *group.last().unwrap();
    let last = &lines[last_index];
    let column_right = column_right(lines, last_index).max(line.right);
    let joiner = if last.ends_with_hyphen {
        0.0
    } else {
        last.space
    };
    last.right + joiner + line.first_word < column_right - SAME_EDGE_PT
}

fn mean_pitch(lines: &[LineSummary], group: &[usize]) -> f64 {
    if group.len() < 2 {
        return 0.0;
    }
    (lines[group[0]].y - lines[*group.last().unwrap()].y) / (group.len() - 1) as f64
}

/// justified: every line but the last ends at the same right edge, and the
/// source stretched some space to get there.
fn alignment_of(lines: &[LineSummary], group: &[usize]) -> Alignment {
    if group.len() < 2 {
        return Alignment::Left;
    }
    let body = &group[..group.len() - 1];
    let right = body
        .iter()
        .map(|&i| lines[i].right)
        .fold(f64::MIN, f64::max);
    let flush = body
        .iter()
        .all(|&i| right - lines[i].right <= JUSTIFIED_EDGE_PT);
    let stretched = body.iter().any(|&i| lines[i].stretched);
    if flush && stretched {
        Alignment::Justified
    } else {
        Alignment::Left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f64, right: f64, y: f64) -> LineSummary {
        LineSummary {
            x,
            right,
            y,
            size: 10.0,
            family: "Ltr 1 10".into(),
            first_word: 20.0,
            space: 2.5,
            stretched: false,
            ends_with_hyphen: false,
            starts_with_marker: false,
            excluded: false,
        }
    }

    fn shapes(lines: &[LineSummary]) -> Vec<Vec<usize>> {
        detect_paragraphs(lines)
            .into_iter()
            .map(|p| p.lines)
            .collect()
    }

    #[test]
    fn a_steady_column_is_one_paragraph() {
        let lines = [
            line(72.0, 300.0, 700.0),
            line(72.0, 298.0, 688.0),
            line(72.0, 150.0, 676.0),
        ];
        let found = detect_paragraphs(&lines);
        assert_eq!(found.len(), 1);
        assert!((found[0].pitch - 12.0).abs() < 1e-9);
        assert_eq!(found[0].alignment, Alignment::Left);
    }

    #[test]
    fn a_short_line_followed_by_a_fitting_word_ends_the_paragraph() {
        // the second line ends at 150, and the third line's 20 pt word would have fitted
        let lines = [
            line(72.0, 300.0, 700.0),
            line(72.0, 150.0, 688.0),
            line(72.0, 299.0, 676.0),
        ];
        assert_eq!(shapes(&lines), [vec![0, 1], vec![2]]);
    }

    #[test]
    fn first_line_indent_and_hanging_indent_are_kept() {
        let indented = [
            line(90.0, 300.0, 700.0),
            line(72.0, 300.0, 688.0),
            line(72.0, 200.0, 676.0),
        ];
        assert_eq!(shapes(&indented), [vec![0, 1, 2]]);
        // a new indented line after two body lines starts a new paragraph
        let next = [
            line(90.0, 300.0, 700.0),
            line(72.0, 300.0, 688.0),
            line(90.0, 300.0, 676.0),
        ];
        assert_eq!(shapes(&next), [vec![0, 1], vec![2]]);
    }

    #[test]
    fn a_change_of_pitch_font_or_gap_splits() {
        let mut lines = vec![
            line(72.0, 300.0, 700.0),
            line(72.0, 300.0, 688.0),
            line(72.0, 300.0, 670.0),
        ];
        assert_eq!(shapes(&lines), [vec![0, 1], vec![2]]);
        lines[2].y = 676.0;
        lines[2].family = "Ltr 2 10".into();
        assert_eq!(shapes(&lines), [vec![0, 1], vec![2]]);
        lines[2].family = lines[0].family.clone();
        lines[1].excluded = true;
        assert_eq!(shapes(&lines), [vec![0], vec![2]]);
    }

    #[test]
    fn list_items_are_paragraphs_of_their_own() {
        // a wide line above sets the column's right edge; two short items below it
        let mut lines = vec![
            line(72.0, 500.0, 712.0),
            line(72.0, 250.0, 700.0),
            line(72.0, 230.0, 688.0),
        ];
        lines[2].first_word = 10.0;
        assert_eq!(shapes(&lines), [vec![0, 1], vec![2]]);
        lines[1].first_word = 300.0;
        lines[2].starts_with_marker = true;
        assert_eq!(shapes(&lines), [vec![0, 1], vec![2]]);
    }

    #[test]
    fn markers_are_bullets_numbers_and_single_letters() {
        for text in ["\u{2022} Weekly", "1. Lab", "12) Item", "a) first"] {
            assert!(starts_with_marker(text), "{text}");
        }
        for text in ["PHYS 201", "10:30 to", "2.1 Room", "etc. and", "Figure 2."] {
            assert!(!starts_with_marker(text), "{text}");
        }
    }

    #[test]
    fn flush_stretched_lines_are_justified() {
        let mut lines = vec![
            line(72.0, 300.0, 700.0),
            line(72.0, 299.9, 688.0),
            line(72.0, 150.0, 676.0),
        ];
        lines[0].stretched = true;
        assert_eq!(detect_paragraphs(&lines)[0].alignment, Alignment::Justified);
        lines[1].right = 296.0;
        lines[2].first_word = 2.0;
        assert_eq!(detect_paragraphs(&lines)[0].alignment, Alignment::Left);
    }
}
