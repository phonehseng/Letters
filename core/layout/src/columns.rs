//! balancing columns where a multi-column section ends on a page with a
//! continuous section break: word and libreoffice both even out the
//! columns of the last page so the text after the break starts below the
//! shortest possible block of columns.
//!
//! the lines already placed in the region are redistributed: the smallest
//! column height that holds them all in order is searched for, then lines
//! (and the rules and pictures drawn with them) move to their new column
//! and height. widow control is not re-applied inside a balanced block.

use crate::pages::Flow;
use crate::result::Story;

/// one placed line of the region, with the gap above it in its column.
struct Slot {
    line: usize,
    gap: f64,
    height: f64,
    column: usize,
    /// which paragraph the line belongs to, and its place in it
    paragraph: Vec<u32>,
    index_in_paragraph: usize,
    lines_in_paragraph: usize,
}

impl Flow<'_, '_> {
    /// the lowest point the current region's body text reaches.
    pub fn region_bottom(&self) -> f64 {
        let page = self.page_ref();
        let lines_bottom = page
            .page
            .lines
            .iter()
            .filter(|line| line.story == Story::Body && line.top >= page.region_top - 0.01)
            .map(|line| line.top + line.height)
            .fold(page.region_top, f64::max);
        if page.columns.len() == 1 {
            lines_bottom.max(self.y)
        } else {
            lines_bottom
        }
    }
}

/// evens out the columns of the region being filled on the current page.
pub fn balance_region(flow: &mut Flow) {
    let columns = flow.page_ref().columns.clone();
    let region_top = flow.page_ref().region_top;
    let slots = region_slots(flow, &columns, region_top);
    if slots.is_empty() {
        return;
    }
    let total: f64 = slots.iter().map(|s| s.gap + s.height).sum();
    let tallest = slots.iter().map(|s| s.height).fold(0.0, f64::max);
    let limit = flow.bottom() - region_top;
    let count = columns.len();
    // the smallest height that fits, in steps of a tenth of a point
    let mut height = (total / count as f64).max(tallest);
    while height < limit && !fits(&slots, height, count) {
        height += 0.1;
    }
    if !fits(&slots, height, count) {
        return;
    }
    let assignment = assign(&slots, height, count);
    move_lines(flow, &slots, &assignment, &columns, region_top);
    let used = assignment.iter().map(|(_, y, h)| y + h).fold(0.0, f64::max);
    flow.y = region_top + used;
    flow.column = count - 1;
}

fn region_slots(flow: &Flow, columns: &[(f64, f64)], region_top: f64) -> Vec<Slot> {
    let page = &flow.page_ref().page;
    let mut slots: Vec<Slot> = Vec::new();
    let mut previous_bottom: Option<(usize, f64)> = None;
    for (index, line) in page.lines.iter().enumerate() {
        if line.story != Story::Body || line.top < region_top - 0.01 {
            continue;
        }
        let column = columns
            .iter()
            .rposition(|(x, _)| line.x >= *x - 1.0)
            .unwrap_or(0);
        let gap = match previous_bottom {
            Some((previous_column, bottom)) if previous_column == column => {
                (line.top - bottom).max(0.0)
            }
            _ => 0.0,
        };
        previous_bottom = Some((column, line.top + line.height));
        slots.push(Slot {
            line: index,
            gap,
            height: line.height,
            column,
            paragraph: line.block.clone(),
            index_in_paragraph: 0,
            lines_in_paragraph: 0,
        });
    }
    number_lines_of_paragraphs(&mut slots);
    slots
}

/// each slot's index in its paragraph and the paragraph's line count.
fn number_lines_of_paragraphs(slots: &mut [Slot]) {
    let mut start = 0;
    while start < slots.len() {
        let mut end = start + 1;
        while end < slots.len() && slots[end].paragraph == slots[start].paragraph {
            end += 1;
        }
        for (index, slot) in slots[start..end].iter_mut().enumerate() {
            slot.index_in_paragraph = index;
            slot.lines_in_paragraph = end - start;
        }
        start = end;
    }
}

/// where a column break wanted before slot `at` really goes, by widow
/// and orphan control: word balances with the same rules it paginates
/// with (two_columns in the corpus: a three-line last paragraph stays
/// whole in the first column rather than splitting two and one).
fn allowed_break(slots: &[Slot], at: usize, column_start: usize) -> Option<usize> {
    let slot = &slots[at];
    let (index, count) = (slot.index_in_paragraph, slot.lines_in_paragraph);
    let paragraph_start = at - index;
    let wanted = crate::pages::widow_orphan(index, 0, count);
    let adjusted = paragraph_start + wanted;
    // a break that would have to move back past the column's own first
    // line means this column height does not work
    (adjusted > column_start).then_some(adjusted)
}

