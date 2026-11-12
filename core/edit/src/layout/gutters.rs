//! finding the white gutters between columns of text.
//!
//! one line alone can't tell a column gutter from a wide word space or a
//! tab. a gutter shows itself over several lines: a white band at the same
//! place that line after line has text on both sides of. lines are never
//! joined across such a band, however narrow it is.

use super::grouping::visible;
use super::*;

/// a band at least this wide (in font sizes) can be a gutter. the narrowest
/// common gutter is latex's default of 10 pt between columns of 10 pt
/// text; justified word spaces seldom stretch that far, and never in the
/// same place several lines running.
const GUTTER_MIN_WIDTH: f32 = 1.0;
/// how many lines running must have text on both sides of the band. one
/// or two lines with a tab gap at the same place stay joined.
const GUTTER_MIN_LINES: usize = 3;
/// directions closer than this (as a dot product) count as the same.
const SAME_DIRECTION: f32 = 0.995;

/// a white band between columns, in the coordinates of its text direction.
#[derive(Clone, Debug)]
pub(super) struct Gutter {
    direction: Point,
    /// where the band starts and ends, measured along `direction`
    from: f32,
    to: f32,
    /// the first and last baselines with text on both sides, measured
    /// along the normal
    top: f32,
    bottom: f32,
}

impl Gutter {
    /// whether a glyph starting at `origin` sits across the gutter from a
    /// line that ends at `line_end`.
    pub(super) fn divides(&self, line: &LineModel, line_end: Point, origin: Point) -> bool {
        if line.direction.dot(self.direction) < SAME_DIRECTION {
            return false;
        }
        let baseline = line.start.dot(normal_of(self.direction));
        let slack = BASELINE_TOLERANCE * line.size;
        if baseline < self.top - slack || baseline > self.bottom + slack {
            return false;
        }
        // the middle, not the edges: a trailing space may reach a little
        // way into the band, and the band's edges are only as exact as
        // the most ragged line that made it
        let middle = (self.from + self.to) / 2.0;
        line_end.dot(self.direction) < middle && origin.dot(self.direction) >= middle
    }
}

/// the text on one baseline: where its ink is, along the text direction.
struct Row {
    direction: Point,
    baseline: f32,
    size: f32,
    /// stretches covered by glyphs, sorted and merged
    ink: Vec<(f32, f32)>,
}

impl Row {
    fn has_ink_before(&self, along: f32) -> bool {
        self.ink.iter().any(|(_, end)| *end <= along)
    }

    fn has_ink_after(&self, along: f32) -> bool {
        self.ink.iter().any(|(start, _)| *start >= along)
    }

    /// the white stretches between glyphs, wide enough to be a gutter.
    fn wide_holes(&self) -> Vec<(f32, f32)> {
        let min_width = GUTTER_MIN_WIDTH * self.size;
        self.ink
            .windows(2)
            .map(|pair| (pair[0].1, pair[1].0))
            .filter(|(from, to)| to - from >= min_width)
            .collect()
    }

    /// the widest part of a band that this row leaves white.
    fn white_part_of(&self, from: f32, to: f32) -> (f32, f32) {
        let mut pieces = vec![(from, to)];
        for (ink_start, ink_end) in &self.ink {
            pieces = pieces
                .into_iter()
                .flat_map(|(start, end)| [(start, end.min(*ink_start)), (start.max(*ink_end), end)])
                .filter(|(start, end)| end > start)
                .collect();
        }
        pieces
            .into_iter()
            .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
            .unwrap_or((from, from))
    }
}

/// a band being followed down the page, row by row.
struct Candidate {
    from: f32,
    to: f32,
    top: f32,
    bottom: f32,
    lines: usize,
}

/// the gutters of a page, found from lines grouped without them.
pub(super) fn find_gutters(run: &Interpretation, lines: &[LineModel]) -> Vec<Gutter> {
    let mut gutters = Vec::new();
    for rows in rows_by_direction(run, lines) {
        gutters.extend(gutters_in(&rows));
    }
    gutters
}

fn normal_of(direction: Point) -> Point {
    Point::new(-direction.y, direction.x)
}

