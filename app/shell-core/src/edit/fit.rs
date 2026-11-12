//! "Shrink to Fit": putting new text back into the lines its paragraph had.
//!
//! when typed text needs more lines than the paragraph had, letters-edit
//! re-sets it anyway and reports the overflow; the new lines may then sit
//! over whatever is below. the person can keep that ("Let It Grow") or ask
//! for the text to fit. the engine reflows a block to its own width and has
//! no "make it smaller" switch, so fitting is done in two engine calls on a
//! copy of the document:
//!
//! 1. the words are shared out over the paragraph's own number of lines
//!    (`balance_lines`) and re-set line by line (`reflow: false`), so no
//!    line is added below;
//! 2. the paragraph's text is made smaller by the share that brings its
//!    widest line back inside the old width.
//!
//! only when both worked is the copy kept, so it stays one undo step and a
//! failure leaves the document as it was.

use letters_edit::{
    replace_text, set_text_style, EditError, PageObjects, Rect, TextEditOptions, TextEditResult,
    TextStyleChange,
};
use lopdf::Document;

use super::block_matching;

/// text is never made smaller than this share of its size, so it stays
/// readable; past that it keeps its size and runs wider instead.
pub const SMALLEST_SHARE: f32 = 0.5;

/// re-sets a block's text into the number of lines it has now, smaller
/// where needed. see the module comment.
pub fn replace_fitting(
    doc: &mut Document,
    page: usize,
    block_id: &str,
    text: &str,
    options: &TextEditOptions,
) -> Result<TextEditResult, EditError> {
    let before = letters_edit::page_objects(doc, page)?;
    let block = before
        .text_blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or(EditError::UnknownObject)?;
    let place = block.rect;
    let line_count = block.lines.len().max(1);
    let size = block
        .lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.size)
        .fold(0.0_f32, f32::max);

    let mut copy = doc.clone();
    let lines = balance_lines(text, line_count).join("\n");
    let line_by_line = TextEditOptions {
        reflow: false,
        ..*options
    };
    let set = replace_text(&mut copy, page, block_id, &lines, &line_by_line)?;
    let mut result = shrink_to_width(&mut copy, page, &set.objects, &place, size, options)?;
    result.notices.splice(0..0, set.notices);
    *doc = copy;
    Ok(result)
}

/// makes the block at `place` small enough for its widest line to fit the
/// old width again. a block that already fits is left alone.
fn shrink_to_width(
    doc: &mut Document,
    page: usize,
    objects: &PageObjects,
    place: &Rect,
    size: f32,
    options: &TextEditOptions,
) -> Result<TextEditResult, EditError> {
    let block = block_matching(objects, place).ok_or(EditError::UnknownObject)?;
    let widest = block
        .lines
        .iter()
        .map(|line| line.rect.width)
        .fold(0.0_f32, f32::max);
    let share = shrink_share(place.width, widest);
    if share >= 1.0 || size <= 0.0 {
        return Ok(TextEditResult {
            objects: objects.clone(),
            notices: Vec::new(),
            overflow: None,
        });
    }
    let style = TextStyleChange {
        size: Some(size * share),
        ..TextStyleChange::default()
    };
    let id = block.id.clone();
    set_text_style(doc, page, &id, None, &style, options)
}

/// how much smaller text must be for a line `widest` points wide to fit
/// `room` points: 1 when it fits, never below `SMALLEST_SHARE`. a little
/// is kept spare, since glyph boxes and the engine's widths differ slightly.
pub fn shrink_share(room: f32, widest: f32) -> f32 {
    if widest <= room || widest <= 0.0 {
        return 1.0;
    }
    (room / widest * 0.98).clamp(SMALLEST_SHARE, 1.0)
}

/// shares the words of `text` out over `count` lines, keeping their order
/// and making the lines about as long as each other.
///
/// greedy by characters: a line ends once it has reached its share of
/// what is left, so a long word early on doesn't leave the last line empty.
pub fn balance_lines(text: &str, count: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let count = count.max(1);
    if words.len() <= count {
        let mut lines: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        lines.resize(lines.len().max(1), String::new());
        return lines;
    }
    let mut lines = Vec::with_capacity(count);
    let mut rest = &words[..];
    for remaining_lines in (1..=count).rev() {
        if remaining_lines == 1 {
            lines.push(rest.join(" "));
            break;
        }
        let left: usize = rest.iter().map(|word| word.len() + 1).sum();
        let target = left / remaining_lines;
        // the first word always goes; each next one goes while the line is
        // nearer its share with it than without it. every remaining line
        // keeps at least one word
        let mut taken = 1;
        let mut length = rest[0].len() + 1;
        while taken < rest.len() - (remaining_lines - 1) {
            let next = rest[taken].len() + 1;
            if length + next / 2 > target {
                break;
            }
            length += next;
            taken += 1;
        }
        lines.push(rest[..taken].join(" "));
        rest = &rest[taken..];
    }
    lines
}
