//! comparing where unchanged text sits: shifted lines, reflowed paragraphs,
//! margins and page sizes. also builds the scroll map and the offset that
//! lines the two pages up.
//!
//! the hard part is not reporting what is only a consequence of another
//! change. a sentence added to the first paragraph pushes everything below
//! it down a line, and that is not news. so, walking the text in reading
//! order, an edit (or a restyled word, or a reflowed paragraph) "explains"
//! whatever offset the next line has, and that offset becomes the expected
//! one. a line is reported only when it sits away from the expected offset
//! with nothing before it to explain why.

use std::collections::HashSet;

use crate::geometry::{PageSize, Rect};
use crate::result::{Category, Change, ChangeKind, Offset, ScrollPoint};
use crate::sentences::{capitalised, join_parts, points, quote, shorten};
use crate::text_diff::Piece;
use crate::words::PageText;

/// what the layout comparison of one page pair found.
pub(crate) struct LayoutFindings {
    pub changes: Vec<Change>,
    pub scroll_map: Vec<ScrollPoint>,
    /// the typical offset of the new page's text from the old page's, upright
    pub offset: Offset,
}

/// the walk through both pages: matched words, and the events that explain shifts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Event {
    Matched(usize, usize),
    Explained,
}

/// how one old line sits on the new page.
#[derive(Clone, Debug)]
struct LineMeasure {
    left_line: usize,
    right_line: usize,
    /// vertical offsets of its matched words
    dys: Vec<f32>,
    /// the offset of the line's start, when the line starts with the same
    /// word on both pages
    dx: Option<f32>,
    /// visible tops of the first matched word on each page, for the scroll map
    tops: (f32, f32),
}

impl LineMeasure {
    fn dy(&self) -> f32 {
        let mut dys = self.dys.clone();
        dys.sort_by(f32::total_cmp);
        dys[dys.len() / 2]
    }
}

/// lines that sit away from where they were expected, by the same amount.
#[derive(Clone, Debug)]
struct ShiftGroup {
    delta: (f32, f32),
    /// how far from the expected position
    relative: (f32, f32),
    lines: Vec<LineMeasure>,
}

pub(crate) struct LayoutInput<'a> {
    pub pieces: &'a [Piece],
    pub left: &'a PageText,
    pub right: &'a PageText,
    pub restyled: &'a [bool],
    pub pages: (u32, u32),
    pub tolerance: f32,
    pub page_resized: bool,
    pub report: bool,
}

pub(crate) fn compare_layout(input: &LayoutInput) -> LayoutFindings {
    let events = walk(input);
    let reflowed = reflowed_blocks(input, &events);
    let measures = measure_lines(input, &events, &reflowed);
    let mut changes = Vec::new();
    if input.report {
        changes.extend(reflow_changes(input, &reflowed));
        changes.extend(shift_changes(input, &measures));
    }
    LayoutFindings {
        changes,
        scroll_map: scroll_map(input, &measures),
        offset: typical_offset(&measures),
    }
}

fn walk(input: &LayoutInput) -> Vec<Event> {
    let mut events = Vec::new();
    for piece in input.pieces {
        match piece {
            Piece::Same { left, right } => {
                for (a, b) in left.clone().zip(right.clone()) {
                    if input.restyled.get(a).copied().unwrap_or(false) {
                        events.push(Event::Explained);
                    }
                    events.push(Event::Matched(a, b));
                }
            }
            Piece::Changed { .. } | Piece::Spacing { .. } => events.push(Event::Explained),
        }
    }
    events
}

/// a paragraph is reflowed when its words break into lines differently.
/// returns (old block, new block, whether an edit inside explains it).
fn reflowed_blocks(input: &LayoutInput, events: &[Event]) -> Vec<(usize, usize, bool)> {
    let (left, right) = (input.left, input.right);
    let (edited_left, edited_right) = edited_blocks(input);
    let mut found: Vec<(usize, usize, bool)> = Vec::new();
    let matched: Vec<(usize, usize)> = events
        .iter()
        .filter_map(|event| match event {
            Event::Matched(a, b) => Some((*a, *b)),
            Event::Explained => None,
        })
        .collect();
    for pair in matched.windows(2) {
        let ((a1, b1), (a2, b2)) = (pair[0], pair[1]);
        if a2 != a1 + 1 || b2 != b1 + 1 {
            continue;
        }
        let (old1, old2, new1, new2) = (
            &left.words[a1],
            &left.words[a2],
            &right.words[b1],
            &right.words[b2],
        );
        if old1.block != old2.block || new1.block != new2.block {
            continue;
        }
        let breaks_before = old1.line != old2.line;
        let breaks_after = new1.line != new2.line;
        if breaks_before != breaks_after && !found.iter().any(|(block, _, _)| *block == old1.block)
        {
            let explained = edited_left.contains(&old1.block) || edited_right.contains(&new1.block);
            found.push((old1.block, new1.block, explained));
        }
    }
    found
}

