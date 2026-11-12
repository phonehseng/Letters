//! finds ruled tables: grids of lines drawn on the page.
//!
//! this is deliberately conservative. only a full grid counts: at least
//! three horizontal rules and three vertical rules (two rows, two
//! columns), every rule running across the whole table. tables drawn
//! with spaces or partial lines are left as paragraphs, because tagging
//! text as a table by mistake is worse for a screen reader user than
//! missing one.

use crate::content::{PathUse, Segment};
use crate::geometry::Rect;

/// lines this close together count as one rule, in points.
const SAME_LINE: f32 = 1.5;

/// a rule must run across at least this share of the table.
const FULL_SPAN: f32 = 0.9;

/// a detected table: the positions of its rules on the visible page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Grid {
    /// x positions of the vertical rules, left to right
    pub columns: Vec<f32>,
    /// y positions of the horizontal rules, top to bottom
    pub rows: Vec<f32>,
}

impl Grid {
    pub fn rect(&self) -> Rect {
        let (left, right) = (self.columns[0], *self.columns.last().unwrap());
        let (top, bottom) = (self.rows[0], *self.rows.last().unwrap());
        Rect::new(left, top, right - left, bottom - top)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len() - 1
    }

    pub fn column_count(&self) -> usize {
        self.columns.len() - 1
    }

    /// the cell (row, column) a point is in, if it is inside the grid.
    pub fn cell_at(&self, (x, y): (f32, f32)) -> Option<(usize, usize)> {
        let column = self
            .columns
            .windows(2)
            .position(|pair| x >= pair[0] && x < pair[1])?;
        let row = self
            .rows
            .windows(2)
            .position(|pair| y >= pair[0] && y < pair[1])?;
        Some((row, column))
    }
}

/// a straight rule: horizontal (at y, from x0 to x1) or vertical.
#[derive(Clone, Copy, Debug)]
struct Rule {
    at: f32,
    from: f32,
    to: f32,
}

/// the ruled tables among the page's shapes.
pub(crate) fn find_grids(paths: &[PathUse]) -> Vec<Grid> {
    let segments: Vec<&Segment> = paths.iter().flat_map(|path| path.segments.iter()).collect();
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    for s in segments {
        let (dx, dy) = ((s.x1 - s.x0).abs(), (s.y1 - s.y0).abs());
        if dy < 0.5 && dx > 10.0 {
            horizontal.push(Rule {
                at: (s.y0 + s.y1) / 2.0,
                from: s.x0.min(s.x1),
                to: s.x0.max(s.x1),
            });
        } else if dx < 0.5 && dy > 6.0 {
            vertical.push(Rule {
                at: (s.x0 + s.x1) / 2.0,
                from: s.y0.min(s.y1),
                to: s.y0.max(s.y1),
            });
        }
    }
    let horizontal = merge(horizontal);
    let vertical = merge(vertical);
    let mut grids = Vec::new();
    for group in connected(&horizontal, &vertical) {
        if let Some(grid) = full_grid(&group.0, &group.1) {
            grids.push(grid);
        }
    }
    grids
}

/// rules at the same position and touching become one longer rule; this
/// joins the many short edges that cell borders are often drawn with.
fn merge(mut rules: Vec<Rule>) -> Vec<Rule> {
    rules.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.from.total_cmp(&b.from)));
    let mut merged: Vec<Rule> = Vec::new();
    for rule in rules {
        match merged
            .iter_mut()
            .rev()
            .find(|m| (m.at - rule.at).abs() <= SAME_LINE && rule.from <= m.to + SAME_LINE)
        {
            Some(existing) => {
                existing.to = existing.to.max(rule.to);
                existing.from = existing.from.min(rule.from);
            }
            None => merged.push(rule),
        }
    }
    merged
}

fn crosses(h: &Rule, v: &Rule) -> bool {
    v.at >= h.from - SAME_LINE
        && v.at <= h.to + SAME_LINE
        && h.at >= v.from - SAME_LINE
        && h.at <= v.to + SAME_LINE
}

