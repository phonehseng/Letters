//! the pieces of text the analysis works with.
//!
//! letters-edit reads a page's text as lines. a line can run across a
//! table's columns or a page's columns when they share a baseline, so it
//! is cut wherever the gap between two letters is wider than a word gap.
//! each piece (a fragment) is a stretch of words on one baseline. list
//! markers drawn apart from their text are joined back to it, and small
//! raised numbers are joined to the text they belong to as superscripts.

use letters_edit::{FontFamily, PageObjects, TextLine};

use super::fonts::family_name;
use super::links::{link_at, PageLink};
use super::markers::list_marker;
use super::model::{GenericFamily, TextRun};
use crate::content::Rgb;
use crate::geometry::Rect;

/// a gap wider than this many times the text size ends a fragment.
/// word gaps are about a quarter of the size; justified lines stretch
/// them, but rarely past one; table columns are further apart.
pub(crate) const FRAGMENT_GAP: f32 = 1.4;
/// a gap wider than this many times the size is a space between words
const WORD_GAP: f32 = 0.15;

/// one character with its box and style.
#[derive(Clone, Debug)]
pub(crate) struct Glyph {
    pub ch: char,
    pub rect: Rect,
    /// the style of the run the character belongs to (its text is empty)
    pub style: TextRun,
}

/// a stretch of words on one baseline.
#[derive(Clone, Debug)]
pub(crate) struct Fragment {
    pub rect: Rect,
    /// distance of the baseline from the top of the page
    pub baseline: f32,
    /// the size most of its characters have
    pub size: f32,
    pub glyphs: Vec<Glyph>,
    /// the text doesn't run left to right
    pub rotated: bool,
    /// the direction the baseline runs, in degrees clockwise on screen
    pub angle: f32,
    /// where the baseline starts on the page
    pub origin: (f32, f32),
    /// drawn invisibly, like the text layer over a scan
    pub invisible: bool,
}

impl Fragment {
    /// the text with spaces where the words are apart.
    pub fn text(&self) -> String {
        self.runs().iter().map(|run| run.text.as_str()).collect()
    }

    /// the glyphs gathered into runs of one style, with spaces put in
    /// where words are apart but no space character was drawn.
    pub fn runs(&self) -> Vec<TextRun> {
        let mut runs: Vec<TextRun> = self.placed_runs().into_iter().map(|(run, _)| run).collect();
        trim_runs(&mut runs);
        runs
    }

    /// the runs with the box of each run's characters, for writers that
    /// place text exactly. spaces at the ends are kept, so the boxes and
    /// the text match.
    pub fn placed_runs(&self) -> Vec<(TextRun, Rect)> {
        let mut runs: Vec<(TextRun, Rect)> = Vec::new();
        let mut previous: Option<&Glyph> = None;
        for glyph in &self.glyphs {
            if let Some(before) = previous {
                let gap = glyph.rect.x - before.rect.right();
                let needs_space = gap > WORD_GAP * self.size.max(1.0)
                    && !before.ch.is_whitespace()
                    && !glyph.ch.is_whitespace()
                    && !self.rotated;
                if needs_space {
                    push_text(&mut runs, &before.style, ' ', None);
                }
            }
            push_text(&mut runs, &glyph.style, glyph.ch, Some(glyph.rect));
            previous = Some(glyph);
        }
        runs
    }

    /// every character that isn't a space is bold.
    pub fn is_bold(&self) -> bool {
        self.glyphs
            .iter()
            .filter(|g| !g.ch.is_whitespace())
            .all(|g| g.style.bold)
    }

    pub fn char_count(&self) -> usize {
        self.glyphs.iter().filter(|g| !g.ch.is_whitespace()).count()
    }

    fn update_bounds(&mut self) {
        self.glyphs.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        let rect = self
            .glyphs
            .iter()
            .map(|g| g.rect)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        self.rect = rect;
        self.origin = (rect.x, self.baseline);
    }
}

fn push_text(runs: &mut Vec<(TextRun, Rect)>, style: &TextRun, ch: char, rect: Option<Rect>) {
    if let Some((last, last_rect)) = runs.last_mut() {
        if same_style(last, style) {
            last.text.push(ch);
            if let Some(rect) = rect {
                *last_rect = last_rect.union(&rect);
            }
            return;
        }
    }
    let mut run = style.clone();
    run.text = ch.to_string();
    runs.push((run, rect.unwrap_or_default()));
}

pub(crate) fn same_style(a: &TextRun, b: &TextRun) -> bool {
    a.font_ref == b.font_ref
        && a.font_family == b.font_family
        && a.bold == b.bold
        && a.italic == b.italic
        && (a.size - b.size).abs() < 0.05
        && a.color == b.color
        && a.link == b.link
        && a.superscript == b.superscript
}

