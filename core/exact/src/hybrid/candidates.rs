//! candidate construction: one detected paragraph written as one real
//! word paragraph, at one of three levels of constraint, least intrusive
//! first. word wraps the lines itself (no forced breaks, except the pins
//! of the last level), so typing into the paragraph re-wraps it.
//!
//! the line start and every glyph target come from round one's plan
//! (recipe 7.2 and 7.4); what changes is how much of it the docx asks for:
//!
//! - `Natural`: left aligned text only fixes the start of every word (the
//!   spacing goes on the space before it); justified text only fixes the
//!   glyphs inside words and lets word spread the spaces.
//! - `EveryGlyph`: every glyph gets the spacing that puts it on its pixel,
//!   exactly round one's plan, inside one paragraph.
//! - `Pinned`: as `EveryGlyph`, left aligned, with a manual line break
//!   (`w:br`) after each line whose break word would not make by itself.
//!
//! every glyph keeps its own run with alternating east asian language
//! tags: inside one text object word's pdf writer places glyphs by its own
//! rounding (recipe 6.4), and the person never sees runs anyway.

use serde::{Deserialize, Serialize};

use super::paragraphs::{Alignment, DetectedParagraph};
use super::rules::{twips_for_indent, JUSTIFY_SQUEEZE};
use super::PlannedLine;
use crate::calibration::model::WordModel;
use crate::docx::xml::runs_xml;
use crate::plan::glyphs::CharRun;
use crate::units::round_half_even;
use crate::word::spacing::{spacing_for_pixel_steps, spacing_pixels};
use crate::word::writer_model::twips_for_pixel;

/// how much of round one's plan a candidate asks of word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Natural,
    EveryGlyph,
    Pinned,
}

impl Level {
    pub const LADDER: [Level; 3] = [Level::Natural, Level::EveryGlyph, Level::Pinned];
}

/// which steps between glyphs keep the source's position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Steps {
    /// the start of every word lands on its pixel; inside words the font's advances
    WordStarts,
    /// steps inside words keep the source's; spaces take the font's advance
    InsideWords,
    /// every glyph lands on its pixel
    Every,
}

/// the pixel targets word should produce, from the line start, for one of
/// the step choices. `targets` are round one's (recipe 7.4).
pub fn effective_targets(
    targets: &[i64],
    advances: &[i64],
    is_space: &[bool],
    steps: Steps,
) -> Vec<i64> {
    let mut effective = Vec::with_capacity(targets.len());
    for index in 0..targets.len() {
        if index == 0 {
            effective.push(0);
            continue;
        }
        let previous = effective[index - 1];
        let natural = previous + advances[index - 1];
        let source_step = targets[index] - targets[index - 1];
        let after_space = is_space[index - 1];
        effective.push(match steps {
            Steps::Every => targets[index] - targets[0],
            Steps::WordStarts if after_space && !is_space[index] => targets[index] - targets[0],
            Steps::WordStarts => natural,
            Steps::InsideWords if after_space || is_space[index] => natural,
            Steps::InsideWords => previous + source_step,
        });
    }
    effective
}

/// one line of a candidate, as word should lay it out.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateLine {
    /// layout pixel of the line start, from the page's left edge
    pub start_px: i64,
    /// pixels from the line start, per glyph
    pub pens: Vec<i64>,
    pub runs: Vec<CharRun>,
    /// pixels from the line start to the end of the content
    pub right_px: i64,
    /// pixels from the line start to the end of the first word
    pub first_word_px: i64,
    /// pixels of the line's spaces (what justification may squeeze)
    pub spaces_px: i64,
    /// width of the space that joins this line to the next (0 after a hyphen)
    pub joiner_px: i64,
    pub ends_with_hyphen: bool,
}

