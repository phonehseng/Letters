//! turning a page's letters into words, lines and blocks in reading order.
//!
//! a pdf has no idea what a word or a line is: it only places letters. so
//! the words are rebuilt from where the letters sit:
//! 1. the page is turned upright if its text runs sideways or upside down
//! 2. letters at the same height form rows; a wide gap in a row splits it
//!    into fragments, so two columns side by side don't merge
//! 3. gaps and spaces inside a fragment split it into words
//! 4. fragments stacked closely with the same left or right edge form
//!    blocks (a paragraph, or a run of paragraphs in one column)
//! 5. blocks are read top to bottom and left to right, a block on the left
//!    before the one beside it, so two columns read column by column
//! 6. a word broken with a hyphen at the end of a line is joined again

use std::ops::Range;

use crate::geometry::{round_tenth, PageSize, QuarterTurn, Rect};
use crate::result::TextStyle;
use crate::source::Glyph;

/// a gap wider than this many font sizes splits a row: it is a column gap,
/// not a space between words. justified text rarely spaces words wider.
const COLUMN_GAP_EMS: f32 = 1.5;

/// a gap wider than this many font sizes ends a word even without a
/// space character, since many files place words without spaces.
const WORD_GAP_EMS: f32 = 0.18;

/// letter spacing counted as normal for a fragment is at most this many
/// font sizes; anything wider is a gap between words.
const MAX_TRACKING_EMS: f32 = 0.5;

/// lines further apart than this many font sizes start a new block, so a
/// blank line between paragraphs separates them.
const BLOCK_GAP_EMS: f32 = 1.2;

/// above this many blocks the careful ordering is skipped for a plain
/// top-to-bottom sort, which keeps damaged pages quick.
const MAX_ORDERED_BLOCKS: usize = 400;

