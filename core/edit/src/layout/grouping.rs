//! grouping glyphs into lines (same baseline, small gaps) and lines into
//! blocks (similar size, even spacing, lined up, no rule between them).

use super::*;

/// a glyph on screen: baseline start and end, size and direction.
struct VisibleGlyph {
    origin: Point,
    end: Point,
    direction: Point,
    size: f32,
}

fn visible(run: &Interpretation, glyph: &GlyphRecord) -> VisibleGlyph {
    let origin = run.space.visible_point(glyph.origin);
    let end = run.space.visible_point(glyph.end);
    let mut direction = end.minus(origin);
    if direction.length() < 1e-6 {
        // a glyph with no width (a combining accent, a zero-width space)
        // runs in the direction of its text matrix
        let to_page = glyph.text_matrix.then(&run.shows[glyph.show].ctm);
        direction = run
            .space
            .visible_vector(to_page.apply_vector(Point::new(1.0, 0.0)));
    }
    VisibleGlyph {
        origin,
        end,
        direction: direction.unit(),
        size: glyph.size.max(0.1),
    }
}

pub(super) fn group_lines(run: &Interpretation) -> Vec<LineModel> {
    let mut lines: Vec<LineModel> = Vec::new();
    for (index, glyph) in run.glyphs.iter().enumerate() {
        let seen = visible(run, glyph);
        let joins = lines
            .last()
            .and_then(|line| joining_gap(run, line, &seen, glyph));
        match (joins, lines.last_mut()) {
            (Some(gap), Some(line)) => {
                let previous_space = match line.chars.last() {
                    Some(LineChar::Glyph(last)) => run.glyphs[*last].is_space,
                    _ => true,
                };
                if gap > WORD_GAP * line.size && !previous_space && !glyph.is_space {
                    line.chars.push(LineChar::Gap);
                }
                line.chars.push(LineChar::Glyph(index));
                line.length = line
                    .length
                    .max(seen.end.minus(line.start).dot(line.direction));
            }
            _ => lines.push(LineModel {
                chars: vec![LineChar::Glyph(index)],
                start: seen.origin,
                direction: seen.direction,
                normal: Point::new(-seen.direction.y, seen.direction.x),
                size: seen.size,
                length: seen.end.minus(seen.origin).dot(seen.direction),
            }),
        }
    }
    lines
}

/// the gap before a glyph if it continues the line, or none if it starts
/// a new one.
fn joining_gap(
    run: &Interpretation,
    line: &LineModel,
    seen: &VisibleGlyph,
    glyph: &GlyphRecord,
) -> Option<f32> {
    let same_direction = line.direction.dot(seen.direction) > 0.995;
    let off_baseline = seen.origin.minus(line.start).dot(line.normal).abs();
    let ratio = seen.size / line.size;
    let line_end = line.start.plus(line.direction.scaled(line.length));
    let gap = seen.origin.minus(line_end).dot(line.direction);
    let close = gap > -0.6 * line.size && gap < COLUMN_GAP * line.size;
    // invisible text (a scan's text layer) never joins visible text
    let last_invisible = line
        .chars
        .iter()
        .rev()
        .find_map(|char| match char {
            LineChar::Glyph(index) => Some(run.glyphs[*index].invisible),
            LineChar::Gap => None,
        })
        .unwrap_or(false);
    (same_direction
        && off_baseline < BASELINE_TOLERANCE * line.size
        && (0.6..1.7).contains(&ratio)
        && close
        && last_invisible == glyph.invisible)
        .then_some(gap.max(0.0))
}

pub(super) fn group_blocks(lines: Vec<LineModel>, separators: &[Separator]) -> Vec<BlockModel> {
    let mut blocks: Vec<BlockModel> = Vec::new();
    for line in lines {
        let joins = blocks
            .last()
            .is_some_and(|block| continues_block(block, &line, separators));
        match (joins, blocks.last_mut()) {
            (true, Some(block)) => {
                let previous = block.lines.last().expect("blocks have lines");
                let step = line.start.minus(previous.start).dot(previous.normal);
                block.leading.get_or_insert(step);
                block.lines.push(line);
            }
            _ => blocks.push(BlockModel {
                lines: vec![line],
                alignment: TextAlignment::Left,
                leading: None,
            }),
        }
    }
    for block in &mut blocks {
        block.alignment = alignment_of(block);
    }
    blocks
}

fn continues_block(block: &BlockModel, line: &LineModel, separators: &[Separator]) -> bool {
    let first = &block.lines[0];
    let previous = block.lines.last().expect("blocks have lines");
    if separators::separated(separators, previous, line) {
        return false;
    }
    if first.direction.dot(line.direction) < 0.995
        || !(0.8..1.25).contains(&(line.size / first.size))
    {
        return false;
    }
    let step = line.start.minus(previous.start).dot(previous.normal);
    if !(0.8 * first.size..2.5 * first.size).contains(&step) {
        return false;
    }
    if let Some(leading) = block.leading {
        if (step - leading).abs() > 0.25 * leading {
            return false;
        }
    }
    // the line must sit under the block, not off to one side
    let along = |line: &LineModel| {
        let start = line.start.minus(first.start).dot(first.direction);
        (start, start + line.length)
    };
    let (start, end) = along(line);
    let (block_start, block_end) = block
        .lines
        .iter()
        .map(along)
        .fold((f32::MAX, f32::MIN), |(low, high), (start, end)| {
            (low.min(start), high.max(end))
        });
    let slack = 0.5 * first.size;
    start < block_end + slack && end > block_start - slack
}

fn alignment_of(block: &BlockModel) -> TextAlignment {
    if block.lines.len() < 2 {
        return TextAlignment::Left;
    }
    let first = &block.lines[0];
    let tolerance = 0.5 * first.size;
    let ends: Vec<(f32, f32)> = block
        .lines
        .iter()
        .map(|line| {
            let start = line.start.minus(first.start).dot(first.direction);
            (start, start + line.length)
        })
        .collect();
    let lined_up = |values: &[f32]| {
        values
            .iter()
            .all(|value| (value - values[0]).abs() < tolerance)
    };
    let starts: Vec<f32> = ends.iter().map(|(start, _)| *start).collect();
    let rights: Vec<f32> = ends.iter().map(|(_, end)| *end).collect();
    let centres: Vec<f32> = ends
        .iter()
        .map(|(start, end)| (start + end) / 2.0)
        .collect();
    let all_but_last = &rights[..rights.len() - 1];
    if lined_up(&starts) && all_but_last.len() >= 2 && lined_up(all_but_last) {
        TextAlignment::Justified
    } else if lined_up(&starts) {
        TextAlignment::Left
    } else if lined_up(&rights) {
        TextAlignment::Right
    } else if lined_up(&centres) {
        TextAlignment::Center
    } else {
        TextAlignment::Left
    }
}
