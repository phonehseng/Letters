//! "exact, easy to edit": real paragraphs wherever word is proven to place
//! them exactly, round one's positioned lines everywhere else.
//!
//! the steps, each in its own module:
//! - `paragraphs`: group the planned lines of a page into paragraphs
//! - `candidates`: write a paragraph as one word paragraph that word wraps
//!   itself, at three levels of constraint, least intrusive first
//! - `proof`: export the candidates through word and accept a paragraph
//!   only when every glyph landed within the threshold
//! - `stacks` (anchoring): stack the accepted paragraphs of a column in
//!   shared frames, so an edit moves exactly what it should
//! - `rules`: the measured rules of word's paragraph layout behind it all
//! - `completion`: fonts that also cover what a person may type
//! - `cache`: decisions per document, so a second conversion needs one export
//! - `ladder`, `settle`, `summary`: the proof rounds, the final
//!   arrangement and the report, in that order
//! - `ladder`, `settle`, `summary`: the proof rounds, the final
//!   arrangement and the report, in that order
//! - `edits`: the edit tests run through word automation
//!
//! nothing is ever reported as passing that wasn't measured: without word
//! the promotions are predicted by the same pixel arithmetic and the
//! report says "predicted, not checked against Word on this computer".

pub mod cache;
pub mod candidates;
pub mod completion;
pub mod edits;
mod ladder;
mod overlap;
pub mod paragraphs;
pub mod proof;
pub mod rules;
mod settle;
pub mod stacks;
mod summary;
pub mod vertical;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::bridge::WordBridge;
use crate::calibration::{measure_baselines, WordModel};
use crate::pdf::PageContent;
use crate::plan::glyphs::{CharRun, LinePlan};
use crate::plan::lines::Line;
use crate::CancelFlag;
use crate::ExactError;
use ladder::{detect_slots, predict, prove};
use proof::{GlyphKey, Verdict};
use settle::{settle, write_final};
use stacks::StackReport;
pub use summary::summarize;
use summary::summarize_report;

pub const PREDICTED: &str = "Predicted, not checked against Word on this computer.";

/// at most this many proof exports before the final one: the ladder has
/// four attempts and a split paragraph gets two more.
pub const MAX_PROOF_ROUNDS: usize = 6;
/// at most this many exports to settle the stacked arrangement.
pub const MAX_FINAL_ROUNDS: usize = 3;

/// one horizontal line as round one planned it, and its positioned form.
#[derive(Clone, Debug)]
pub struct PlannedLine {
    pub line: Line,
    pub plan: LinePlan,
    pub runs: Vec<CharRun>,
    /// round one's frame for this line (recipe 7.1)
    pub locked_xml: String,
    /// and its position
    pub position: crate::report::FramePosition,
}

/// a page of planned lines, in content order.
#[derive(Clone, Debug)]
pub struct PlannedPage {
    pub index: usize,
    pub width: f64,
    pub height: f64,
    pub lines: Vec<PlannedLine>,
}

/// the body of one page of the docx: its items in document order.
#[derive(Clone, Debug)]
pub struct PageBody {
    pub items: Vec<String>,
}

/// writes the docx from page bodies and returns its path.
pub type WriteDocx<'a> = dyn FnMut(&[PageBody], &Path) -> Result<(), ExactError> + 'a;
/// exports a docx through word, returns the seconds it took.
pub type ExportPdf<'a> = dyn FnMut(&Path, &Path) -> Result<f64, ExactError> + 'a;

/// a promoted paragraph in the final docx, for the report and the edit tests.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotedParagraph {
    pub page: usize,
    pub lines: usize,
    pub level: String,
    pub justified: bool,
    pub pins: usize,
    /// which shared frame of its page holds it, and its place in it
    pub stack: usize,
    pub place_in_stack: usize,
    pub line_pitch_pt: f64,
    pub text: String,
    /// measured source glyphs, per line
    #[serde(skip)]
    pub glyphs: Vec<Vec<GlyphKey>>,
}

/// what the hybrid mode did, and how editable the result is.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HybridReport {
    pub checked_in_word: bool,
    /// a plain sentence: what was measured, or that nothing was
    pub note: String,
    pub glyphs: usize,
    /// shares of the measured glyphs
    pub glyphs_in_multi_line_paragraphs: usize,
    pub glyphs_in_single_line_paragraphs: usize,
    pub glyphs_in_positioned_lines: usize,
    pub lines: usize,
    pub lines_in_multi_line_paragraphs: usize,
    pub lines_in_single_line_paragraphs: usize,
    pub positioned_lines: usize,
    pub paragraphs_detected: usize,
    pub multi_line_paragraphs_detected: usize,
    pub multi_line_paragraphs_promoted: usize,
    pub single_line_paragraphs_promoted: usize,
    pub promoted_by_level: BTreeMap<String, usize>,
    pub pins: usize,
    pub splits: usize,
    /// cuts made before the proofs where no exact line height fits a
    /// paragraph's baselines (`vertical::vertical_pieces`)
    pub vertical_cuts: usize,
    /// shared frames with more than one paragraph, and the paragraphs in them
    pub shared_frames: usize,
    pub paragraphs_in_shared_frames: usize,
    /// text in the body flow: none, frames held better (see `stacks`)
    pub glyphs_in_body_flow: usize,
    pub moved_to_own_frame: usize,
    pub demoted_after_stacking: usize,
    pub demoted_for_mean: usize,
    pub exports: usize,
    /// line heights whose baseline tables were measured for this document
    pub measured_line_heights: Vec<i64>,
    pub word_seconds: f64,
    pub seconds: f64,
    pub cache: String,
    pub rounds: Vec<RoundReport>,
    pub promoted: Vec<PromotedParagraph>,
    /// multi-line paragraphs left in positioned lines, and why
    pub not_promoted: Vec<NotPromoted>,
    pub stacks: Vec<StackReport>,
}

