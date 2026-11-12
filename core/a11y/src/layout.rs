//! the order a person reads a page in, worked out from where things are.
//!
//! this uses the "x-y cut" method: look for a clear vertical gap that
//! splits everything into a left part and a right part (a gutter between
//! columns), or else the widest horizontal gap that splits it into a top
//! and a bottom part, and repeat inside each part. reading the parts
//! left before right and top before bottom gives column-aware order:
//! a full-width title first, then the left column, then the right one.

use crate::geometry::Rect;

/// columns need a gutter at least this wide, in points, to be split.
const COLUMN_GAP: f32 = 10.0;

/// parts above one another need at least this much space between them.
const ROW_GAP: f32 = 0.5;

/// cuts deeper than this stop and read the rest top to bottom.
const DEPTH_LIMIT: usize = 64;

/// the reading order of some boxes on a page, as indexes into `boxes`.
pub fn reading_order(boxes: &[Rect]) -> Vec<usize> {
    let mut order = Vec::with_capacity(boxes.len());
    cut((0..boxes.len()).collect(), boxes, 0, &mut order);
    order
}

fn cut(group: Vec<usize>, boxes: &[Rect], depth: usize, order: &mut Vec<usize>) {
    if group.len() <= 1 || depth >= DEPTH_LIMIT {
        order.extend(top_to_bottom(group, boxes));
        return;
    }
    let vertical = widest_gap(&group, |i| (boxes[i].x, boxes[i].right()));
    if let Some((gap, split)) = vertical {
        if gap >= COLUMN_GAP {
            let (left, right): (Vec<usize>, Vec<usize>) =
                group.iter().partition(|&&i| boxes[i].center().0 < split);
            if !left.is_empty() && !right.is_empty() {
                cut(left, boxes, depth + 1, order);
                cut(right, boxes, depth + 1, order);
                return;
            }
        }
    }
    let horizontal = widest_gap(&group, |i| (boxes[i].y, boxes[i].bottom()));
    if let Some((gap, split)) = horizontal {
        if gap >= ROW_GAP {
            let (top, bottom): (Vec<usize>, Vec<usize>) =
                group.iter().partition(|&&i| boxes[i].center().1 < split);
            if !top.is_empty() && !bottom.is_empty() {
                cut(top, boxes, depth + 1, order);
                cut(bottom, boxes, depth + 1, order);
                return;
            }
        }
    }
    order.extend(top_to_bottom(group, boxes));
}

/// boxes that can't be cut apart (they overlap), read by their top edge,
/// then left to right.
fn top_to_bottom(mut group: Vec<usize>, boxes: &[Rect]) -> Vec<usize> {
    group.sort_by(|&a, &b| {
        boxes[a]
            .y
            .total_cmp(&boxes[b].y)
            .then(boxes[a].x.total_cmp(&boxes[b].x))
    });
    group
}

/// the widest empty band in the boxes' projection on one axis, as (width
/// of the gap, the middle of the gap).
fn widest_gap(group: &[usize], span: impl Fn(usize) -> (f32, f32)) -> Option<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = group.iter().map(|&i| span(i)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut reach = spans.first()?.1;
    let mut best: Option<(f32, f32)> = None;
    for &(start, end) in &spans[1..] {
        let gap = start - reach;
        if gap > 0.0 && best.is_none_or(|(widest, _)| gap > widest) {
            best = Some((gap, reach + gap / 2.0));
        }
        reach = reach.max(end);
    }
    best
}

/// how often reading in the `first` order jumps around in the `second`
/// order: the share of steps from one item to the next that don't go to
/// the very next item of `second`, and the number of such steps. reading
/// two columns row by row breaks almost every step, while one misplaced
/// caption breaks only two or three.
pub fn broken_steps(first: &[usize], second: &[usize]) -> (f32, usize) {
    let position: std::collections::HashMap<usize, usize> = second
        .iter()
        .enumerate()
        .map(|(place, &item)| (item, place))
        .collect();
    let ranks: Vec<usize> = first
        .iter()
        .filter_map(|item| position.get(item).copied())
        .collect();
    if ranks.len() < 2 {
        return (0.0, 0);
    }
    let broken = ranks
        .windows(2)
        .filter(|pair| pair[1] != pair[0] + 1)
        .count();
    (broken as f32 / (ranks.len() - 1) as f32, broken)
}

/// how differently two orders arrange the same items: the share of
/// pairs that come in the opposite order (0 means the same order, 1 means
/// exactly reversed). items in only one of the lists are ignored.
#[cfg(test)]
pub fn disagreement(first: &[usize], second: &[usize]) -> f32 {
    let position: std::collections::HashMap<usize, usize> = second
        .iter()
        .enumerate()
        .map(|(place, &item)| (item, place))
        .collect();
    let ranks: Vec<usize> = first
        .iter()
        .filter_map(|item| position.get(item).copied())
        .collect();
    let pairs = ranks.len() * ranks.len().saturating_sub(1) / 2;
    if pairs == 0 {
        return 0.0;
    }
    let mut swapped = 0;
    for (index, a) in ranks.iter().enumerate() {
        swapped += ranks[index + 1..].iter().filter(|b| *b < a).count();
    }
    swapped as f32 / pairs as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, width: f32) -> Rect {
        Rect::new(x, y, width, 10.0)
    }

    #[test]
    fn one_column_reads_top_to_bottom() {
        let boxes = [
            line(50.0, 100.0, 400.0),
            line(50.0, 60.0, 400.0),
            line(50.0, 80.0, 400.0),
        ];
        assert_eq!(reading_order(&boxes), vec![1, 2, 0]);
    }

    #[test]
    fn two_columns_read_one_column_at_a_time() {
        // a title across the page, then two columns with lines side by side
        let mut boxes = vec![line(50.0, 40.0, 500.0)];
        for row in 0..3 {
            let y = 80.0 + row as f32 * 14.0;
            boxes.push(line(50.0, y, 230.0));
            boxes.push(line(320.0, y, 230.0));
        }
        let order = reading_order(&boxes);
        assert_eq!(order, vec![0, 1, 3, 5, 2, 4, 6]);
    }

    #[test]
    fn reading_across_columns_breaks_most_steps() {
        let (share, count) = broken_steps(&[0, 1, 4, 2, 5, 3, 6], &[0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(count, 5);
        assert!(share > 0.8);
        assert_eq!(broken_steps(&[0, 1, 2], &[0, 1, 2]), (0.0, 0));
    }

    #[test]
    fn disagreement_counts_swapped_pairs() {
        assert_eq!(disagreement(&[0, 1, 2, 3], &[0, 1, 2, 3]), 0.0);
        assert_eq!(disagreement(&[0, 1, 2, 3], &[3, 2, 1, 0]), 1.0);
        assert!((disagreement(&[0, 1, 2], &[1, 0, 2]) - 1.0 / 3.0).abs() < 0.001);
    }
}
