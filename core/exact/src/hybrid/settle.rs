//! writing the document and settling the final arrangement: the accepted
//! paragraphs are stacked in shared frames, exported, and any that moved get
//! a frame of their own or go back to positioned lines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::ladder::{page_of, Slot, SlotState};
use super::proof::{demote_for_mean, judge, Distances, GlyphKey};
use super::stacks::{arrange, arrange_apart, Item, StackReport};
use super::summary::glyph_keys;
use super::{HybridContext, HybridReport, PageBody, PlannedPage, MAX_FINAL_ROUNDS};
use crate::pdf::PdfFile;
use crate::verify::THRESHOLD_MEAN_PT;
use crate::ExactError;

/// the items of one page: accepted (or trying) paragraphs at their first
/// line, positioned lines everywhere else, in line order.
fn page_items<'a>(page: &'a PlannedPage, slots: &'a [Slot]) -> Vec<Item<'a>> {
    let mut by_first_line: BTreeMap<usize, &Slot> = BTreeMap::new();
    let mut covered = vec![false; page.lines.len()];
    for slot in slots.iter().filter(|s| s.page == page.index) {
        if slot.candidate().is_some() {
            for &line in &slot.paragraph.lines {
                covered[line] = true;
            }
            by_first_line.insert(slot.paragraph.lines[0], slot);
        }
    }
    let mut items = Vec::new();
    for (index, line) in page.lines.iter().enumerate() {
        if let Some(slot) = by_first_line.get(&index) {
            items.push(Item::Paragraph {
                candidate: slot
                    .candidate()
                    .expect("only slots with candidates are listed"),
                own_frame: slot.own_frame,
            });
        } else if !covered[index] {
            items.push(Item::Locked(&line.locked_xml));
        }
    }
    items
}

/// writes the docx with every page's items, stacked or each apart.
fn write_document(
    pages: &[PlannedPage],
    slots: &[Slot],
    stacked: bool,
    path: &Path,
    context: &mut HybridContext,
) -> Result<Vec<StackReport>, ExactError> {
    let mut bodies = Vec::new();
    let mut stacks = Vec::new();
    for page in pages {
        let items = page_items(page, slots);
        let xml = if stacked {
            let (xml, reports) = arrange(page.index, &items);
            stacks.extend(reports);
            xml
        } else {
            arrange_apart(&items)
        };
        bodies.push(PageBody { items: xml });
    }
    (context.write)(&bodies, path)?;
    Ok(stacks)
}

/// writes, exports through word and measures every glyph.
pub(super) fn export_and_measure(
    pages: &[PlannedPage],
    slots: &[Slot],
    stacked: bool,
    path: &Path,
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<(Distances, PathBuf, Vec<StackReport>), ExactError> {
    let stacks = write_document(pages, slots, stacked, path, context)?;
    let pdf = path.with_extension("word.pdf");
    let export = context
        .export
        .as_mut()
        .expect("proof exports only run with word");
    report.word_seconds += export(path, &pdf)?;
    report.exports += 1;
    let word_pages = PdfFile::open(&pdf)?.read_pages();
    Ok((Distances::measure(context.source, &word_pages), pdf, stacks))
}

/// the final arrangement: stacks are written and exported; a paragraph
/// that moved in its stack gets a frame of its own, one that still fails
/// is left positioned, and paragraphs that push the document mean over
/// the target are demoted, worst first. returns word's pdf of the result.
pub(super) fn settle(
    pages: &[PlannedPage],
    slots: &mut [Slot],
    locked: &Distances,
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<Option<PathBuf>, ExactError> {
    let docx = context.docx.to_path_buf();
    for round in 0..MAX_FINAL_ROUNDS {
        let (distances, pdf, stacks) =
            export_and_measure(pages, slots, true, &docx, context, report)?;
        report.stacks = stacks;
        let mut changed = false;
        for slot in slots.iter_mut() {
            let SlotState::Accepted(candidate) = &slot.state else {
                continue;
            };
            let verdict = judge(
                &glyph_keys(page_of(pages, slot.page), &candidate.lines),
                &distances,
                Some(locked),
            );
            if verdict.accepted() {
                continue;
            }
            changed = true;
            if !slot.own_frame {
                slot.own_frame = true;
                report.moved_to_own_frame += 1;
            } else {
                slot.state = SlotState::Positioned;
                report.demoted_after_stacking += 1;
            }
        }
        if !changed && distances.mean() >= THRESHOLD_MEAN_PT {
            changed = demote_mean(pages, slots, &distances, locked, report);
        }
        if !changed {
            return Ok(Some(pdf));
        }
        if round + 1 == MAX_FINAL_ROUNDS {
            break;
        }
    }
    // the last change was never exported: export what is written now
    let (_, pdf, stacks) = export_and_measure(pages, slots, true, &docx, context, report)?;
    report.stacks = stacks;
    Ok(Some(pdf))
}

/// demotes promoted paragraphs, worst first, until the estimated mean
/// passes; the fallback positions are those of round one's docx.
fn demote_mean(
    pages: &[PlannedPage],
    slots: &mut [Slot],
    distances: &Distances,
    fallback: &Distances,
    report: &mut HybridReport,
) -> bool {
    let promoted: Vec<usize> = (0..slots.len())
        .filter(|&i| matches!(slots[i].state, SlotState::Accepted(_)))
        .collect();
    let keys: Vec<Vec<GlyphKey>> = promoted
        .iter()
        .map(|&i| {
            let slot = &slots[i];
            glyph_keys(page_of(pages, slot.page), &slot.paragraph.lines)
                .into_iter()
                .flatten()
                .collect()
        })
        .collect();
    let demote = demote_for_mean(&keys, distances, fallback, THRESHOLD_MEAN_PT);
    for &index in &demote {
        slots[promoted[index]].state = SlotState::Positioned;
    }
    report.demoted_for_mean += demote.len();
    !demote.is_empty()
}

/// where round one's positioned lines put every glyph: word's pdf of round
/// one's docx when the checking export left one, else one more export.
pub(super) fn locked_distances(
    pages: &[PlannedPage],
    slots: &[Slot],
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<Distances, ExactError> {
    if let Some(pdf) = &context.locked_pdf {
        return Ok(Distances::measure(
            context.source,
            &PdfFile::open(pdf)?.read_pages(),
        ));
    }
    let path = context.work.join("hybrid_locked.docx");
    let mut positioned: Vec<Slot> = slots.to_vec();
    for slot in &mut positioned {
        slot.state = SlotState::Positioned;
    }
    Ok(export_and_measure(pages, &positioned, false, &path, context, report)?.0)
}

/// without word: the final docx, stacked, never exported.
pub(super) fn write_final(
    pages: &[PlannedPage],
    slots: &[Slot],
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<(), ExactError> {
    let docx = context.docx.to_path_buf();
    report.stacks = write_document(pages, slots, true, &docx, context)?;
    Ok(())
}