/// a multi-line paragraph that stayed in positioned lines.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotPromoted {
    pub page: usize,
    pub lines: usize,
    pub text: String,
    /// the last proof export's verdict (none: no level could keep its breaks)
    pub last_verdict: Option<Verdict>,
}

impl HybridReport {
    /// share of the text in real multi-line paragraphs, 0 to 1.
    pub fn multi_line_share(&self) -> f64 {
        self.glyphs_in_multi_line_paragraphs as f64 / self.glyphs.max(1) as f64
    }
}

/// one proof export.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundReport {
    pub candidates: usize,
    pub accepted: usize,
    pub by_level: BTreeMap<String, usize>,
}

/// what the hybrid builder needs from the conversion.
pub struct HybridContext<'c, 'f> {
    pub model: &'c mut WordModel,
    pub source: &'c [PageContent],
    /// `None` when word isn't there: promotions are then predicted
    pub export: Option<&'c mut ExportPdf<'f>>,
    pub write: &'c mut WriteDocx<'c>,
    pub work: &'c Path,
    pub docx: &'c Path,
    /// word's pdf of round one's docx, when the checking export left one
    pub locked_pdf: Option<PathBuf>,
    /// where decisions are cached, and the document's key
    pub cache: Option<(PathBuf, String)>,
    /// word, for the baseline tables of the paragraphs' line heights
    pub bridge: Option<&'c WordBridge>,
    pub cancel: &'c CancelFlag,
    /// where measured word facts are kept between conversions
    pub calibration_dir: Option<PathBuf>,
}

/// the hybrid result: the docx is written, and word's pdf of it when word was there.
pub struct HybridOutcome {
    pub report: HybridReport,
    pub final_pdf: Option<PathBuf>,
}

/// builds the hybrid docx: proves paragraphs through word (or predicts them
/// without it), stacks the accepted ones and writes the result.
pub fn build_hybrid(
    pages: &[PlannedPage],
    context: &mut HybridContext,
) -> Result<HybridOutcome, ExactError> {
    let started = Instant::now();
    let mut report = HybridReport {
        cache: "off".into(),
        ..HybridReport::default()
    };
    let mut slots = detect_slots(pages, &mut report);
    let word = context.export.is_some();
    let mut locked_positions = None;
    measure_line_heights(&slots, context, &mut report)?;
    slots = ladder::split_into_vertical_pieces(pages, slots, context.model, &mut report);
    let cached = context
        .cache
        .as_ref()
        .and_then(|(dir, key)| cache::load(dir, key));
    if let Some(decisions) = &cached {
        report.cache = "hit".into();
        cache::apply(&mut slots, decisions, pages, context.model);
    } else if word {
        if context.cache.is_some() {
            report.cache = "miss".into();
        }
        let locked = settle::locked_distances(pages, &slots, context, &mut report)?;
        prove(pages, &mut slots, &locked, context, &mut report)?;
        locked_positions = Some(locked);
    } else {
        predict(pages, &mut slots, context.model);
    }
    let final_pdf = if word {
        let locked = match locked_positions {
            Some(locked) => locked,
            None => settle::locked_distances(pages, &slots, context, &mut report)?,
        };
        settle(pages, &mut slots, &locked, context, &mut report)?
    } else {
        write_final(pages, &slots, context, &mut report)?;
        None
    };
    if word {
        if let Some((dir, key)) = &context.cache {
            let _ = cache::save(dir, key, &slots);
        }
    }
    report.checked_in_word = word;
    report.note = if word {
        "Every promoted paragraph was checked in Word on this computer.".into()
    } else {
        PREDICTED.into()
    };
    summarize_report(pages, &slots, &mut report);
    report.seconds = started.elapsed().as_secs_f64();
    Ok(HybridOutcome { report, final_pdf })
}

/// measures the baseline tables of every line height a multi-line paragraph
/// may get (`vertical::line_height_choices`), all in one word session: off
/// the recipe's 16 measured heights the closed rule misplaces about one
/// line in seven (`calibration::measure_baselines`).
fn measure_line_heights(
    slots: &[ladder::Slot],
    context: &mut HybridContext,
    report: &mut HybridReport,
) -> Result<(), ExactError> {
    let Some(bridge) = context.bridge else {
        return Ok(());
    };
    let heights: Vec<i64> = slots
        .iter()
        .filter(|s| s.paragraph.lines.len() > 1)
        .flat_map(|s| vertical::line_height_choices(s.paragraph.pitch))
        .collect();
    let measured = measure_baselines(
        context.model,
        &heights,
        bridge,
        &context.work.join("baselines"),
        context.cancel,
    )?;
    if !measured.is_empty() {
        report.exports += 1;
        if let Some(dir) = &context.calibration_dir {
            let _ = context.model.save(dir);
        }
    }
    report.measured_line_heights = measured;
    Ok(())
}
