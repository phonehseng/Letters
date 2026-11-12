//! where every character of a page sits, for selecting text on screen.
//!
//! this file knows nothing about pdfium. the engine hands it the page's
//! characters in pdfium's reading order, each with its rectangle already
//! turned into viewer coordinates (points from the top left of the page
//! as shown, y down), and gets back the characters grouped into lines.
//!
//! two things make that less simple than it sounds:
//! - pdfium invents characters that are not drawn: spaces between words
//!   and line breaks between lines. they have no useful rectangle, but
//!   the spaces matter for copying text and for finding word edges, so
//!   they are kept and given a rectangle borrowed from their neighbours.
//!   line breaks are dropped; the line index says where lines change.
//! - on a turned page a line of text runs down or up the screen instead
//!   of across, so "is this the same line" is asked along the direction
//!   the line actually runs.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::search::TextRect;
use crate::DocumentId;

/// one character of a page and where it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextChar {
    pub ch: char,
    /// points from the top left of the page as shown, y down
    pub rect: TextRect,
    /// which line it belongs to, counted from 0 in reading order
    pub line: u32,
}

/// the characters of one page, in reading order, and the box of each line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<TextChar>,
    /// one rectangle per line, covering all of its characters
    pub lines: Vec<TextRect>,
}

/// a character as the engine read it, before lines are known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawChar {
    pub ch: char,
    /// none when pdfium has no usable box for it (an invented space)
    pub rect: Option<TextRect>,
}

/// characters on one line share at least this much of their height
/// (or width, on a turned page) with the line.
const SAME_LINE_OVERLAP: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Across,
    Down,
}

/// the line being built while the characters are read.
struct OpenLine {
    bounds: TextRect,
    last: TextRect,
    direction: Option<Direction>,
}

impl OpenLine {
    fn new(rect: TextRect) -> Self {
        Self {
            bounds: rect,
            last: rect,
            direction: None,
        }
    }

    /// whether `rect` carries on this line, and if so in which direction.
    fn continues(&self, rect: &TextRect) -> Option<Direction> {
        let across = overlap_share(rect.y, rect.height, self.bounds.y, self.bounds.height);
        let down = overlap_share(rect.x, rect.width, self.bounds.x, self.bounds.width);
        match self.direction {
            Some(Direction::Across) => (across >= SAME_LINE_OVERLAP).then_some(Direction::Across),
            Some(Direction::Down) => (down >= SAME_LINE_OVERLAP).then_some(Direction::Down),
            None => {
                // the second character of a line decides which way it runs
                let moved_x =
                    (centre(rect.x, rect.width) - centre(self.last.x, self.last.width)).abs();
                let moved_y =
                    (centre(rect.y, rect.height) - centre(self.last.y, self.last.height)).abs();
                if across >= SAME_LINE_OVERLAP && moved_x >= moved_y {
                    Some(Direction::Across)
                } else if down >= SAME_LINE_OVERLAP {
                    Some(Direction::Down)
                } else if across >= SAME_LINE_OVERLAP {
                    Some(Direction::Across)
                } else {
                    None
                }
            }
        }
    }

    fn add(&mut self, rect: TextRect, direction: Direction) {
        self.bounds = union(&self.bounds, &rect);
        self.last = rect;
        self.direction = Some(direction);
    }
}

/// groups a page's characters into lines and gives invented spaces a box.
pub fn build_page_text(raw: &[RawChar]) -> PageText {
    // first pass: decide each character's line; invented spaces wait
    // for a box until their neighbours are known
    let mut placed: Vec<(char, Option<TextRect>, u32)> = Vec::new();
    let mut lines: Vec<TextRect> = Vec::new();
    let mut open: Option<OpenLine> = None;
    let mut break_seen = false;
    for character in raw {
        if character.ch == '\r' || character.ch == '\n' {
            break_seen = true;
            continue;
        }
        if character.ch.is_control() {
            continue;
        }
        let Some(rect) = character.rect.filter(has_area) else {
            // a space without a box belongs to the line it sits in. at
            // the very start of a page, or right after a break, there is
            // no such line, so it is dropped
            if character.ch.is_whitespace() && open.is_some() && !break_seen {
                placed.push((character.ch, None, lines.len() as u32 - 1));
            }
            continue;
        };
        let continued = match (&open, break_seen) {
            (Some(line), false) => line.continues(&rect),
            _ => None,
        };
        match (continued, open.as_mut()) {
            (Some(direction), Some(line)) => {
                line.add(rect, direction);
                *lines.last_mut().expect("an open line is listed") = line.bounds;
            }
            _ => {
                open = Some(OpenLine::new(rect));
                lines.push(rect);
            }
        }
        break_seen = false;
        placed.push((character.ch, Some(rect), lines.len() as u32 - 1));
    }
    let chars = fill_space_boxes(&placed);
    PageText { chars, lines }
}