/// a paragraph ready to be written: geometry in word's own units.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub page: usize,
    /// indexes into the page's planned lines
    pub lines: Vec<usize>,
    pub level: Level,
    /// written with `w:jc w:val="both"`
    pub justified: bool,
    /// layout pixels from the page's left edge
    pub first_start_px: i64,
    pub body_start_px: i64,
    /// the right edge of the text area
    pub right_px: i64,
    /// the range of right edges that keep every unpinned break: (lowest, highest)
    pub window_px: (i64, i64),
    /// twips from the page top to the first line's top
    pub top_twips: i64,
    pub line_twips: i64,
    /// line k is followed by a manual line break
    pub pins: Vec<usize>,
    #[serde(skip)]
    pub runs: Vec<CharRun>,
    /// run indexes after which a `w:br` goes
    #[serde(skip)]
    pub breaks_after: Vec<usize>,
}

/// the frame a candidate is written in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBox {
    /// layout pixel of the frame's left edge
    pub left_px: i64,
    /// layout pixel of the right edge of its text area
    pub right_px: i64,
    pub top_twips: i64,
}

impl Candidate {
    /// the frame that holds this paragraph alone, exactly where it was proven.
    pub fn own_frame(&self) -> FrameBox {
        FrameBox {
            left_px: self.first_start_px.min(self.body_start_px),
            right_px: self.right_px,
            top_twips: self.top_twips,
        }
    }

