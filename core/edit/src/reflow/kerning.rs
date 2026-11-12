//! the kerning a page already uses: the small TJ numbers its writer put
//! between two letters in one font. new text in that font repeats them
//! for the same pairs, and adds none for pairs the page never shows,
//! so new words are spaced the way the old words around them are.

use std::collections::HashMap;

use crate::interpret::ShowItem;
use crate::layout::PageModel;

/// TJ numbers larger than this (thousandths of an em) are word gaps or
/// column moves, not kerning; real kerning pairs stay well under 150.
const MAX_KERN: f32 = 150.0;

/// the usual TJ number between each pair of characters drawn in `font`.
pub(crate) fn kerning_pairs(model: &PageModel, font: Option<usize>) -> HashMap<(char, char), f32> {
    let mut seen: HashMap<(char, char), Vec<f32>> = HashMap::new();
    for show in &model.run.shows {
        for window in show.items.windows(3) {
            let [ShowItem::Glyph(first), ShowItem::Adjust(number), ShowItem::Glyph(second)] =
                window
            else {
                continue;
            };
            if number.abs() > MAX_KERN {
                continue;
            }
            if let Some(pair) = letter_pair(model, *first, *second, font) {
                seen.entry(pair).or_default().push(*number);
            }
        }
    }
    seen.into_iter()
        .map(|(pair, mut numbers)| {
            numbers.sort_by(f32::total_cmp);
            (pair, numbers[numbers.len() / 2])
        })
        .collect()
}

/// two glyphs as a pair of letters in the font, or none when either is a
/// space, a ligature, or in another font.
fn letter_pair(
    model: &PageModel,
    first: usize,
    second: usize,
    font: Option<usize>,
) -> Option<(char, char)> {
    let single = |glyph: usize| {
        let record = &model.run.glyphs[glyph];
        let mut chars = record.text.chars();
        let character = chars.next()?;
        let is_letter = chars.next().is_none() && !character.is_whitespace();
        (is_letter && record.font == font).then_some(character)
    };
    Some((single(first)?, single(second)?))
}