/// gives each invented space the gap between the characters either side
/// of it on its line, or a sliver at the end of the line before it.
fn fill_space_boxes(placed: &[(char, Option<TextRect>, u32)]) -> Vec<TextChar> {
    let mut chars = Vec::with_capacity(placed.len());
    for (index, &(ch, rect, line)) in placed.iter().enumerate() {
        let rect = rect.unwrap_or_else(|| {
            let before = placed[..index]
                .iter()
                .rev()
                .find(|other| other.2 == line && other.1.is_some())
                .and_then(|other| other.1);
            let after = placed[index + 1..]
                .iter()
                .find(|other| other.2 == line && other.1.is_some())
                .and_then(|other| other.1);
            gap_between(before, after)
        });
        chars.push(TextChar { ch, rect, line });
    }
    chars
}

/// the box of the space between two characters on one line.
fn gap_between(before: Option<TextRect>, after: Option<TextRect>) -> TextRect {
    let Some(before) = before else {
        return TextRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    };
    let Some(after) = after else {
        // nothing follows: a zero-width box at the end of the character
        return TextRect {
            x: before.x + before.width,
            width: 0.0,
            ..before
        };
    };
    let runs_down = (after.y - before.y).abs() > (after.x - before.x).abs();
    if runs_down {
        let top = before.y + before.height;
        TextRect {
            y: top,
            height: (after.y - top).max(0.0),
            ..before
        }
    } else {
        let left = before.x + before.width;
        TextRect {
            x: left,
            width: (after.x - left).max(0.0),
            ..before
        }
    }
}

fn has_area(rect: &TextRect) -> bool {
    rect.width > 0.0 && rect.height > 0.0 && rect.x.is_finite() && rect.y.is_finite()
}

fn centre(start: f32, size: f32) -> f32 {
    start + size / 2.0
}

/// how much of the smaller of two spans the other one covers, 0 to 1.
fn overlap_share(start_a: f32, size_a: f32, start_b: f32, size_b: f32) -> f32 {
    let overlap = (start_a + size_a).min(start_b + size_b) - start_a.max(start_b);
    let smaller = size_a.min(size_b);
    if smaller <= 0.0 {
        return 0.0;
    }
    (overlap / smaller).max(0.0)
}

fn union(a: &TextRect, b: &TextRect) -> TextRect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    TextRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// how many pages' text the worker keeps. selecting text touches the
/// page under the pointer and perhaps its neighbours; a few is plenty,
/// and each page's text is small.
pub const PAGE_TEXT_CACHE_PAGES: usize = 8;

/// the text of the last few pages asked for, newest first.
///
/// reading a page's text is quick but not free, and a drag across a
/// page asks for the same page many times over.
#[derive(Default)]
pub struct PageTextCache {
    entries: VecDeque<(DocumentId, u32, Arc<PageText>)>,
}

impl PageTextCache {
    pub fn get(&mut self, id: DocumentId, page: u32) -> Option<Arc<PageText>> {
        let position = self
            .entries
            .iter()
            .position(|(document, number, _)| *document == id && *number == page)?;
        // a page asked for again becomes the newest
        let entry = self.entries.remove(position)?;
        let text = entry.2.clone();
        self.entries.push_front(entry);
        Some(text)
    }

    pub fn insert(&mut self, id: DocumentId, page: u32, text: Arc<PageText>) {
        self.entries
            .retain(|(document, number, _)| !(*document == id && *number == page));
        self.entries.push_front((id, page, text));
        self.entries.truncate(PAGE_TEXT_CACHE_PAGES);
    }