/// whether the slots fit in `count` columns of `height`, in order.
fn fits(slots: &[Slot], height: f64, count: usize) -> bool {
    let assignment = assign(slots, height, count);
    assignment.len() == slots.len()
        && assignment
            .iter()
            .all(|(column, y, h)| *column < count && y + h <= height + 0.01)
}

/// each slot's new column, y within the region, and height.
fn assign(slots: &[Slot], height: f64, count: usize) -> Vec<(usize, f64, f64)> {
    let mut out: Vec<(usize, f64, f64)> = Vec::with_capacity(slots.len());
    let mut column = 0;
    let mut column_start = 0;
    let mut y = 0.0;
    let mut index = 0;
    while index < slots.len() {
        let slot = &slots[index];
        let gap = if y == 0.0 { 0.0 } else { slot.gap };
        if y > 0.0 && y + gap + slot.height > height + 0.01 && column + 1 < count {
            let Some(at) = allowed_break(slots, index, column_start) else {
                // marks the height as failed (see `fits`)
                out.push((count, 0.0, f64::INFINITY));
                return out;
            };
            out.truncate(at);
            column += 1;
            column_start = at;
            y = 0.0;
            index = at;
            continue;
        }
        out.push((column, y + gap, slot.height));
        y += gap + slot.height;
        index += 1;
    }
    out
}

fn move_lines(
    flow: &mut Flow,
    slots: &[Slot],
    assignment: &[(usize, f64, f64)],
    columns: &[(f64, f64)],
    region_top: f64,
) {
    let page = &mut flow.page().page;
    let mut moved_rects = vec![false; page.rects.len()];
    let mut moved_images = vec![false; page.images.len()];
    for (slot, (column, y, _)) in slots.iter().zip(assignment) {
        let line = &page.lines[slot.line];
        let (old_x, old_width) = columns[slot.column];
        let dx = columns[*column].0 - old_x;
        let dy = region_top + y - line.top;
        let (top, bottom) = (line.top, line.top + line.height);
        let inside = |x: f64, y: f64| {
            x >= old_x - 1.0
                && x <= old_x + old_width + 1.0
                && y >= top - 0.01
                && y <= bottom + 0.01
        };
        for (index, rect) in page.rects.iter_mut().enumerate() {
            if !moved_rects[index] && inside(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
            {
                rect.x += dx;
                rect.y += dy;
                moved_rects[index] = true;
            }
        }
        for (index, image) in page.images.iter_mut().enumerate() {
            if !moved_images[index]
                && !image.floating
                && inside(image.x + 0.1, image.y + image.height - 0.1)
            {
                image.x += dx;
                image.y += dy;
                moved_images[index] = true;
            }
        }
        let line = &mut page.lines[slot.line];
        line.x += dx;
        line.top += dy;
        line.baseline += dy;
        for caret in &mut line.carets {
            *caret += dx;
        }
        for run in &mut line.runs {
            run.y += dy;
            for x in &mut run.x {
                *x += dx;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(heights: &[f64]) -> Vec<Slot> {
        heights
            .iter()
            .enumerate()
            .map(|(line, height)| Slot {
                line,
                gap: 0.0,
                height: *height,
                column: 0,
                // every line its own paragraph: no widow rules in the way
                paragraph: vec![line as u32],
                index_in_paragraph: 0,
                lines_in_paragraph: 1,
            })
            .collect()
    }

    #[test]
    fn ten_equal_lines_split_five_and_five() {
        let slots = slots(&[10.0; 10]);
        assert!(fits(&slots, 50.0, 2));
        assert!(!fits(&slots, 49.0, 2));
        let assignment = assign(&slots, 50.0, 2);
        assert_eq!(assignment[4], (0, 40.0, 10.0));
        assert_eq!(assignment[5], (1, 0.0, 10.0));
    }

    #[test]
    fn a_three_line_paragraph_is_not_split_two_and_one() {
        let mut slots = slots(&[10.0; 3]);
        for slot in &mut slots {
            slot.paragraph = vec![0];
        }
        number_lines_of_paragraphs(&mut slots);
        assert!(!fits(&slots, 20.0, 2));
        assert!(fits(&slots, 30.0, 2));
    }

    #[test]
    fn an_odd_count_puts_the_extra_line_first() {
        let slots = slots(&[10.0; 7]);
        assert!(fits(&slots, 40.0, 2));
        assert!(!fits(&slots, 39.0, 2));
    }
}
