//! the ladder: every paragraph tries its attempts, least intrusive first,
//! all paragraphs at once in each proof export; a paragraph that fails every
//! attempt is split at its failing lines. without word, a prediction.

use super::candidates::{build_candidate, Candidate, Level, PageGeometry};
use super::paragraphs::{detect_paragraphs, DetectedParagraph};
use super::proof::{judge, Distances, Verdict};
use super::settle::export_and_measure;
use super::summary::{glyph_keys, level_name, summarize};
use super::vertical::vertical_pieces;
use super::{HybridContext, HybridReport, PlannedPage, RoundReport, MAX_PROOF_ROUNDS};
use crate::calibration::WordModel;
use crate::ExactError;

/// the ladder of attempts: a level and a nudge of the right edge in pixels.
pub(super) const ATTEMPTS: [(Level, i64); 4] = [
    (Level::Natural, 0),
    (Level::EveryGlyph, 0),
    (Level::Pinned, 0),
    (Level::Pinned, 2),
];
/// a paragraph split at its failing lines tries again from this attempt.
pub(super) const SPLIT_ATTEMPT: usize = 2;

/// one paragraph on its way through the ladder.
#[derive(Clone, Debug)]
pub(super) struct Slot {
    pub(super) page: usize,
    pub(super) paragraph: DetectedParagraph,
    pub(super) attempt: usize,
    pub(super) state: SlotState,
    pub(super) was_split: bool,
    /// the shared frame above may not hold it
    pub(super) own_frame: bool,
    /// what the last proof export said, when it failed
    pub(super) last_verdict: Option<Verdict>,
}

#[derive(Clone, Debug)]
pub(super) enum SlotState {
    /// for its next attempt
    Waiting,
    Trying(Candidate),
    Accepted(Candidate),
    Positioned,
}

impl Slot {
    pub(super) fn candidate(&self) -> Option<&Candidate> {
        match &self.state {
            SlotState::Trying(c) | SlotState::Accepted(c) => Some(c),
            _ => None,
        }
    }
}

/// every paragraph of every page, waiting for its first attempt.
pub(super) fn detect_slots(pages: &[PlannedPage], report: &mut HybridReport) -> Vec<Slot> {
    let mut slots = Vec::new();
    for page in pages {
        for paragraph in detect_paragraphs(&summarize(&page.lines)) {
            report.paragraphs_detected += 1;
            report.multi_line_paragraphs_detected += usize::from(paragraph.lines.len() > 1);
            slots.push(Slot {
                page: page.index,
                paragraph,
                attempt: 0,
                state: SlotState::Waiting,
                was_split: false,
                own_frame: false,
                last_verdict: None,
            });
        }
    }
    slots
}

/// cuts every multi-line paragraph whose lines no single placement prints
/// close enough into pieces that fit (`vertical::vertical_pieces`); each
/// piece is a paragraph of its own from then on, stacked with the others.
pub(super) fn split_into_vertical_pieces(
    pages: &[PlannedPage],
    slots: Vec<Slot>,
    model: &mut WordModel,
    report: &mut HybridReport,
) -> Vec<Slot> {
    let mut pieces = Vec::new();
    for slot in slots {
        if slot.paragraph.lines.len() < 2 {
            pieces.push(slot);
            continue;
        }
        let page = page_of(pages, slot.page);
        let geometry = PageGeometry {
            width_pt: page.width,
            height_pt: page.height,
        };
        let runs = vertical_pieces(
            &slot.paragraph.lines,
            &page.lines,
            &geometry,
            model,
            slot.paragraph.pitch,
        );
        report.vertical_cuts += runs.len() - 1;
        for lines in runs {
            let mut piece = slot.clone();
            piece.paragraph.lines = lines;
            pieces.push(piece);
        }
    }
    pieces
}

pub(super) fn page_of(pages: &[PlannedPage], index: usize) -> &PlannedPage {
    pages
        .iter()
        .find(|p| p.index == index)
        .expect("slots belong to planned pages")
}

/// the next candidate of a slot from its current attempt on, skipping
/// attempts that can't keep the breaks; `None` when the ladder is done.
fn next_candidate(
    slot: &mut Slot,
    pages: &[PlannedPage],
    model: &mut WordModel,
) -> Option<Candidate> {
    let page = page_of(pages, slot.page);
    let geometry = PageGeometry {
        width_pt: page.width,
        height_pt: page.height,
    };
    while slot.attempt < ATTEMPTS.len() {
        let (level, nudge) = ATTEMPTS[slot.attempt];
        // a single line has no breaks to pin
        if !(level == Level::Pinned && slot.paragraph.lines.len() == 1) {
            if let Some(candidate) = build_candidate(
                slot.page,
                &page.lines,
                &slot.paragraph,
                level,
                &geometry,
                model,
                nudge,
            ) {
                return Some(candidate);
            }
        }
        slot.attempt += 1;
    }
    None
}