/// the text of one page, ready to compare.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageText {
    /// the visible page size
    pub size: PageSize,
    /// how far the text is turned from upright
    pub turn: QuarterTurn,
    pub words: Vec<Word>,
    pub lines: Vec<Line>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Word {
    /// the word as written (a hyphenated word is joined without its hyphen)
    pub text: String,
    /// the form compared with the other document (see `comparison_key`)
    pub key: String,
    /// where it is on the visible page; two rectangles when it was hyphenated
    pub rects: Vec<Rect>,
    /// where its first part is in the upright frame
    pub upright: Rect,
    pub line: usize,
    pub block: usize,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub upright: Rect,
    pub visible: Rect,
    pub block: usize,
    pub words: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Block {
    pub upright: Rect,
    pub visible: Rect,
    pub lines: Range<usize>,
}

impl PageText {
    pub fn has_text(&self) -> bool {
        !self.words.is_empty()
    }

    /// the upright frame's size.
    pub fn upright_size(&self) -> PageSize {
        self.turn.upright_size(self.size)
    }

    /// a rectangle in the upright frame, on the visible page.
    pub fn to_visible(&self, upright: &Rect) -> Rect {
        self.turn.rect_to_visible(upright, self.upright_size())
    }

    /// the first few words of the page, to describe it in a sentence.
    pub fn opening_words(&self, count: usize) -> String {
        self.words
            .iter()
            .take(count)
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// a letter in the upright frame.
#[derive(Clone, Debug)]
struct Placed {
    glyph: usize,
    ch: char,
    rect: Rect,
    size: f32,
}

/// letters at one height that belong together: a line, or part of one.
#[derive(Clone, Debug)]
struct Fragment {
    letters: Vec<Placed>,
    rect: Rect,
    size: f32,
}

/// reads the words of a page from its letters.
pub(crate) fn read_page_text(glyphs: &[Glyph], size: PageSize, ignore_case: bool) -> PageText {
    let turn = dominant_turn(glyphs);
    let upright_size = turn.upright_size(size);
    let placed = place_upright(glyphs, turn, size);
    let fragments = split_rows(group_rows(placed));
    let blocks = group_blocks(&fragments);
    let order = reading_order(&blocks, &fragments);
    assemble(
        glyphs,
        &fragments,
        &blocks,
        &order,
        turn,
        size,
        upright_size,
        ignore_case,
    )
}

/// which way most of the text runs, judged from letter to letter.
fn dominant_turn(glyphs: &[Glyph]) -> QuarterTurn {
    let mut votes = [0usize; 4];
    for pair in glyphs.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.ch.is_whitespace() || b.ch.is_whitespace() {
            continue;
        }
        let dx = b.rect.center_x() - a.rect.center_x();
        let dy = b.rect.center_y() - a.rect.center_y();
        let distance = (dx * dx + dy * dy).sqrt();
        // only neighbouring letters of one word say anything about direction
        if distance < 0.01 || distance > 3.0 * a.font_size.max(1.0) {
            continue;
        }
        let vote = if dx.abs() >= dy.abs() {
            if dx > 0.0 {
                0
            } else {
                2
            }
        } else if dy > 0.0 {
            1
        } else {
            3
        };
        votes[vote] += 1;
    }
    let winner = (0..4)
        .max_by_key(|&index| (votes[index], index == 0))
        .unwrap_or(0);
    match winner {
        1 => QuarterTurn::Clockwise,
        2 => QuarterTurn::Half,
        3 => QuarterTurn::CounterClockwise,
        _ => QuarterTurn::None,
    }
}

/// every visible letter in the upright frame.
fn place_upright(glyphs: &[Glyph], turn: QuarterTurn, size: PageSize) -> Vec<Placed> {
    glyphs
        .iter()
        .enumerate()
        .filter(|(_, glyph)| !glyph.ch.is_control())
        .map(|(index, glyph)| {
            let rect = turn.rect_to_upright(&glyph.rect, size);
            Placed {
                glyph: index,
                ch: glyph.ch,
                rect,
                size: glyph.font_size.max(rect.height).max(0.5),
            }
        })
        .collect()
}

/// a letter drawn again almost on top of the same letter, a common way to
/// fake bold. it is kept once.
fn is_repeat(earlier: &Placed, letter: &Placed) -> bool {
    // measured against the letter's width, so the two l's of "small" in a
    // narrow font are still two letters
    earlier.ch == letter.ch
        && (earlier.rect.x - letter.rect.x).abs() < 0.35 * earlier.rect.width.max(0.1)
        && (earlier.rect.y - letter.rect.y).abs() < 0.2 * letter.size
}

/// puts letters whose middles are at the same height into rows.
fn group_rows(mut letters: Vec<Placed>) -> Vec<Vec<Placed>> {
    letters.sort_by(|a, b| a.rect.center_y().total_cmp(&b.rect.center_y()));
    struct Row {
        center: f32,
        height: f32,
        letters: Vec<Placed>,
    }
    let mut rows: Vec<Row> = Vec::new();
    for letter in letters {
        let center = letter.rect.center_y();
        let height = letter.rect.height.max(0.5);
        // rows are made in order of height, so only the last few can match
        let found = rows
            .iter_mut()
            .rev()
            .take_while(|row| row.center >= center - 2.0 * height.max(row.height))
            .filter(|row| (row.center - center).abs() <= 0.5 * row.height.min(height))
            .min_by(|a, b| {
                (a.center - center)
                    .abs()
                    .total_cmp(&(b.center - center).abs())
            });
        match found {
            Some(row) => {
                let count = row.letters.len() as f32;
                row.center = (row.center * count + center) / (count + 1.0);
                row.height = (row.height * count + height) / (count + 1.0);
                row.letters.push(letter);
            }
            None => rows.push(Row {
                center,
                height,
                letters: vec![letter],
            }),
        }
    }
    rows.into_iter().map(|row| row.letters).collect()
}

/// splits each row where a gap is wide enough to be a column gap.
fn split_rows(rows: Vec<Vec<Placed>>) -> Vec<Fragment> {
    let mut fragments = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        let mut current: Vec<Placed> = Vec::new();
        for letter in row {
            if current
                .last()
                .is_some_and(|earlier| is_repeat(earlier, &letter))
            {
                continue;
            }
            if let Some(previous) = current
                .iter()
                .rev()
                .find(|earlier| !earlier.ch.is_whitespace())
            {
                let gap = letter.rect.x - previous.rect.right();
                if !letter.ch.is_whitespace()
                    && gap > COLUMN_GAP_EMS * previous.size.max(letter.size)
                {
                    fragments.push(make_fragment(std::mem::take(&mut current)));
                }
            }
            current.push(letter);
        }
        if !current.is_empty() {
            fragments.push(make_fragment(current));
        }
    }
    // a fragment of only spaces says nothing
    fragments.retain(|fragment| {
        fragment
            .letters
            .iter()
            .any(|letter| !letter.ch.is_whitespace())
    });
    fragments
}

fn make_fragment(letters: Vec<Placed>) -> Fragment {
    let visible: Vec<&Placed> = letters
        .iter()
        .filter(|letter| !letter.ch.is_whitespace())
        .collect();
    let rect = Rect::union_all(visible.iter().map(|letter| &letter.rect))
        .or_else(|| Rect::union_all(letters.iter().map(|letter| &letter.rect)))
        .unwrap_or_default();
    let size = median(visible.iter().map(|letter| letter.size).collect()).unwrap_or(rect.height);
    Fragment {
        letters,
        rect,
        size,
    }
}

/// groups fragments into blocks. returns, for each block, the indexes of
/// its fragments from top to bottom.
fn group_blocks(fragments: &[Fragment]) -> Vec<Vec<usize>> {
    let mut by_top: Vec<usize> = (0..fragments.len()).collect();
    by_top.sort_by(|&a, &b| {
        fragments[a]
            .rect
            .y
            .total_cmp(&fragments[b].rect.y)
            .then(fragments[a].rect.x.total_cmp(&fragments[b].rect.x))
    });
    let mut blocks: Vec<Vec<usize>> = Vec::new();
    for index in by_top {
        let next = &fragments[index];
        let best = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| continues_block(&fragments[*block.last().unwrap()], next))
            .min_by(|(_, a), (_, b)| {
                let gap_a = next.rect.y - fragments[*a.last().unwrap()].rect.bottom();
                let gap_b = next.rect.y - fragments[*b.last().unwrap()].rect.bottom();
                gap_a.abs().total_cmp(&gap_b.abs())
            })
            .map(|(position, _)| position);
        match best {
            Some(position) => blocks[position].push(index),
            None => blocks.push(vec![index]),
        }
    }
    blocks
}