    /// the paragraph's xml in `frame`, with `before` twips of space before.
    pub fn xml(&self, frame: &FrameBox, before_twips: i64) -> String {
        let left = twips_for_indent(self.body_start_px - frame.left_px);
        let combined = twips_for_indent(self.first_start_px - frame.left_px);
        let first = combined - left;
        let right = twips_for_indent(frame.right_px - self.right_px).max(0);
        let first_attribute = if first >= 0 {
            format!(r#"w:firstLine="{first}""#)
        } else {
            format!(r#"w:hanging="{}""#, -first)
        };
        let justify = if self.justified {
            r#"<w:jc w:val="both"/>"#
        } else {
            ""
        };
        format!(
            r#"<w:p><w:pPr>{}{NEUTRAL}<w:spacing w:before="{before_twips}" w:after="0" w:line="{}" w:lineRule="exact"/><w:ind w:left="{left}" w:right="{right}" {first_attribute}/>{justify}</w:pPr>{}</w:p>"#,
            frame_properties(frame),
            self.line_twips,
            self.runs_with_breaks()
        )
    }

    fn runs_with_breaks(&self) -> String {
        let mut xml = String::new();
        let mut start = 0;
        for &end in &self.breaks_after {
            xml.push_str(&runs_xml(&self.runs[start..=end]));
            xml.push_str("<w:r><w:br/></w:r>");
            start = end + 1;
        }
        xml.push_str(&runs_xml(&self.runs[start..]));
        xml
    }
}

/// the layout-neutral paragraph properties of recipe 7.1, in the order the
/// schema gives them (round one's frames keep their own order).
const NEUTRAL: &str = concat!(
    r#"<w:widowControl w:val="0"/><w:suppressAutoHyphens/>"#,
    r#"<w:autoSpaceDE w:val="0"/><w:autoSpaceDN w:val="0"/>"#,
    r#"<w:adjustRightInd w:val="0"/><w:snapToGrid w:val="0"/>"#
);

/// a frame anchored to the page; no height, so it grows with its text.
/// paragraphs with the same frame properties share one frame and stack
/// (measured: 234 of 234 lines below each other, `tests/hybrid_probe.rs`).
pub fn frame_properties(frame: &FrameBox) -> String {
    let x = twips_for_pixel(frame.left_px);
    let width = twips_for_indent(frame.right_px - frame.left_px);
    format!(
        r#"<w:framePr w:w="{width}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{x}" w:y="{}"/>"#,
        frame.top_twips
    )
}

/// the space run that joins two lines, in the font of the line's last glyph.
fn joiner(last: &CharRun) -> CharRun {
    CharRun {
        char: " ".into(),
        spacing_twips: 0,
        hidden: false,
        ..last.clone()
    }
}

fn hyphen_end(text: &str) -> bool {
    text.ends_with(['-', '\u{2010}', '\u{00AD}'])
}

/// the steps a level uses for one alignment.
fn steps_for(level: Level, alignment: Alignment) -> Steps {
    match (level, alignment) {
        (Level::Natural, Alignment::Left) => Steps::WordStarts,
        (Level::Natural, Alignment::Justified) => Steps::InsideWords,
        _ => Steps::Every,
    }
}

/// one line laid out with the chosen steps.
fn candidate_line(line: &PlannedLine, start_px: i64, steps: Steps, space_px: i64) -> CandidateLine {
    let plan = &line.plan;
    let is_space: Vec<bool> = line.runs.iter().map(|r| r.char == " ").collect();
    let pens = effective_targets(&plan.targets_px, &plan.advances_px, &is_space, steps);
    let spacing = spacing_for_pixel_steps(&plan.advances_px, &pens)
        .expect("effective targets start at the line start");
    let runs: Vec<CharRun> = line
        .runs
        .iter()
        .zip(&spacing)
        .map(|(run, &twips)| CharRun {
            spacing_twips: twips,
            ..run.clone()
        })
        .collect();
    let last = pens.len() - 1;
    let right_px = pens[last] + plan.advances_px[last] + spacing_pixels(spacing[last]);
    let first_space = is_space.iter().position(|&s| s).unwrap_or(pens.len());
    let first_word_px = if first_space == 0 {
        0
    } else {
        pens[first_space - 1] + plan.advances_px[first_space - 1]
    };
    let spaces_px = is_space
        .iter()
        .zip(&plan.advances_px)
        .filter(|(s, _)| **s)
        .map(|(_, a)| a)
        .sum();
    let ends_with_hyphen = hyphen_end(&plan.text);
    CandidateLine {
        start_px,
        pens,
        runs,
        right_px,
        first_word_px,
        spaces_px,
        joiner_px: if ends_with_hyphen { 0 } else { space_px },
        ends_with_hyphen,
    }
}

/// the font's space advance in pixels: from a space in the paragraph, else
/// a quarter em at its size (the rebuilt faces' space, recipe 3).
fn space_pixels(lines: &[&PlannedLine]) -> i64 {
    for line in lines {
        for (run, advance) in line.runs.iter().zip(&line.plan.advances_px) {
            if run.char == " " {
                return *advance;
            }
        }
    }
    let size = lines[0].line.max_size();
    round_half_even(0.25 * size / crate::units::PT_PER_PIXEL) as i64
}

/// the most common value (the earliest on a tie).
fn most_common(values: &[i64]) -> i64 {
    let mut best = (values[0], 0);
    for &value in values {
        let count = values.iter().filter(|&&v| v == value).count();
        if count > best.1 {
            best = (value, count);
        }
    }
    best.0
}

/// the lowest and highest right edge (absolute pixels) that keep every
/// break of the lines that aren't pinned.
fn break_window(lines: &[CandidateLine], justified: bool, pinned: &[usize]) -> (i64, i64) {
    let lowest = lines
        .iter()
        .map(|l| l.start_px + l.right_px)
        .max()
        .unwrap_or(0);
    let mut highest = i64::MAX;
    for (k, line) in lines.iter().enumerate().take(lines.len().saturating_sub(1)) {
        if pinned.contains(&k) {
            continue;
        }
        let next = &lines[k + 1];
        let with_next = line.start_px + line.right_px + line.joiner_px + next.first_word_px;
        // a justified line may take its next word by squeezing its spaces
        let squeeze = if justified {
            (JUSTIFY_SQUEEZE * (line.spaces_px + line.joiner_px) as f64).ceil() as i64
        } else {
            0
        };
        highest = highest.min(with_next - squeeze - 1);
    }
    (lowest, highest)
}

/// what the builder needs about the page.
pub struct PageGeometry {
    pub width_pt: f64,
    pub height_pt: f64,
}

/// builds the candidate of one paragraph at one level; `None` when the
/// level can't keep the source's breaks (the next level is tried instead).
pub fn build_candidate(
    page: usize,
    lines: &[PlannedLine],
    paragraph: &DetectedParagraph,
    level: Level,
    geometry: &PageGeometry,
    model: &mut WordModel,
    nudge_px: i64,
) -> Option<Candidate> {
    let members: Vec<&PlannedLine> = paragraph.lines.iter().map(|&i| &lines[i]).collect();
    let starts: Vec<i64> = members
        .iter()
        .map(|l| l.plan.start_pixel)
        .collect::<Option<_>>()?;
    let first_start = starts[0];
    let body_start = if starts.len() > 1 {
        most_common(&starts[1..])
    } else {
        first_start
    };
    let justified_source = paragraph.alignment == Alignment::Justified;
    let justified = justified_source && level != Level::Pinned;
    let steps = steps_for(level, paragraph.alignment);
    let space_px = space_pixels(&members);
    let laid: Vec<CandidateLine> = members
        .iter()
        .enumerate()
        .map(|(k, line)| {
            let start = if k == 0 { first_start } else { body_start };
            candidate_line(line, start, steps, space_px)
        })
        .collect();
    let (right_px, window, pins) = choose_right_edge(
        &members,
        &laid,
        level,
        justified,
        justified_source,
        body_start,
        space_px,
        nudge_px,
    )?;
    let placement = super::vertical::vertical_placement(&members, geometry, model, paragraph.pitch);
    let (runs, breaks_after) = joined_runs(&laid, &pins);
    Some(Candidate {
        page,
        lines: paragraph.lines.clone(),
        level,
        justified,
        first_start_px: first_start,
        body_start_px: body_start,
        right_px,
        window_px: window,
        top_twips: placement.top_twips,
        line_twips: placement.line_twips,
        pins,
        runs,
        breaks_after,
    })
}

/// the right edge of the text area, the break window and the pins.
#[allow(clippy::too_many_arguments)]
fn choose_right_edge(
    members: &[&PlannedLine],
    laid: &[CandidateLine],
    level: Level,
    justified: bool,
    justified_source: bool,
    body_start: i64,
    space_px: i64,
    nudge_px: i64,
) -> Option<(i64, (i64, i64), Vec<usize>)> {
    if level == Level::Pinned {
        let (lowest, _) = break_window(laid, false, &[]);
        let right = lowest + 2 + nudge_px;
        let pins: Vec<usize> = (0..laid.len().saturating_sub(1))
            .filter(|&k| {
                let line = &laid[k];
                line.start_px + line.right_px + line.joiner_px + laid[k + 1].first_word_px <= right
            })
            .collect();
        let (low, high) = break_window(laid, false, &pins);
        return Some((right, (low, high), pins));
    }
    let (lowest, highest) = break_window(laid, justified, &[]);
    if highest < lowest {
        return None;
    }
    let right = if justified {
        // word spreads each line to the right edge: it must be the source's
        let every: Vec<CandidateLine> = members
            .iter()
            .enumerate()
            .map(|(k, line)| {
                let start = if k == 0 { laid[0].start_px } else { body_start };
                candidate_line(line, start, Steps::Every, space_px)
            })
            .collect();
        let body = &every[..every.len() - 1];
        let edge = body.iter().map(|l| l.start_px + l.right_px).max()?;
        if !justified_source || edge < lowest || edge > highest {
            return None;
        }
        edge + nudge_px
    } else {
        lowest + ((highest - lowest) / 2).min(2) + nudge_px
    };
    Some((right, (lowest, highest), Vec::new()))
}

/// every line's runs in one paragraph: a space joins the lines (none after
/// a hyphen), and a pinned line is followed by a manual break.
fn joined_runs(laid: &[CandidateLine], pins: &[usize]) -> (Vec<CharRun>, Vec<usize>) {
    let mut runs = Vec::new();
    let mut breaks = Vec::new();
    for (k, line) in laid.iter().enumerate() {
        runs.extend(line.runs.iter().cloned());
        if k + 1 == laid.len() {
            break;
        }
        if !line.ends_with_hyphen {
            runs.push(joiner(line.runs.last().expect("lines have runs")));
        }
        if pins.contains(&k) {
            breaks.push(runs.len() - 1);
        }
    }
    (runs, breaks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_starts_keep_only_the_starts_of_words() {
        // "ab cd": advances 10, targets drift by a pixel inside each word
        let targets = [0, 11, 21, 26, 37];
        let advances = [10, 10, 5, 10, 10];
        let spaces = [false, false, true, false, false];
        let starts = effective_targets(&targets, &advances, &spaces, Steps::WordStarts);
        assert_eq!(starts, [0, 10, 20, 26, 36]);
        let inside = effective_targets(&targets, &advances, &spaces, Steps::InsideWords);
        assert_eq!(inside, [0, 11, 21, 26, 37]);
        let every = effective_targets(&targets, &advances, &spaces, Steps::Every);
        assert_eq!(every, targets);
    }

    #[test]
    fn inside_words_lets_the_spaces_take_the_font_advance() {
        // a stretched space (target 30 instead of 25) shrinks back to its advance
        let targets = [0, 10, 20, 30, 40];
        let advances = [10, 10, 5, 10, 10];
        let spaces = [false, false, true, false, false];
        let inside = effective_targets(&targets, &advances, &spaces, Steps::InsideWords);
        assert_eq!(inside, [0, 10, 20, 25, 35]);
    }

    fn laid(start: i64, right: i64, first_word: i64) -> CandidateLine {
        CandidateLine {
            start_px: start,
            pens: vec![0],
            runs: Vec::new(),
            right_px: right,
            first_word_px: first_word,
            spaces_px: 40,
            joiner_px: 20,
            ends_with_hyphen: false,
        }
    }

    #[test]
    fn the_window_lies_between_the_longest_line_and_the_next_word() {
        // lines end at 600 + 1000 and 600 + 990; the second line's first word is 80 px
        let lines = [laid(600, 1000, 50), laid(600, 990, 80), laid(600, 400, 60)];
        // line 0 with the next word: 1600 + 20 + 80 = 1700; line 1: 1590 + 20 + 60 = 1670
        assert_eq!(break_window(&lines, false, &[]), (1600, 1669));
        // justified lines may squeeze a quarter of their 60 px of spaces
        assert_eq!(break_window(&lines, true, &[]), (1600, 1654));
        // a pinned line no longer bounds the window
        assert_eq!(break_window(&lines, false, &[1]), (1600, 1699));
    }

    #[test]
    fn pins_go_where_the_next_word_would_fit() {
        let runs = |n: usize| -> Vec<CharRun> {
            (0..n)
                .map(|_| CharRun {
                    char: "a".into(),
                    family: "F".into(),
                    half_points: 20,
                    color: "000000".into(),
                    spacing_twips: 0,
                    hidden: false,
                })
                .collect()
        };
        let mut lines = vec![laid(600, 1000, 50), laid(600, 500, 80), laid(600, 400, 60)];
        for line in &mut lines {
            line.runs = runs(3);
        }
        let (all, breaks) = joined_runs(&lines, &[1]);
        // 3 + joiner + 3 + joiner + 3, a break after the second joiner
        assert_eq!(all.len(), 11);
        assert_eq!(breaks, [7]);
        assert_eq!(all[3].char, " ");
    }

    #[test]
    fn frames_have_no_height_so_they_grow() {
        let frame = FrameBox {
            left_px: 600,
            right_px: 3000,
            top_twips: 1440,
        };
        let xml = frame_properties(&frame);
        assert!(xml.contains(r#"w:x="1440""#));
        assert!(!xml.contains("w:h="));
        // 2400 px: 5759 twips is the smallest width word turns into 2400 px
        assert!(xml.contains(r#"w:w="5759""#));
    }
}