/// blocks holding an edit or a restyled word, on each page.
fn edited_blocks(input: &LayoutInput) -> (HashSet<usize>, HashSet<usize>) {
    let (left, right) = (input.left, input.right);
    let mut edited_left = HashSet::new();
    let mut edited_right = HashSet::new();
    let block_near = |page: &PageText, at: usize| {
        page.words
            .get(at)
            .or_else(|| at.checked_sub(1).and_then(|before| page.words.get(before)))
            .map(|word| word.block)
    };
    for piece in input.pieces {
        if let Piece::Changed { left: a, right: b } | Piece::Spacing { left: a, right: b } = piece {
            edited_left.extend(left.words[a.clone()].iter().map(|word| word.block));
            edited_right.extend(right.words[b.clone()].iter().map(|word| word.block));
            // an addition still changes the paragraph it went into on the old page
            edited_left.extend(block_near(left, a.start));
            edited_right.extend(block_near(right, b.start));
        }
    }
    for (index, restyled) in input.restyled.iter().enumerate() {
        if *restyled {
            edited_left.insert(left.words[index].block);
        }
    }
    for piece in input.pieces {
        if let Piece::Same { left: a, right: b } = piece {
            for (old, new) in a.clone().zip(b.clone()) {
                // a restyled word's new block, found through the matched pieces
                let restyled = input.restyled.get(old).copied().unwrap_or(false);
                // a word hyphenated on one page only breaks its lines differently by nature
                let hyphenated_once = left.words[old].rects.len() != right.words[new].rects.len();
                if restyled || hyphenated_once {
                    edited_left.insert(left.words[old].block);
                    edited_right.insert(right.words[new].block);
                }
            }
        }
    }
    (edited_left, edited_right)
}

/// measures each old line's offset, line by line in reading order. a
/// line in a reflowed block is not measured, and counts as an explanation.
fn measure_lines(
    input: &LayoutInput,
    events: &[Event],
    reflowed: &[(usize, usize, bool)],
) -> Vec<Option<LineMeasure>> {
    let (left, right) = (input.left, input.right);
    let reflowed_left: HashSet<usize> = reflowed.iter().map(|(block, _, _)| *block).collect();
    // None in the list marks an explanation between lines
    let mut measures: Vec<Option<LineMeasure>> = Vec::new();
    let mut current: Option<LineMeasure> = None;
    for event in events {
        let (a, b) = match *event {
            Event::Explained => {
                measures.extend(current.take().map(Some));
                measures.push(None);
                continue;
            }
            Event::Matched(a, b) => (a, b),
        };
        let (old, new) = (&left.words[a], &right.words[b]);
        if reflowed_left.contains(&old.block) {
            measures.extend(current.take().map(Some));
            // one explanation is enough for a whole reflowed paragraph
            if !matches!(measures.last(), Some(None)) {
                measures.push(None);
            }
            continue;
        }
        if current
            .as_ref()
            .is_some_and(|line| line.left_line != old.line)
        {
            measures.extend(current.take().map(Some));
        }
        let line = current.get_or_insert_with(|| LineMeasure {
            left_line: old.line,
            right_line: new.line,
            dys: Vec::new(),
            dx: None,
            tops: (old.rects[0].y, new.rects[0].y),
        });
        line.dys.push(new.upright.bottom() - old.upright.bottom());
        let starts_both_lines =
            left.lines[old.line].words.start == a && right.lines[new.line].words.start == b;
        if starts_both_lines {
            line.dx = Some(new.upright.x - old.upright.x);
        }
    }
    measures.extend(current.take().map(Some));
    measures
}