/// the proof rounds: every waiting paragraph is exported at its next
/// attempt, all at once, each in its own frame so none can move another.
pub(super) fn prove(
    pages: &[PlannedPage],
    slots: &mut Vec<Slot>,
    locked: &Distances,
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<(), ExactError> {
    for round in 0..MAX_PROOF_ROUNDS {
        let mut trying = 0;
        for slot in slots.iter_mut() {
            if matches!(slot.state, SlotState::Waiting) {
                slot.state = match next_candidate(slot, pages, context.model) {
                    Some(candidate) => {
                        trying += 1;
                        SlotState::Trying(candidate)
                    }
                    None => SlotState::Positioned,
                };
            }
        }
        if trying == 0 {
            break;
        }
        let path = context.work.join(format!("hybrid_proof_{round}.docx"));
        let distances = export_and_measure(pages, slots, false, &path, context, report)?.0;
        let mut round_report = RoundReport {
            candidates: trying,
            ..RoundReport::default()
        };
        let mut splits = Vec::new();
        for (index, slot) in slots.iter_mut().enumerate() {
            let SlotState::Trying(candidate) = &slot.state else {
                continue;
            };
            let verdict = judge(
                &glyph_keys(page_of(pages, slot.page), &candidate.lines),
                &distances,
                Some(locked),
            );
            *round_report
                .by_level
                .entry(level_name(candidate.level))
                .or_default() += 1;
            if verdict.accepted() {
                round_report.accepted += 1;
                slot.state = SlotState::Accepted(candidate.clone());
                continue;
            }
            let level = candidate.level;
            slot.attempt += 1;
            slot.state = SlotState::Waiting;
            slot.last_verdict = Some(verdict.clone());
            if level == Level::Pinned && slot.attempt >= ATTEMPTS.len() && !slot.was_split {
                splits.push((index, verdict));
            }
        }
        report.rounds.push(round_report);
        split_failing(slots, splits, report);
    }
    for slot in slots.iter_mut() {
        if !matches!(slot.state, SlotState::Accepted(_)) {
            slot.state = SlotState::Positioned;
        }
    }
    Ok(())
}

/// a paragraph that failed every attempt is split at its failing lines:
/// runs of lines that passed become paragraphs of their own and try the
/// pinned attempts again; the failing lines stay positioned.
fn split_failing(slots: &mut Vec<Slot>, splits: Vec<(usize, Verdict)>, report: &mut HybridReport) {
    for (index, verdict) in splits.into_iter().rev() {
        let slot = slots[index].clone();
        let lines = &slot.paragraph.lines;
        if verdict.failing_lines.len() >= lines.len() {
            continue;
        }
        let mut pieces: Vec<Slot> = Vec::new();
        let mut run: Vec<usize> = Vec::new();
        for (position, &line) in lines.iter().enumerate() {
            if verdict.failing_lines.contains(&position) {
                if !run.is_empty() {
                    pieces.push(piece(&slot, std::mem::take(&mut run)));
                }
                continue;
            }
            run.push(line);
        }
        if !run.is_empty() {
            pieces.push(piece(&slot, run));
        }
        report.splits += 1;
        // the paragraph itself stays positioned; only its pieces carry candidates
        slots[index].state = SlotState::Positioned;
        for (offset, piece) in pieces.into_iter().enumerate() {
            slots.insert(index + 1 + offset, piece);
        }
    }
    // keep the slots in page and line order, so items are written in order
    slots.sort_by_key(|s| (s.page, s.paragraph.lines.first().copied().unwrap_or(0)));
}

fn piece(slot: &Slot, lines: Vec<usize>) -> Slot {
    Slot {
        page: slot.page,
        paragraph: DetectedParagraph {
            lines,
            alignment: slot.paragraph.alignment,
            pitch: slot.paragraph.pitch,
        },
        attempt: SPLIT_ATTEMPT,
        state: SlotState::Waiting,
        was_split: true,
        own_frame: false,
        last_verdict: None,
    }
}

/// without word: a paragraph is promoted only when the pixel arithmetic
/// leaves every break at least two pixels of room either way, left aligned
/// with every glyph's spacing (the measured rules alone place it).
pub(super) fn predict(pages: &[PlannedPage], slots: &mut [Slot], model: &mut WordModel) {
    for slot in slots.iter_mut() {
        let page = page_of(pages, slot.page);
        let geometry = PageGeometry {
            width_pt: page.width,
            height_pt: page.height,
        };
        let candidate = build_candidate(
            slot.page,
            &page.lines,
            &slot.paragraph,
            Level::Pinned,
            &geometry,
            model,
            0,
        );
        slot.state = match candidate {
            Some(c) if c.pins.is_empty() && c.window_px.1 - c.right_px >= 2 => {
                SlotState::Accepted(c)
            }
            _ => SlotState::Positioned,
        };
    }
}
