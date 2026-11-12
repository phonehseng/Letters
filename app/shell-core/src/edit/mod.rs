//! direct editing of page content (the Edit tab), the parts that need no
//! window: the embedded font of some text for drawing an editor over it,
//! fitting re-set text back into its paragraph, reading pictures, and the
//! font library's fonts lent to edits that add letters to a font (`donors`).
//!
//! the editing itself is core/edit (letters-edit). the shell calls it
//! through `AppState::edit` so every change is one undo step; these
//! helpers sit around those calls and are tested here.

pub mod donors;
pub mod fit;
pub mod fonts;
pub mod pictures;

#[cfg(test)]
mod tests;

use letters_edit::{PageObjects, Rect, TextBlock};

/// the block on a page that best matches a place where a block was
/// before an edit. ids change with every edit, so a block is found
/// again by where it is: the one overlapping `rect` the most, and among
/// equals the one whose top left corner is closest.
pub fn block_matching<'a>(objects: &'a PageObjects, rect: &Rect) -> Option<&'a TextBlock> {
    let score = |block: &TextBlock| {
        let overlap = overlap_area(&block.rect, rect);
        let distance = (block.rect.x - rect.x).hypot(block.rect.y - rect.y);
        (overlap, -distance)
    };
    objects
        .text_blocks
        .iter()
        .filter(|block| overlap_area(&block.rect, rect) > 0.0 || near(&block.rect, rect))
        .max_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn overlap_area(a: &Rect, b: &Rect) -> f32 {
    let width = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
    let height = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
    width * height
}

/// a block that shrank to nothing still counts if it starts where the old one did
fn near(a: &Rect, b: &Rect) -> bool {
    (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0
}