/// groups lines that sit away from the expected offset.
fn shift_groups(input: &LayoutInput, measures: &[Option<LineMeasure>]) -> Vec<ShiftGroup> {
    let tolerance = input.tolerance;
    let close = |a: (f32, f32), b: (f32, f32)| {
        (a.0 - b.0).abs() <= tolerance && (a.1 - b.1).abs() <= tolerance
    };
    // a resized page moves everything; its first line sets what is expected
    let mut expected: Option<(f32, f32)> = (!input.page_resized).then_some((0.0, 0.0));
    // the last known horizontal offset, for lines whose start can't be measured
    let mut known_dx = 0.0;
    let mut groups: Vec<ShiftGroup> = Vec::new();
    let mut open = false;
    for measure in measures {
        let Some(line) = measure else {
            if let Some((dx, _)) = expected {
                known_dx = dx;
            }
            expected = None;
            open = false;
            continue;
        };
        let Some(wanted) = expected else {
            expected = Some((line.dx.unwrap_or(known_dx), line.dy()));
            open = false;
            continue;
        };
        let delta = (line.dx.unwrap_or(wanted.0), line.dy());
        if close(delta, wanted) {
            open = false;
            continue;
        }
        match groups.last_mut() {
            Some(group) if open && close(group.delta, delta) => group.lines.push(line.clone()),
            _ => {
                groups.push(ShiftGroup {
                    delta,
                    relative: (delta.0 - wanted.0, delta.1 - wanted.1),
                    lines: vec![line.clone()],
                });
                open = true;
            }
        }
    }
    groups
}

fn shift_changes(input: &LayoutInput, measures: &[Option<LineMeasure>]) -> Vec<Change> {
    let measured_lines = measures.iter().flatten().count();
    shift_groups(input, measures)
        .into_iter()
        .map(|group| shift_change(input, &group, measured_lines))
        .collect()
}

fn shift_change(input: &LayoutInput, group: &ShiftGroup, measured_lines: usize) -> Change {
    let (left, right) = (input.left, input.right);
    let left_lines: Vec<usize> = group.lines.iter().map(|line| line.left_line).collect();
    let right_lines: Vec<usize> = group.lines.iter().map(|line| line.right_line).collect();
    let first_text = line_text(left, left_lines[0]);
    let whole_page = group.lines.len() == measured_lines && measured_lines > 1;
    let tolerance = input.tolerance;
    let detail = if whole_page && group.relative.1.abs() <= tolerance {
        let old_margin = left_lines
            .iter()
            .map(|&line| left.lines[line].upright.x)
            .fold(f32::MAX, f32::min);
        format!(
            "Left margin changed from {} to {} pt",
            points(old_margin),
            points(old_margin + group.relative.0)
        )
    } else {
        let what = match (whole_page, group.lines.len()) {
            (true, _) => "All text".to_owned(),
            (false, 1) => "Line".to_owned(),
            (false, count) => format!("{count} lines"),
        };
        format!(
            "{what} moved {} ({})",
            direction_words(group.relative, tolerance),
            quote(&first_text)
        )
    };
    Change {
        kind: ChangeKind::Shifted,
        category: Category::Layout,
        left_page: Some(input.pages.0),
        right_page: Some(input.pages.1),
        left_rects: block_rects(left, &left_lines),
        right_rects: block_rects(right, &right_lines),
        old_text: Some(first_text.clone()),
        new_text: Some(line_text(right, right_lines[0])),
        detail,
        ..Change::default()
    }
}

/// "down 12 pt", "right 36 pt and up 4 pt".
fn direction_words(relative: (f32, f32), tolerance: f32) -> String {
    let mut parts = Vec::new();
    if relative.0.abs() > tolerance {
        let way = if relative.0 > 0.0 { "right" } else { "left" };
        parts.push(format!("{way} {} pt", points(relative.0.abs())));
    }
    if relative.1.abs() > tolerance {
        let way = if relative.1 > 0.0 { "down" } else { "up" };
        parts.push(format!("{way} {} pt", points(relative.1.abs())));
    }
    join_parts(&parts)
}

/// one rectangle per block for a set of lines, on the visible page.
fn block_rects(page: &PageText, lines: &[usize]) -> Vec<Rect> {
    let mut by_block: Vec<(usize, Rect)> = Vec::new();
    for &line in lines {
        let line = &page.lines[line];
        match by_block.iter_mut().find(|(block, _)| *block == line.block) {
            Some((_, rect)) => *rect = rect.union(&line.visible),
            None => by_block.push((line.block, line.visible)),
        }
    }
    by_block.into_iter().map(|(_, rect)| rect).collect()
}

fn line_text(page: &PageText, line: usize) -> String {
    let words = &page.words[page.lines[line].words.clone()];
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    shorten(&text, 40)
}