/// whether `next` is the line after `last` in the same block.
fn continues_block(last: &Fragment, next: &Fragment) -> bool {
    let size = last.size.max(next.size);
    let gap = next.rect.y - last.rect.bottom();
    if gap < -0.5 * size || gap > BLOCK_GAP_EMS * size {
        return false;
    }
    // a heading over its paragraph is a block of its own
    if (last.size - next.size).abs() > 0.25 * size {
        return false;
    }
    let same_left = (next.rect.x - last.rect.x).abs() <= size;
    let same_right = (next.rect.right() - last.rect.right()).abs() <= size;
    let mostly_overlapping =
        last.rect.horizontal_overlap(&next.rect) >= 0.7 * last.rect.width.max(next.rect.width);
    same_left || same_right || mostly_overlapping
}

/// the order blocks are read in: a block comes first when nothing still
/// unread must be read before it (something above it in the same columns,
/// or beside it on the left in the same rows).
fn reading_order(blocks: &[Vec<usize>], fragments: &[Fragment]) -> Vec<usize> {
    let bounds: Vec<Rect> = blocks
        .iter()
        .map(|block| {
            Rect::union_all(block.iter().map(|&index| &fragments[index].rect)).unwrap_or_default()
        })
        .collect();
    let top_left_first = |a: &usize, b: &usize| {
        bounds[*a]
            .y
            .total_cmp(&bounds[*b].y)
            .then(bounds[*a].x.total_cmp(&bounds[*b].x))
    };
    let mut remaining: Vec<usize> = (0..blocks.len()).collect();
    if blocks.len() > MAX_ORDERED_BLOCKS {
        remaining.sort_by(top_left_first);
        return remaining;
    }
    let mut order = Vec::with_capacity(blocks.len());
    while !remaining.is_empty() {
        let free: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&candidate| {
                !remaining.iter().any(|&other| {
                    other != candidate && must_come_before(&bounds[other], &bounds[candidate])
                })
            })
            .collect();
        // a cycle can only come from overlapping blocks; then the top left one goes first
        let pool = if free.is_empty() {
            remaining.clone()
        } else {
            free
        };
        let chosen = pool.into_iter().min_by(top_left_first).unwrap();
        remaining.retain(|&index| index != chosen);
        order.push(chosen);
    }
    order
}

fn must_come_before(a: &Rect, b: &Rect) -> bool {
    let tolerance = 1.0;
    let above = a.bottom() <= b.y + tolerance && a.horizontal_overlap(b) > 0.0;
    let left_of = a.right() <= b.x + tolerance && a.vertical_overlap(b) > 0.0;
    above || left_of
}