/// gathers lines into rows (lines on one baseline, which a wide gap has
/// already split) and sorts each direction's rows down the page.
fn rows_by_direction(run: &Interpretation, lines: &[LineModel]) -> Vec<Vec<Row>> {
    let mut groups: Vec<Vec<Row>> = Vec::new();
    for line in lines {
        let group = match groups
            .iter()
            .position(|rows| rows[0].direction.dot(line.direction) >= SAME_DIRECTION)
        {
            Some(index) => &mut groups[index],
            None => {
                groups.push(Vec::new());
                groups.last_mut().expect("just pushed")
            }
        };
        add_to_rows(run, group, line);
    }
    for rows in &mut groups {
        for row in rows.iter_mut() {
            row.ink = merged(std::mem::take(&mut row.ink));
        }
        rows.sort_by(|a, b| a.baseline.total_cmp(&b.baseline));
    }
    groups
}

fn add_to_rows(run: &Interpretation, rows: &mut Vec<Row>, line: &LineModel) {
    let direction = rows.first().map_or(line.direction, |row| row.direction);
    let baseline = line.start.dot(normal_of(direction));
    let tolerance = BASELINE_TOLERANCE * line.size;
    let index = match rows
        .iter()
        .position(|row| (row.baseline - baseline).abs() < tolerance)
    {
        Some(index) => index,
        None => {
            rows.push(Row {
                direction,
                baseline,
                size: line.size,
                ink: Vec::new(),
            });
            rows.len() - 1
        }
    };
    let row = &mut rows[index];
    row.size = row.size.max(line.size);
    for char in &line.chars {
        let LineChar::Glyph(glyph) = char else {
            continue;
        };
        // a space is white: it may sit right inside a gutter
        if run.glyphs[*glyph].is_space {
            continue;
        }
        let seen = visible(run, &run.glyphs[*glyph]);
        let start = seen.origin.dot(direction);
        let end = seen.end.dot(direction);
        row.ink.push((start.min(end), start.max(end)));
    }
}

/// sorts stretches and joins the ones that touch or overlap.
fn merged(mut ink: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    ink.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut joined: Vec<(f32, f32)> = Vec::new();
    for (start, end) in ink {
        match joined.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => joined.push((start, end)),
        }
    }
    joined
}

/// follows white bands down the rows of one direction. a band lives on
/// while each row leaves enough of it white; it counts a line only when
/// that row has text on both sides, so the white margin beside a single
/// column is never taken for a gutter.
fn gutters_in(rows: &[Row]) -> Vec<Gutter> {
    let mut open: Vec<Candidate> = Vec::new();
    let mut found = Vec::new();
    for row in rows {
        let mut still_open = Vec::new();
        for candidate in open {
            match narrowed(candidate, row) {
                Ok(candidate) => still_open.push(candidate),
                Err(closed) => found.extend(as_gutter(closed, row.direction)),
            }
        }
        open = still_open;
        start_candidates(row, &mut open);
    }
    let direction = rows.first().map(|row| row.direction);
    for candidate in open {
        found.extend(direction.and_then(|direction| as_gutter(candidate, direction)));
    }
    found
}

/// the band after one more row, or the band as it was if the row closes it.
fn narrowed(mut candidate: Candidate, row: &Row) -> Result<Candidate, Candidate> {
    let (from, to) = row.white_part_of(candidate.from, candidate.to);
    if to - from < GUTTER_MIN_WIDTH * row.size {
        return Err(candidate);
    }
    if row.has_ink_before(from) && row.has_ink_after(to) {
        candidate.lines += 1;
        candidate.bottom = row.baseline;
    }
    candidate.from = from;
    candidate.to = to;
    Ok(candidate)
}

/// a row's wide holes that no open band already covers start new bands.
fn start_candidates(row: &Row, open: &mut Vec<Candidate>) {
    for (from, to) in row.wide_holes() {
        let covered = open.iter().any(|band| band.from < to && from < band.to);
        if !covered {
            open.push(Candidate {
                from,
                to,
                top: row.baseline,
                bottom: row.baseline,
                lines: 1,
            });
        }
    }
}

fn as_gutter(candidate: Candidate, direction: Point) -> Option<Gutter> {
    (candidate.lines >= GUTTER_MIN_LINES).then_some(Gutter {
        direction,
        from: candidate.from,
        to: candidate.to,
        top: candidate.top,
        bottom: candidate.bottom,
    })
}