/// groups of rules that touch each other, each a possible table.
fn connected(horizontal: &[Rule], vertical: &[Rule]) -> Vec<(Vec<Rule>, Vec<Rule>)> {
    let mut group_of_h: Vec<Option<usize>> = vec![None; horizontal.len()];
    let mut group_of_v: Vec<Option<usize>> = vec![None; vertical.len()];
    let mut groups = 0;
    for start in 0..horizontal.len() {
        if group_of_h[start].is_some() {
            continue;
        }
        group_of_h[start] = Some(groups);
        let mut queue = vec![(true, start)];
        while let Some((is_h, index)) = queue.pop() {
            if is_h {
                for (v, slot) in group_of_v.iter_mut().enumerate() {
                    if slot.is_none() && crosses(&horizontal[index], &vertical[v]) {
                        *slot = Some(groups);
                        queue.push((false, v));
                    }
                }
            } else {
                for (h, slot) in group_of_h.iter_mut().enumerate() {
                    if slot.is_none() && crosses(&horizontal[h], &vertical[index]) {
                        *slot = Some(groups);
                        queue.push((true, h));
                    }
                }
            }
        }
        groups += 1;
    }
    (0..groups)
        .map(|g| {
            let h = horizontal
                .iter()
                .zip(&group_of_h)
                .filter(|(_, s)| **s == Some(g))
                .map(|(r, _)| *r)
                .collect();
            let v = vertical
                .iter()
                .zip(&group_of_v)
                .filter(|(_, s)| **s == Some(g))
                .map(|(r, _)| *r)
                .collect();
            (h, v)
        })
        .collect()
}

/// a grid when every rule spans the whole group and there are enough.
fn full_grid(horizontal: &[Rule], vertical: &[Rule]) -> Option<Grid> {
    if horizontal.len() < 3 || vertical.len() < 3 {
        return None;
    }
    let left = vertical.iter().map(|r| r.at).fold(f32::INFINITY, f32::min);
    let right = vertical
        .iter()
        .map(|r| r.at)
        .fold(f32::NEG_INFINITY, f32::max);
    let top = horizontal
        .iter()
        .map(|r| r.at)
        .fold(f32::INFINITY, f32::min);
    let bottom = horizontal
        .iter()
        .map(|r| r.at)
        .fold(f32::NEG_INFINITY, f32::max);
    let (width, height) = (right - left, bottom - top);
    if width < 20.0 || height < 10.0 {
        return None;
    }
    let spans_width = horizontal
        .iter()
        .all(|r| (r.to.min(right) - r.from.max(left)) >= FULL_SPAN * width);
    let spans_height = vertical
        .iter()
        .all(|r| (r.to.min(bottom) - r.from.max(top)) >= FULL_SPAN * height);
    if !spans_width || !spans_height {
        return None;
    }
    let mut columns: Vec<f32> = vertical.iter().map(|r| r.at).collect();
    let mut rows: Vec<f32> = horizontal.iter().map(|r| r.at).collect();
    columns.sort_by(f32::total_cmp);
    rows.sort_by(f32::total_cmp);
    columns.dedup_by(|a, b| (*a - *b).abs() <= SAME_LINE);
    rows.dedup_by(|a, b| (*a - *b).abs() <= SAME_LINE);
    (columns.len() >= 3 && rows.len() >= 3).then_some(Grid { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Mark;

    fn path(segments: Vec<Segment>) -> PathUse {
        PathUse {
            op_start: 0,
            op_end: 0,
            in_form: false,
            rect: Rect::default(),
            mark: Mark::Untagged,
            segments,
        }
    }

    fn h(y: f32, x0: f32, x1: f32) -> Segment {
        Segment {
            x0,
            y0: y,
            x1,
            y1: y,
        }
    }

    fn v(x: f32, y0: f32, y1: f32) -> Segment {
        Segment {
            x0: x,
            y0,
            x1: x,
            y1,
        }
    }

    #[test]
    fn a_full_grid_is_a_table() {
        let mut segments = Vec::new();
        for y in [100.0, 120.0, 140.0, 160.0] {
            segments.push(h(y, 50.0, 350.0));
        }
        for x in [50.0, 150.0, 250.0, 350.0] {
            segments.push(v(x, 100.0, 160.0));
        }
        let grids = find_grids(&[path(segments)]);
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].row_count(), 3);
        assert_eq!(grids[0].column_count(), 3);
        assert_eq!(grids[0].cell_at((60.0, 110.0)), Some((0, 0)));
        assert_eq!(grids[0].cell_at((300.0, 150.0)), Some((2, 2)));
    }

    #[test]
    fn lines_without_columns_are_not_a_table() {
        let segments = vec![
            h(100.0, 50.0, 350.0),
            h(120.0, 50.0, 350.0),
            h(140.0, 50.0, 350.0),
        ];
        assert!(find_grids(&[path(segments)]).is_empty());
    }

    #[test]
    fn a_grid_with_a_short_rule_is_not_trusted() {
        let mut segments = vec![
            h(100.0, 50.0, 350.0),
            h(120.0, 50.0, 120.0),
            h(140.0, 50.0, 350.0),
        ];
        for x in [50.0, 150.0, 350.0] {
            segments.push(v(x, 100.0, 140.0));
        }
        assert!(find_grids(&[path(segments)]).is_empty());
    }
}