fn reflow_changes(input: &LayoutInput, reflowed: &[(usize, usize, bool)]) -> Vec<Change> {
    reflowed
        .iter()
        .filter(|(_, _, explained)| !explained)
        .map(|&(old_block, new_block, _)| {
            let (left, right) = (input.left, input.right);
            let opening = block_opening(left, old_block);
            Change {
                kind: ChangeKind::Reflowed,
                category: Category::Layout,
                left_page: Some(input.pages.0),
                right_page: Some(input.pages.1),
                left_rects: vec![left.blocks[old_block].visible],
                right_rects: vec![right.blocks[new_block].visible],
                old_text: Some(opening.clone()),
                new_text: Some(block_opening(right, new_block)),
                detail: capitalised(&format!(
                    "paragraph reflowed with different line breaks ({})",
                    quote(&opening)
                )),
                ..Change::default()
            }
        })
        .collect()
}

fn block_opening(page: &PageText, block: usize) -> String {
    let lines = page.blocks[block].lines.clone();
    let first_word = page.lines[lines.start].words.start;
    let words: Vec<&str> = page.words[first_word..]
        .iter()
        .take_while(|word| word.block == block)
        .take(6)
        .map(|word| word.text.as_str())
        .collect();
    words.join(" ")
}

/// matching heights for side by side scrolling, always going down on both pages.
fn scroll_map(input: &LayoutInput, measures: &[Option<LineMeasure>]) -> Vec<ScrollPoint> {
    let mut points: Vec<ScrollPoint> = measures
        .iter()
        .flatten()
        .map(|line| ScrollPoint {
            left_y: line.tops.0,
            right_y: line.tops.1,
        })
        .collect();
    points.sort_by(|a, b| a.left_y.total_cmp(&b.left_y));
    let mut map = vec![ScrollPoint::default()];
    for point in points {
        let last = map[map.len() - 1];
        if point.left_y > last.left_y && point.right_y >= last.right_y {
            map.push(point);
        }
    }
    let ends = ScrollPoint {
        left_y: input.left.size.height,
        right_y: input.right.size.height,
    };
    if ends.left_y > map[map.len() - 1].left_y && ends.right_y >= map[map.len() - 1].right_y {
        map.push(ends);
    }
    map
}

/// the middle offset of all measured lines, so one moved line doesn't
/// change how the pages are lined up.
fn typical_offset(measures: &[Option<LineMeasure>]) -> Offset {
    let lines: Vec<&LineMeasure> = measures.iter().flatten().collect();
    if lines.is_empty() {
        return Offset::default();
    }
    let mut dys: Vec<f32> = lines.iter().map(|line| line.dy()).collect();
    let mut dxs: Vec<f32> = lines.iter().filter_map(|line| line.dx).collect();
    dys.sort_by(f32::total_cmp);
    dxs.sort_by(f32::total_cmp);
    Offset {
        dx: dxs.get(dxs.len() / 2).copied().unwrap_or(0.0),
        dy: dys[dys.len() / 2],
    }
}

/// a sentence for a page that changed size or turned, or none when it didn't.
pub(crate) fn page_size_change(old: PageSize, new: PageSize, pages: (u32, u32)) -> Option<Change> {
    let same = (old.width - new.width).abs() < 1.0 && (old.height - new.height).abs() < 1.0;
    if same {
        return None;
    }
    let size = |page: PageSize| format!("{} × {} pt", points(page.width), points(page.height));
    let detail = if old.is_landscape() != new.is_landscape() {
        let orientation = |page: PageSize| {
            if page.is_landscape() {
                "landscape"
            } else {
                "portrait"
            }
        };
        format!(
            "Page turned from {} to {} ({} to {})",
            orientation(old),
            orientation(new),
            size(old),
            size(new)
        )
    } else {
        format!("Page size changed from {} to {}", size(old), size(new))
    };
    Some(Change {
        kind: ChangeKind::Resized,
        category: Category::Layout,
        left_page: Some(pages.0),
        right_page: Some(pages.1),
        left_rects: vec![old.full_page()],
        right_rects: vec![new.full_page()],
        detail,
        ..Change::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_read_naturally() {
        assert_eq!(direction_words((0.0, 12.0), 2.0), "down 12 pt");
        assert_eq!(
            direction_words((-36.0, -4.0), 2.0),
            "left 36 pt and up 4 pt"
        );
    }

    #[test]
    fn turning_a_page_is_named() {
        let change = page_size_change(
            PageSize::new(612.0, 792.0),
            PageSize::new(792.0, 612.0),
            (0, 0),
        )
        .unwrap();
        assert_eq!(
            change.detail,
            "Page turned from portrait to landscape (612 × 792 pt to 792 × 612 pt)"
        );
        assert!(page_size_change(
            PageSize::new(612.0, 792.0),
            PageSize::new(612.4, 792.0),
            (0, 0)
        )
        .is_none());
    }
}