/// builds the final word, line and block lists in reading order.
#[allow(clippy::too_many_arguments)]
fn assemble(
    glyphs: &[Glyph],
    fragments: &[Fragment],
    blocks: &[Vec<usize>],
    order: &[usize],
    turn: QuarterTurn,
    size: PageSize,
    upright_size: PageSize,
    ignore_case: bool,
) -> PageText {
    let mut page = PageText {
        size,
        turn,
        ..PageText::default()
    };
    for &block_index in order {
        let block_number = page.blocks.len();
        let first_line = page.lines.len();
        for &fragment_index in &blocks[block_index] {
            let fragment = &fragments[fragment_index];
            let line_number = page.lines.len();
            let first_word = page.words.len();
            for letters in split_words(&fragment.letters) {
                page.words.push(make_word(
                    glyphs,
                    &letters,
                    line_number,
                    block_number,
                    ignore_case,
                ));
            }
            page.lines.push(Line {
                upright: fragment.rect,
                visible: turn.rect_to_visible(&fragment.rect, upright_size),
                block: block_number,
                words: first_word..page.words.len(),
            });
        }
        let upright = Rect::union_all(page.lines[first_line..].iter().map(|line| &line.upright))
            .unwrap_or_default();
        page.blocks.push(Block {
            upright,
            visible: turn.rect_to_visible(&upright, upright_size),
            lines: first_line..page.lines.len(),
        });
    }
    join_hyphenated_words(&mut page, ignore_case);
    page
}

