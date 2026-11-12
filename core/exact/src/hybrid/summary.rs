//! summaries: what paragraph detection needs to know about each planned
//! line, and the editability numbers of the final arrangement.

use std::collections::BTreeMap;

use super::candidates::Level;
use super::ladder::{page_of, Slot, SlotState};
use super::paragraphs::{self, LineSummary};
use super::proof::{measured, GlyphKey};
use super::stacks::StackReport;
use super::{HybridReport, NotPromoted, PlannedLine, PlannedPage, PromotedParagraph};
use crate::plan::glyphs::CharRun;
use crate::units::PT_PER_PIXEL;

/// summaries of a page's lines for paragraph detection.
pub fn summarize(lines: &[PlannedLine]) -> Vec<LineSummary> {
    lines.iter().map(summarize_line).collect()
}

fn summarize_line(planned: &PlannedLine) -> LineSummary {
    let (line, plan, runs) = (&planned.line, &planned.plan, &planned.runs);
    let is_space: Vec<bool> = runs.iter().map(|r| r.char == " ").collect();
    let space_px = is_space
        .iter()
        .zip(&plan.advances_px)
        .find(|(s, _)| **s)
        .map(|(_, a)| *a as f64)
        .unwrap_or(0.25 * line.max_size() / PT_PER_PIXEL);
    let first_space = is_space.iter().position(|&s| s);
    let first_word_px = match first_space {
        Some(0) => 0,
        Some(j) => plan.targets_px[j - 1] + plan.advances_px[j - 1],
        None => {
            plan.targets_px.last().copied().unwrap_or(0)
                + plan.advances_px.last().copied().unwrap_or(0)
        }
    };
    let marker = paragraphs::starts_with_marker(&plan.text);
    // the gap after a list marker is a tab in the source, not a stretched space
    let skip = if marker {
        first_space.map_or(0, |j| j + 1)
    } else {
        0
    };
    let stretched = (skip..runs.len().saturating_sub(1)).any(|i| {
        is_space[i] && plan.targets_px[i + 1] - plan.targets_px[i] > plan.advances_px[i] + 1
    });
    LineSummary {
        x: line.x(),
        right: line.x() + line.width(),
        y: line.y(),
        size: line.max_size(),
        family: dominant_family(runs),
        first_word: first_word_px as f64 * PT_PER_PIXEL,
        space: space_px * PT_PER_PIXEL,
        stretched,
        ends_with_hyphen: plan.text.ends_with(['-', '\u{2010}', '\u{00AD}']),
        starts_with_marker: marker,
        excluded: line.hidden()
            || runs.iter().any(|r| r.hidden)
            || plan.start_pixel.is_none()
            || line.is_rotated(),
    }
}

fn dominant_family(runs: &[CharRun]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for run in runs.iter().filter(|r| r.char != " ") {
        *counts.entry(run.family.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(family, _)| family.to_string())
        .unwrap_or_default()
}

/// the measured source glyphs of each line of a paragraph.
pub(super) fn glyph_keys(page: &PlannedPage, lines: &[usize]) -> Vec<Vec<GlyphKey>> {
    lines
        .iter()
        .map(|&i| {
            page.lines[i]
                .line
                .glyphs
                .iter()
                .filter(|g| measured(g))
                .map(|g| (g.page, g.sequence))
                .collect()
        })
        .collect()
}

pub fn level_name(level: Level) -> String {
    match level {
        Level::Natural => "natural",
        Level::EveryGlyph => "everyGlyph",
        Level::Pinned => "pinned",
    }
    .into()
}

/// the editability numbers of the final arrangement.
pub(super) fn summarize_report(pages: &[PlannedPage], slots: &[Slot], report: &mut HybridReport) {
    let stack_of = stack_places(&report.stacks);
    for page in pages {
        for line in &page.lines {
            report.lines += 1;
            report.glyphs += line.line.glyphs.iter().filter(|g| measured(g)).count();
        }
    }
    report.positioned_lines = report.lines;
    report.glyphs_in_positioned_lines = report.glyphs;
    for slot in slots {
        if let SlotState::Accepted(candidate) = &slot.state {
            count_promoted(pages, slot.page, candidate, &stack_of, report);
        } else if slot.paragraph.lines.len() > 1 {
            report.not_promoted.push(not_promoted(pages, slot));
        }
    }
    let shared: Vec<&StackReport> = report.stacks.iter().filter(|s| s.paragraphs > 1).collect();
    report.shared_frames = shared.len();
    report.paragraphs_in_shared_frames = shared.iter().map(|s| s.paragraphs).sum();
}

/// (page, first line) to (frame number on its page from 1, place in the frame).
fn stack_places(stacks: &[StackReport]) -> BTreeMap<(usize, usize), (usize, usize)> {
    let mut places = BTreeMap::new();
    let mut numbers: BTreeMap<usize, usize> = BTreeMap::new();
    for stack in stacks {
        let number = numbers.entry(stack.page).or_default();
        *number += 1;
        for (place, &first) in stack.members.iter().enumerate() {
            places.insert((stack.page, first), (*number, place));
        }
    }
    places
}

/// adds one promoted paragraph to the counts and the list.
fn count_promoted(
    pages: &[PlannedPage],
    page_index: usize,
    candidate: &super::candidates::Candidate,
    stack_of: &BTreeMap<(usize, usize), (usize, usize)>,
    report: &mut HybridReport,
) {
    let keys = glyph_keys(page_of(pages, page_index), &candidate.lines);
    let glyphs: usize = keys.iter().map(Vec::len).sum();
    let lines = candidate.lines.len();
    report.positioned_lines -= lines;
    report.glyphs_in_positioned_lines -= glyphs;
    if lines > 1 {
        report.multi_line_paragraphs_promoted += 1;
        report.lines_in_multi_line_paragraphs += lines;
        report.glyphs_in_multi_line_paragraphs += glyphs;
    } else {
        report.single_line_paragraphs_promoted += 1;
        report.lines_in_single_line_paragraphs += 1;
        report.glyphs_in_single_line_paragraphs += glyphs;
    }
    let level = level_name(candidate.level);
    *report.promoted_by_level.entry(level.clone()).or_default() += 1;
    report.pins += candidate.pins.len();
    let (stack, place) = stack_of
        .get(&(page_index, candidate.lines[0]))
        .copied()
        .unwrap_or_default();
    report.promoted.push(PromotedParagraph {
        page: page_index,
        lines,
        level,
        justified: candidate.justified,
        pins: candidate.pins.len(),
        stack,
        place_in_stack: place,
        line_pitch_pt: candidate.line_twips as f64 / 20.0,
        text: candidate.runs.iter().map(|r| r.char.as_str()).collect(),
        glyphs: keys,
    });
}

/// a multi-line paragraph that is not promoted whole (its pieces may be,
/// when it was split).
fn not_promoted(pages: &[PlannedPage], slot: &Slot) -> NotPromoted {
    let page = page_of(pages, slot.page);
    let texts: Vec<&str> = slot
        .paragraph
        .lines
        .iter()
        .map(|&i| page.lines[i].plan.text.as_str())
        .collect();
    NotPromoted {
        page: slot.page,
        lines: slot.paragraph.lines.len(),
        text: texts.join(" | "),
        last_verdict: slot.last_verdict.clone(),
    }
}