/// drops spaces at the start and end, and runs left empty.
fn trim_runs(runs: &mut Vec<TextRun>) {
    if let Some(first) = runs.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = runs.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    runs.retain(|run| !run.text.is_empty());
}

/// the fragments of a page, top to bottom, with links applied.
pub(crate) fn page_fragments(objects: &PageObjects, links: &[PageLink]) -> Vec<Fragment> {
    let mut fragments: Vec<Fragment> = objects
        .text_blocks
        .iter()
        .flat_map(|block| block.lines.iter())
        .flat_map(|line| split_line(line, links))
        .filter(|fragment| fragment.char_count() > 0)
        .collect();
    fragments.sort_by(|a, b| {
        a.baseline
            .total_cmp(&b.baseline)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
    attach_superscripts(&mut fragments);
    join_markers(&mut fragments);
    fragments
}

fn generic(family: FontFamily) -> GenericFamily {
    match family {
        FontFamily::Serif => GenericFamily::Serif,
        FontFamily::SansSerif => GenericFamily::SansSerif,
        FontFamily::Monospace => GenericFamily::Monospace,
    }
}

/// cuts one line at its wide gaps.
fn split_line(line: &TextLine, links: &[PageLink]) -> Vec<Fragment> {
    let [start, end] = line.baseline;
    let rotated = (end.y - start.y).abs() > (end.x - start.x).abs() * 0.1 + 0.5;
    let baseline = start.y.max(end.y);
    let turn = Turn {
        rotated,
        angle: (end.y - start.y).atan2(end.x - start.x).to_degrees(),
        origin: (start.x, start.y),
    };
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut current: Vec<Glyph> = Vec::new();
    let mut invisible = true;
    let mut sizes: Vec<f32> = Vec::new();
    for run in &line.runs {
        let style = TextRun {
            text: String::new(),
            font_family: family_name(&run.font.name),
            generic_family: generic(run.font.family),
            bold: run.font.bold,
            italic: run.font.italic,
            size: (run.size * 10.0).round() / 10.0,
            color: Rgb(run.color.r, run.color.g, run.color.b),
            link: None,
            superscript: false,
            font_ref: run.font.id.clone(),
        };
        invisible &= run.invisible;
        for (ch, rect) in run.text.chars().zip(&run.char_boxes) {
            let rect = Rect::from_edit(*rect);
            let mut style = style.clone();
            style.link = link_at(links, rect.center_x(), rect.center_y()).map(str::to_owned);
            if let Some(last) = current.last() {
                let gap = rect.x - last.rect.right();
                if !rotated && gap > FRAGMENT_GAP * run.size.max(1.0) {
                    fragments.push(make_fragment(
                        std::mem::take(&mut current),
                        baseline,
                        &turn,
                        invisible,
                        &sizes,
                    ));
                    sizes.clear();
                }
            }
            sizes.push(run.size);
            current.push(Glyph { ch, rect, style });
        }
    }
    if !current.is_empty() {
        fragments.push(make_fragment(current, baseline, &turn, invisible, &sizes));
    }
    fragments
}

/// which way a line runs.
struct Turn {
    rotated: bool,
    angle: f32,
    origin: (f32, f32),
}

fn make_fragment(
    glyphs: Vec<Glyph>,
    baseline: f32,
    turn: &Turn,
    invisible: bool,
    sizes: &[f32],
) -> Fragment {
    let origin = if turn.rotated {
        turn.origin
    } else {
        // a cut line starts again at its first character
        (
            glyphs.first().map(|g| g.rect.x).unwrap_or(turn.origin.0),
            baseline,
        )
    };
    let mut fragment = Fragment {
        rect: Rect::default(),
        baseline,
        size: most_common_size(sizes),
        glyphs,
        rotated: turn.rotated,
        angle: if turn.rotated { turn.angle } else { 0.0 },
        origin,
        invisible,
    };
    if turn.rotated {
        fragment.rect = fragment
            .glyphs
            .iter()
            .map(|g| g.rect)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
    } else {
        fragment.update_bounds();
    }
    fragment
}

/// the size most characters have, rounded to a tenth of a point.
pub(crate) fn most_common_size(sizes: &[f32]) -> f32 {
    let mut counts: Vec<(i32, usize)> = Vec::new();
    for size in sizes {
        let key = (size * 10.0).round() as i32;
        match counts.iter_mut().find(|(k, _)| *k == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    counts
        .iter()
        .max_by_key(|(key, count)| (*count, *key))
        .map(|(key, _)| *key as f32 / 10.0)
        .unwrap_or(0.0)
}

/// joins small raised fragments ("1", "*", "a") to the text beside them
/// as superscripts.
fn attach_superscripts(fragments: &mut Vec<Fragment>) {
    let mut index = 0;
    while index < fragments.len() {
        let small = &fragments[index];
        let host = (small.char_count() <= 3 && !small.rotated)
            .then(|| {
                fragments.iter().position(|other| {
                    !std::ptr::eq(other, small)
                        && !other.rotated
                        && small.size <= other.size * 0.85
                        && small.baseline < other.baseline - other.size * 0.15
                        && small.baseline > other.baseline - other.size * 1.0
                        && touches_horizontally(&small.rect, &other.rect, other.size)
                })
            })
            .flatten();
        match host {
            Some(host) => {
                let mut small = fragments.remove(index);
                let host = if host > index { host - 1 } else { host };
                for glyph in &mut small.glyphs {
                    glyph.style.superscript = true;
                }
                fragments[host].glyphs.extend(small.glyphs);
                fragments[host].update_bounds();
            }
            None => index += 1,
        }
    }
}

/// next to each other on a line: the small one starts just after the
/// other ends, ends just before it starts, or sits inside its gap.
fn touches_horizontally(small: &Rect, other: &Rect, size: f32) -> bool {
    let slack = size * 0.6;
    small.x >= other.x - slack && small.right() <= other.right() + slack + small.width
}

/// joins a fragment that is only a list marker ("•", "3.") to the text
/// that follows it on the same baseline.
fn join_markers(fragments: &mut Vec<Fragment>) {
    let mut index = 0;
    while index < fragments.len() {
        let marker = &fragments[index];
        let text = marker.text();
        let is_marker = list_marker(&text)
            .is_some_and(|m| m.text.chars().count() == text.trim().chars().count());
        let next = is_marker
            .then(|| {
                fragments.iter().position(|other| {
                    !std::ptr::eq(other, marker)
                        && (other.baseline - marker.baseline).abs() < marker.size * 0.3
                        && other.rect.x > marker.rect.right()
                        && other.rect.x - marker.rect.right() < marker.size.max(other.size) * 4.0
                })
            })
            .flatten();
        match next {
            Some(next) => {
                let marker = fragments.remove(index);
                let next = if next > index { next - 1 } else { next };
                let size = fragments[next].size;
                fragments[next].glyphs.splice(0..0, marker.glyphs);
                fragments[next].size = size;
                fragments[next].update_bounds();
            }
            None => index += 1,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// a fragment of evenly spaced characters, for tests of later steps.
    pub(crate) fn fragment(text: &str, x: f32, baseline: f32, size: f32, bold: bool) -> Fragment {
        let width = size * 0.5;
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                rect: Rect::new(x + i as f32 * width, baseline - size * 0.8, width, size),
                style: TextRun {
                    text: String::new(),
                    font_family: "Helvetica".into(),
                    generic_family: GenericFamily::SansSerif,
                    bold,
                    italic: false,
                    size,
                    color: Rgb::BLACK,
                    link: None,
                    superscript: false,
                    font_ref: "font:1-0".into(),
                },
            })
            .collect();
        let mut fragment = Fragment {
            rect: Rect::default(),
            baseline,
            size,
            glyphs,
            rotated: false,
            angle: 0.0,
            origin: (x, baseline),
            invisible: false,
        };
        fragment.update_bounds();
        fragment
    }

    #[test]
    fn spaces_are_put_in_word_gaps() {
        let mut a = fragment("Hello", 0.0, 100.0, 10.0, false);
        let b = fragment("world", 30.0, 100.0, 10.0, false);
        a.glyphs.extend(b.glyphs);
        assert_eq!(a.text(), "Hello world");
    }

    #[test]
    fn markers_join_their_text() {
        let mut fragments = vec![
            fragment("•", 72.0, 100.0, 12.0, false),
            fragment("First point", 90.0, 100.0, 12.0, false),
        ];
        join_markers(&mut fragments);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].text(), "• First point");
    }

    #[test]
    fn raised_small_numbers_become_superscripts() {
        let mut fragments = vec![
            fragment("Energy", 72.0, 100.0, 12.0, false),
            fragment("2", 108.5, 96.0, 7.0, false),
        ];
        attach_superscripts(&mut fragments);
        assert_eq!(fragments.len(), 1);
        let runs = fragments[0].runs();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].text, "2");
        assert!(runs[1].superscript);
    }

    #[test]
    fn common_size_wins() {
        assert_eq!(most_common_size(&[12.0, 12.0, 8.0]), 12.0);
    }
}