/// splits a fragment's letters into words at spaces and gaps.
///
/// a gap counts as a space when it is clearly wider than the usual gap
/// between letters of this fragment, so letter-spaced text (common in
/// headings) is not broken into single letters.
fn split_words(letters: &[Placed]) -> Vec<Vec<&Placed>> {
    let tracking = usual_letter_gap(letters);
    let mut words: Vec<Vec<&Placed>> = Vec::new();
    let mut current: Vec<&Placed> = Vec::new();
    for letter in letters {
        if letter.ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some(previous) = current.last() {
            let gap = letter.rect.x - previous.rect.right();
            let size = previous.size.max(letter.size);
            if gap > tracking.min(MAX_TRACKING_EMS * size) + WORD_GAP_EMS * size {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(letter);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// the usual gap between neighbouring letters of a fragment (the lower
/// quartile, so the fewer, wider gaps between words don't count), never
/// below zero.
fn usual_letter_gap(letters: &[Placed]) -> f32 {
    let mut gaps: Vec<f32> = letters
        .windows(2)
        .filter(|pair| !pair[0].ch.is_whitespace() && !pair[1].ch.is_whitespace())
        .map(|pair| pair[1].rect.x - pair[0].rect.right())
        .collect();
    // one gap alone could as well be a space between two words
    if gaps.len() < 2 {
        return 0.0;
    }
    gaps.sort_by(f32::total_cmp);
    gaps[gaps.len() / 4].max(0.0)
}

fn make_word(
    glyphs: &[Glyph],
    letters: &[&Placed],
    line: usize,
    block: usize,
    ignore_case: bool,
) -> Word {
    let text: String = letters.iter().map(|letter| letter.ch).collect();
    let upright = Rect::union_all(letters.iter().map(|letter| &letter.rect)).unwrap_or_default();
    let visible = Rect::union_all(letters.iter().map(|letter| &glyphs[letter.glyph].rect))
        .unwrap_or_default();
    let style = dominant_style(letters.iter().map(|letter| &glyphs[letter.glyph]));
    Word {
        key: comparison_key(&text, ignore_case),
        text,
        rects: vec![visible],
        upright,
        line,
        block,
        style,
    }
}

/// the style most of a word's letters have.
fn dominant_style<'a>(glyphs: impl Iterator<Item = &'a Glyph>) -> TextStyle {
    let mut counts: Vec<(TextStyle, usize)> = Vec::new();
    for glyph in glyphs {
        let style = TextStyle::of(glyph);
        match counts.iter_mut().find(|(known, _)| *known == style) {
            Some((_, count)) => *count += 1,
            None => counts.push((style, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(style, _)| style)
        .unwrap_or_default()
}

/// the form a word is compared in: ligatures written out, invisible
/// characters (soft hyphens, zero-width spaces) dropped, and optionally
/// lower case. "ﬁnal" and "final" are the same word to a reader.
pub(crate) fn comparison_key(text: &str, ignore_case: bool) -> String {
    let mut key = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\u{fb00}' => key.push_str("ff"),
            '\u{fb01}' => key.push_str("fi"),
            '\u{fb02}' => key.push_str("fl"),
            '\u{fb03}' => key.push_str("ffi"),
            '\u{fb04}' => key.push_str("ffl"),
            '\u{fb05}' | '\u{fb06}' => key.push_str("st"),
            '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' => {}
            other if ignore_case => key.extend(other.to_lowercase()),
            other => key.push(other),
        }
    }
    key
}

/// joins "compar-" at the end of a line with "ison" at the start of the
/// next, when the next part starts with a small letter (so "well-" and
/// "Known" stay apart). the joined word keeps both rectangles.
fn join_hyphenated_words(page: &mut PageText, ignore_case: bool) {
    let mut joined: Vec<Word> = Vec::with_capacity(page.words.len());
    let mut words = std::mem::take(&mut page.words).into_iter().peekable();
    while let Some(mut word) = words.next() {
        let next_starts_line = words
            .peek()
            .map(|next| next.line != word.line && next.block == word.block)
            .unwrap_or(false);
        if next_starts_line && ends_with_break_hyphen(&word.text) {
            let starts_small = words
                .peek()
                .and_then(|next| next.text.chars().next())
                .map(|first| first.is_lowercase())
                .unwrap_or(false);
            if starts_small {
                let next = words.next().unwrap();
                word.text = format!("{}{}", trim_break_hyphen(&word.text), next.text);
                word.key = comparison_key(&word.text, ignore_case);
                word.rects.extend(next.rects);
            }
        }
        joined.push(word);
    }
    page.words = joined;
    // the word ranges of lines must follow the joined list
    let mut start = 0;
    for (number, line) in page.lines.iter_mut().enumerate() {
        let end = start
            + page.words[start..]
                .iter()
                .take_while(|word| word.line == number)
                .count();
        line.words = start..end;
        start = end;
    }
}

fn ends_with_break_hyphen(text: &str) -> bool {
    let mut letters = text.chars().rev();
    let last = letters.next();
    let before = letters.next();
    matches!(last, Some('-') | Some('\u{ad}') | Some('\u{2010}'))
        && before.map(char::is_alphabetic).unwrap_or(false)
}

fn trim_break_hyphen(text: &str) -> &str {
    text.trim_end_matches(['-', '\u{ad}', '\u{2010}'])
}

fn median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[values.len() / 2])
}

impl TextStyle {
    /// the style of one letter, with the size rounded as it is reported.
    pub(crate) fn of(glyph: &Glyph) -> TextStyle {
        TextStyle {
            font: font_family(&glyph.font_name),
            size: round_tenth(glyph.font_size),
            bold: glyph.bold,
            italic: glyph.italic,
            colour: glyph.colour,
        }
    }
}

/// the family part of a font name: "TimesNewRomanPS-BoldMT" is
/// "TimesNewRoman", "Helvetica-Oblique" is "Helvetica". bold and italic
/// are compared separately, so they are not part of the family.
pub(crate) fn font_family(name: &str) -> String {
    let name = crate::lopdf_source::strip_subset_tag(name);
    let family = name.split(['-', ',']).next().unwrap_or(name);
    let family = family
        .strip_suffix("PSMT")
        .or_else(|| family.strip_suffix("MT"))
        .or_else(|| family.strip_suffix("PS"))
        .unwrap_or(family);
    if family.is_empty() {
        name.to_owned()
    } else {
        family.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::source::Colour;

    /// lays out letters of a fixed width along a baseline, like a simple font.
    fn line_of(text: &str, x: f32, top: f32, size: f32) -> Vec<Glyph> {
        let width = size * 0.5;
        text.chars()
            .enumerate()
            .map(|(index, ch)| Glyph {
                ch,
                rect: Rect::new(x + index as f32 * width, top, width, size),
                font_name: Arc::from("Helvetica"),
                font_size: size,
                colour: Colour::BLACK,
                bold: false,
                italic: false,
            })
            .collect()
    }

    fn words_of(page: &PageText) -> Vec<&str> {
        page.words.iter().map(|word| word.text.as_str()).collect()
    }

    const LETTER: PageSize = PageSize {
        width: 612.0,
        height: 792.0,
    };

    #[test]
    fn spaces_and_gaps_split_words() {
        let mut glyphs = line_of("one two", 72.0, 100.0, 10.0);
        // "three" placed with a gap but no space character
        glyphs.extend(line_of("three", 72.0 + 7.0 * 5.0 + 3.0, 100.0, 10.0));
        let page = read_page_text(&glyphs, LETTER, false);
        assert_eq!(words_of(&page), ["one", "two", "three"]);
        assert_eq!(page.lines.len(), 1);
    }

    #[test]
    fn two_columns_read_column_by_column() {
        let mut glyphs = Vec::new();
        for row in 0..3 {
            let top = 100.0 + row as f32 * 12.0;
            glyphs.extend(line_of(&format!("left{row}"), 72.0, top, 10.0));
            glyphs.extend(line_of(&format!("right{row}"), 320.0, top, 10.0));
        }
        let page = read_page_text(&glyphs, LETTER, false);
        assert_eq!(
            words_of(&page),
            ["left0", "left1", "left2", "right0", "right1", "right2"]
        );
        assert_eq!(page.blocks.len(), 2);
    }

    #[test]
    fn a_title_over_two_columns_comes_first() {
        let mut glyphs = line_of("Title", 250.0, 60.0, 10.0);
        for row in 0..2 {
            let top = 100.0 + row as f32 * 12.0;
            glyphs.extend(line_of(&format!("right{row}"), 320.0, top, 10.0));
            glyphs.extend(line_of(&format!("left{row}"), 72.0, top, 10.0));
        }
        let page = read_page_text(&glyphs, LETTER, false);
        assert_eq!(
            words_of(&page),
            ["Title", "left0", "left1", "right0", "right1"]
        );
    }

    #[test]
    fn hyphenated_words_are_joined_across_lines() {
        let mut glyphs = line_of("a compar-", 72.0, 100.0, 10.0);
        glyphs.extend(line_of("ison of well-", 72.0, 112.0, 10.0));
        glyphs.extend(line_of("Known", 72.0, 124.0, 10.0));
        let page = read_page_text(&glyphs, LETTER, false);
        assert_eq!(words_of(&page), ["a", "comparison", "of", "well-", "Known"]);
        assert_eq!(page.words[1].rects.len(), 2);
        assert_eq!(page.lines[1].words, 2..4);
    }

    #[test]
    fn sideways_text_is_turned_upright() {
        // text running down the page, as on a page turned clockwise
        let glyphs: Vec<Glyph> = "down"
            .chars()
            .enumerate()
            .map(|(index, ch)| Glyph {
                ch,
                rect: Rect::new(500.0, 72.0 + index as f32 * 5.0, 10.0, 5.0),
                font_name: Arc::from("Helvetica"),
                font_size: 10.0,
                colour: Colour::BLACK,
                bold: false,
                italic: false,
            })
            .collect();
        let page = read_page_text(&glyphs, PageSize::new(792.0, 612.0), false);
        assert_eq!(page.turn, QuarterTurn::Clockwise);
        assert_eq!(words_of(&page), ["down"]);
    }

    #[test]
    fn letters_drawn_twice_count_once() {
        let mut glyphs = line_of("bold", 72.0, 100.0, 10.0);
        glyphs.extend(line_of("bold", 72.4, 100.0, 10.0));
        let page = read_page_text(&glyphs, LETTER, false);
        assert_eq!(words_of(&page), ["bold"]);
    }

    #[test]
    fn keys_write_out_ligatures_and_drop_soft_hyphens() {
        assert_eq!(comparison_key("\u{fb01}nal", false), "final");
        assert_eq!(comparison_key("co\u{ad}operate", false), "cooperate");
        assert_eq!(comparison_key("Hello", true), "hello");
    }

    #[test]
    fn families_drop_style_and_vendor_suffixes() {
        assert_eq!(font_family("TimesNewRomanPS-BoldMT"), "TimesNewRoman");
        assert_eq!(font_family("ABCDEF+Arial-BoldMT"), "Arial");
        assert_eq!(font_family("Helvetica"), "Helvetica");
        assert_eq!(font_family("Times-Roman"), "Times");
    }
}
