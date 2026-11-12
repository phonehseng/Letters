//! finding a page's footer: text near the bottom that repeats on the
//! pages next to it (page numbers counted as the same text). reflow
//! never pushes content into it or moves it.
//!
//! only the page before and the page after are read, and only to compare
//! their text; nothing on them changes.

use lopdf::Document;

use crate::layout;
use crate::model::{PageObjects, TextBlock};

/// a footer lives in the lowest fifth of the page; a repeated line higher
/// up is running text or a heading, not a footer.
const FOOTER_ZONE: f32 = 0.2;
/// repeated lines must sit this close (points) to the same height.
const SAME_HEIGHT: f32 = 3.0;

/// the top of the footer on screen, when the page has one.
pub(crate) fn footer_top(doc: &Document, page: usize, objects: &PageObjects) -> Option<f32> {
    let neighbours: Vec<PageObjects> = [page.checked_sub(1), Some(page + 1)]
        .into_iter()
        .flatten()
        .filter_map(|other| layout::build(doc, other).ok().map(|model| model.objects()))
        .collect();
    objects
        .text_blocks
        .iter()
        .filter(|block| in_footer_zone(block, objects.height))
        .filter(|block| neighbours.iter().any(|other| repeats_on(block, other)))
        .map(|block| block.rect.y)
        .reduce(f32::min)
}

fn in_footer_zone(block: &TextBlock, page_height: f32) -> bool {
    block.rect.y > page_height * (1.0 - FOOTER_ZONE)
}

/// the same text at the same height on another page, ignoring digits so
/// "Page 3" repeats as "Page 4".
fn repeats_on(block: &TextBlock, other: &PageObjects) -> bool {
    let text = without_digits(&block.text);
    other.text_blocks.iter().any(|candidate| {
        (candidate.rect.y - block.rect.y).abs() < SAME_HEIGHT
            && without_digits(&candidate.text) == text
    })
}

/// every run of digits becomes one "#", so "Page 9" and "Page 10" match.
fn without_digits(text: &str) -> String {
    let mut result = String::new();
    for character in text.chars() {
        if !character.is_ascii_digit() {
            result.push(character);
        } else if !result.ends_with('#') {
            result.push('#');
        }
    }
    result
}
