//! the order a person reads a page in, columns included.
//!
//! columns are found from their gutters: upright strips of white space
//! that the narrower pieces of text never cross. anything that does cross
//! a gutter (a heading over both columns, a wide table) splits the page
//! into bands; each band is read column by column, top to bottom, and the
//! wide piece is read in its place between the bands.

use crate::geometry::{union_all, Rect};

/// something to put in order, with its box on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Item {
    pub rect: Rect,
    /// text takes part in finding columns; tables and pictures don't
    pub is_text: bool,
}

/// an item's place in the reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
    /// index into the items given
    pub item: usize,
    /// which band and column it was read in; items in the same region are
    /// read one after the other and may form one paragraph
    pub region: (usize, usize),
}

pub(crate) struct ReadingOrder {
    pub placed: Vec<Placed>,
    pub columns: usize,
}

/// the narrowest gap (points) that counts as a gutter between columns
const MIN_GUTTER: f32 = 7.0;
/// text narrower than this share of the text area may sit in a column
const NARROW_SHARE: f32 = 0.55;
/// marks a region that holds one wide item
const SPANNING: usize = usize::MAX;

pub(crate) fn reading_order(items: &[Item]) -> ReadingOrder {
    let gutters = find_gutters(items);
    let crosses = |rect: &Rect| {
        gutters
            .iter()
            .any(|(start, end)| rect.x < start - 1.0 && rect.right() > end + 1.0)
    };
    let column_of = |rect: &Rect| {
        gutters
            .iter()
            .filter(|(start, _)| rect.center_x() > *start)
            .count()
    };

    let mut spanning: Vec<usize> = (0..items.len())
        .filter(|i| crosses(&items[*i].rect))
        .collect();
    spanning.sort_by(|a, b| items[*a].rect.y.total_cmp(&items[*b].rect.y));
    let band_of = |rect: &Rect| {
        spanning
            .iter()
            .filter(|s| items[**s].rect.center_y() < rect.center_y())
            .count()
    };

    let mut placed = Vec::new();
    for band in 0..=spanning.len() {
        for column in 0..=gutters.len() {
            let mut members: Vec<usize> = (0..items.len())
                .filter(|i| !spanning.contains(i))
                .filter(|i| {
                    band_of(&items[*i].rect) == band && column_of(&items[*i].rect) == column
                })
                .collect();
            members.sort_by(|a, b| {
                let (ra, rb) = (&items[*a].rect, &items[*b].rect);
                ra.y.total_cmp(&rb.y).then(ra.x.total_cmp(&rb.x))
            });
            placed.extend(members.into_iter().map(|item| Placed {
                item,
                region: (band, column),
            }));
        }
        if let Some(wide) = spanning.get(band) {
            placed.push(Placed {
                item: *wide,
                region: (band, SPANNING),
            });
        }
    }
    ReadingOrder {
        placed,
        columns: gutters.len() + 1,
    }
}

/// x ranges (start, end) that narrow text never crosses, with enough text
/// on both sides to be columns.
fn find_gutters(items: &[Item]) -> Vec<(f32, f32)> {
    let texts: Vec<&Rect> = items
        .iter()
        .filter(|i| i.is_text)
        .map(|i| &i.rect)
        .collect();
    let Some(area) = union_all(texts.iter().copied()) else {
        return Vec::new();
    };
    let narrow: Vec<&Rect> = texts
        .iter()
        .copied()
        .filter(|r| r.width < area.width * NARROW_SHARE)
        .collect();
    if narrow.len() < 4 {
        return Vec::new();
    }
    // how many narrow pieces cover each point-wide strip of the page
    let bins = area.width.ceil() as usize + 1;
    let mut coverage = vec![0usize; bins];
    for rect in &narrow {
        let from = (rect.x - area.x).floor().max(0.0) as usize;
        let to = ((rect.right() - area.x).ceil() as usize).min(bins - 1);
        for bin in &mut coverage[from..=to] {
            *bin += 1;
        }
    }
    // a few pieces may stray into a gutter (a short heading, say)
    let allowed = narrow.len() / 25;
    let mut gutters = Vec::new();
    let mut run_start: Option<usize> = None;
    for (bin, count) in coverage
        .iter()
        .enumerate()
        .chain(std::iter::once((bins, &usize::MAX)))
    {
        let open = *count <= allowed;
        match (open, run_start) {
            (true, None) => run_start = Some(bin),
            (false, Some(start)) => {
                let (from, to) = (area.x + start as f32, area.x + bin as f32);
                if to - from >= MIN_GUTTER && has_columns_beside(&narrow, from, to) {
                    gutters.push((from, to));
                }
                run_start = None;
            }
            _ => {}
        }
    }
    gutters
}

fn has_columns_beside(narrow: &[&Rect], from: f32, to: f32) -> bool {
    let needed = ((narrow.len() as f32) * 0.15).ceil().max(2.0) as usize;
    let left = narrow.iter().filter(|r| r.right() <= from + 0.5).count();
    let right = narrow.iter().filter(|r| r.x >= to - 0.5).count();
    left >= needed && right >= needed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(x: f32, y: f32, width: f32) -> Item {
        Item {
            rect: Rect::new(x, y, width, 10.0),
            is_text: true,
        }
    }

    #[test]
    fn two_columns_are_read_one_after_the_other() {
        // a heading over both columns, then five lines in each column
        let mut items = vec![text(72.0, 50.0, 468.0)];
        for line in 0..5 {
            let y = 100.0 + line as f32 * 14.0;
            items.push(text(72.0, y, 220.0));
            items.push(text(320.0, y, 220.0));
        }
        let order = reading_order(&items);
        assert_eq!(order.columns, 2);
        let sequence: Vec<usize> = order.placed.iter().map(|p| p.item).collect();
        assert_eq!(sequence, vec![0, 1, 3, 5, 7, 9, 2, 4, 6, 8, 10]);
        assert_ne!(order.placed[1].region, order.placed[6].region);
    }

    #[test]
    fn a_single_column_keeps_its_order() {
        let items: Vec<Item> = (0..6)
            .map(|line| {
                text(
                    72.0,
                    100.0 + line as f32 * 14.0,
                    if line % 2 == 0 { 400.0 } else { 120.0 },
                )
            })
            .collect();
        let order = reading_order(&items);
        assert_eq!(order.columns, 1);
        let sequence: Vec<usize> = order.placed.iter().map(|p| p.item).collect();
        assert_eq!(sequence, vec![0, 1, 2, 3, 4, 5]);
    }
}