    /// forgets a document's pages, after it was reloaded or closed.
    pub fn remove_document(&mut self, id: DocumentId) {
        self.entries.retain(|(document, _, _)| *document != id);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> TextRect {
        TextRect {
            x,
            y,
            width,
            height,
        }
    }

    /// a word laid out left to right from `x`, each letter 10 wide.
    fn across(word: &str, x: f32, y: f32) -> Vec<RawChar> {
        word.chars()
            .enumerate()
            .map(|(index, ch)| RawChar {
                ch,
                rect: Some(rect(x + 10.0 * index as f32, y, 10.0, 20.0)),
            })
            .collect()
    }

    fn invented(ch: char) -> RawChar {
        RawChar { ch, rect: None }
    }

    fn text_of(page: &PageText) -> String {
        page.chars.iter().map(|character| character.ch).collect()
    }

    fn lines_of(page: &PageText) -> Vec<u32> {
        page.chars.iter().map(|character| character.line).collect()
    }

    #[test]
    fn an_empty_page_has_no_text() {
        assert_eq!(build_page_text(&[]), PageText::default());
    }

    #[test]
    fn letters_side_by_side_make_one_line() {
        let page = build_page_text(&across("cell", 72.0, 100.0));
        assert_eq!(text_of(&page), "cell");
        assert_eq!(lines_of(&page), [0, 0, 0, 0]);
        assert_eq!(page.lines, [rect(72.0, 100.0, 40.0, 20.0)]);
    }

    #[test]
    fn line_breaks_start_a_new_line_and_are_dropped() {
        let mut raw = across("ab", 72.0, 100.0);
        raw.push(invented('\r'));
        raw.push(invented('\n'));
        raw.extend(across("cd", 72.0, 130.0));
        let page = build_page_text(&raw);
        assert_eq!(text_of(&page), "abcd");
        assert_eq!(lines_of(&page), [0, 0, 1, 1]);
        assert_eq!(page.lines.len(), 2);
    }

    #[test]
    fn a_jump_to_a_lower_line_starts_a_new_line_even_without_a_break() {
        let mut raw = across("ab", 72.0, 100.0);
        raw.extend(across("cd", 72.0, 130.0));
        let page = build_page_text(&raw);
        assert_eq!(lines_of(&page), [0, 0, 1, 1]);
    }

    #[test]
    fn a_slightly_raised_letter_stays_on_its_line() {
        let mut raw = across("ab", 72.0, 100.0);
        // a superscript drawn a little higher still overlaps most of the line
        raw.push(RawChar {
            ch: '2',
            rect: Some(rect(92.0, 95.0, 6.0, 12.0)),
        });
        let page = build_page_text(&raw);
        assert_eq!(lines_of(&page), [0, 0, 0]);
    }

    #[test]
    fn an_invented_space_gets_the_gap_between_its_neighbours() {
        let mut raw = across("ab", 72.0, 100.0);
        raw.push(invented(' '));
        raw.extend(across("cd", 112.0, 100.0));
        let page = build_page_text(&raw);
        assert_eq!(text_of(&page), "ab cd");
        // "b" ends at 92 and "c" starts at 112
        assert_eq!(page.chars[2].rect, rect(92.0, 100.0, 20.0, 20.0));
        assert_eq!(page.chars[2].line, 0);
    }

    #[test]
    fn a_space_at_the_end_of_a_line_is_a_sliver_after_its_letter() {
        let mut raw = across("ab", 72.0, 100.0);
        raw.push(invented(' '));
        raw.push(invented('\r'));
        raw.push(invented('\n'));
        raw.extend(across("c", 72.0, 130.0));
        let page = build_page_text(&raw);
        assert_eq!(text_of(&page), "ab c");
        assert_eq!(page.chars[2].rect, rect(92.0, 100.0, 0.0, 20.0));
        assert_eq!(page.chars[2].line, 0);
    }

    #[test]
    fn spaces_before_any_text_are_dropped() {
        let mut raw = vec![invented(' ')];
        raw.extend(across("a", 72.0, 100.0));
        assert_eq!(text_of(&build_page_text(&raw)), "a");
    }

    #[test]
    fn invisible_letters_without_a_box_are_dropped() {
        let mut raw = across("a", 72.0, 100.0);
        raw.push(invented('x'));
        raw.push(RawChar {
            ch: 'y',
            rect: Some(rect(90.0, 100.0, 0.0, 0.0)),
        });
        assert_eq!(text_of(&build_page_text(&raw)), "a");
    }

    #[test]
    fn text_running_down_a_turned_page_makes_one_line() {
        // each letter 20 wide and 10 tall, stacked downward
        let raw: Vec<RawChar> = "cell"
            .chars()
            .enumerate()
            .map(|(index, ch)| RawChar {
                ch,
                rect: Some(rect(600.0, 72.0 + 10.0 * index as f32, 20.0, 10.0)),
            })
            .collect();
        let page = build_page_text(&raw);
        assert_eq!(lines_of(&page), [0, 0, 0, 0]);
        assert_eq!(page.lines, [rect(600.0, 72.0, 20.0, 40.0)]);
    }

    #[test]
    fn a_new_column_on_a_turned_page_is_a_new_line() {
        let column = |x: f32| -> Vec<RawChar> {
            (0..3)
                .map(|index| RawChar {
                    ch: 'a',
                    rect: Some(rect(x, 72.0 + 10.0 * index as f32, 20.0, 10.0)),
                })
                .collect()
        };
        let mut raw = column(600.0);
        raw.extend(column(570.0));
        let page = build_page_text(&raw);
        assert_eq!(lines_of(&page), [0, 0, 0, 1, 1, 1]);
    }

    #[test]
    fn the_space_in_downward_text_fills_the_vertical_gap() {
        let raw = vec![
            RawChar {
                ch: 'a',
                rect: Some(rect(600.0, 72.0, 20.0, 10.0)),
            },
            invented(' '),
            RawChar {
                ch: 'b',
                rect: Some(rect(600.0, 90.0, 20.0, 10.0)),
            },
        ];
        let page = build_page_text(&raw);
        assert_eq!(page.chars[1].rect, rect(600.0, 82.0, 20.0, 8.0));
    }

    fn cached(text: &str) -> Arc<PageText> {
        Arc::new(build_page_text(&across(text, 0.0, 0.0)))
    }

    #[test]
    fn the_cache_returns_what_was_put_in() {
        let mut cache = PageTextCache::default();
        cache.insert(DocumentId(1), 0, cached("a"));
        assert_eq!(text_of(&cache.get(DocumentId(1), 0).unwrap()), "a");
        assert!(cache.get(DocumentId(1), 1).is_none());
        assert!(cache.get(DocumentId(2), 0).is_none());
    }

    #[test]
    fn the_cache_keeps_only_the_newest_pages() {
        let mut cache = PageTextCache::default();
        for page in 0..(PAGE_TEXT_CACHE_PAGES as u32 + 3) {
            cache.insert(DocumentId(1), page, cached("a"));
        }
        assert_eq!(cache.len(), PAGE_TEXT_CACHE_PAGES);
        assert!(cache.get(DocumentId(1), 0).is_none());
        assert!(cache
            .get(DocumentId(1), PAGE_TEXT_CACHE_PAGES as u32 + 2)
            .is_some());
    }

    #[test]
    fn a_page_used_again_is_kept_longest() {
        let mut cache = PageTextCache::default();
        cache.insert(DocumentId(1), 0, cached("a"));
        for page in 1..PAGE_TEXT_CACHE_PAGES as u32 {
            cache.insert(DocumentId(1), page, cached("b"));
        }
        cache.get(DocumentId(1), 0);
        cache.insert(DocumentId(1), 99, cached("c"));
        assert!(cache.get(DocumentId(1), 0).is_some());
        assert!(cache.get(DocumentId(1), 1).is_none());
    }

    #[test]
    fn a_reloaded_document_loses_its_cached_pages_only() {
        let mut cache = PageTextCache::default();
        cache.insert(DocumentId(1), 0, cached("a"));
        cache.insert(DocumentId(2), 0, cached("b"));
        cache.remove_document(DocumentId(1));
        assert!(cache.get(DocumentId(1), 0).is_none());
        assert!(cache.get(DocumentId(2), 0).is_some());
    }

    #[test]
    fn putting_a_page_in_twice_keeps_one_copy() {
        let mut cache = PageTextCache::default();
        cache.insert(DocumentId(1), 0, cached("a"));
        cache.insert(DocumentId(1), 0, cached("b"));
        assert_eq!(cache.len(), 1);
        assert_eq!(text_of(&cache.get(DocumentId(1), 0).unwrap()), "b");
    }
}
