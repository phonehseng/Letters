//! what a conversion did, for the person and for tests.

use serde::Serialize;

use crate::calibration::CalibrationReport;
use crate::fonts::FontReport;
use crate::plan::glyphs::{GlyphCounts, LinePlan};
use crate::plan::lines::ScriptCounts;
use crate::verify::VerificationReport;

/// one page of the result.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageReport {
    pub index: usize,
    pub width_pt: f64,
    pub height_pt: f64,
    pub glyphs: usize,
    pub lines: usize,
    pub rotated_lines: usize,
    pub hidden_lines: usize,
    pub pictures: usize,
    pub jpeg_kept: usize,
    pub shapes: usize,
    /// vector items drawn as pictures because word can't draw them exactly
    pub rasterised_items: usize,
    pub raster_pictures: usize,
    /// vector items left out because nothing could draw them
    pub vectors_left_out: usize,
    pub problems: Vec<String>,
}

/// whether word was there, and what it was used for.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordReport {
    pub available: bool,
    pub build: Option<String>,
    /// the result was exported through word to check it
    pub checked: bool,
    /// a plain sentence: "Not checked against Word on this computer." and the like
    pub note: String,
}

/// the checking exports (recipe 7.6).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub passes: usize,
    pub lines_with_misses: Vec<usize>,
    pub glyphs_off_plan: Vec<usize>,
    /// glyphs off plan in the plans kept
    pub glyphs_off_plan_final: usize,
    pub lines_not_found: usize,
    pub seconds: f64,
}

/// the rotated lines' measuring exports (recipe 8).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotatedReport {
    pub lines: usize,
    pub measured: bool,
    /// per measuring export, each line's start along its baseline (None: not found)
    pub along_offsets_pt: Vec<Vec<Option<f64>>>,
    pub corrections_pt: Vec<(f64, f64)>,
}

/// everything a conversion did.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionReport {
    pub source: String,
    pub docx: String,
    pub pages: Vec<PageReport>,
    pub fonts: Vec<FontReport>,
    /// plain sentences for the person, such as "“Minion Pro” doesn’t allow embedding, so a similar font was used."
    pub warnings: Vec<String>,
    pub word: WordReport,
    pub calibration: CalibrationReport,
    pub check: Option<CheckReport>,
    pub rotated: RotatedReport,
    pub verification: Option<VerificationReport>,
    pub glyphs: usize,
    pub lines: usize,
    pub grid_fonts: usize,
    pub scripts: ScriptCounts,
    pub glyph_counts: GlyphCounts,
    pub pdf_bytes: u64,
    pub docx_bytes: u64,
    pub seconds: f64,
    /// what each line asks of word (kept for tests and for re-planning)
    pub line_plans: Vec<LinePlan>,
    /// each horizontal line's frame: x, y, width and line height in twips
    pub frames: Vec<FramePosition>,
}

/// where one line's frame was put.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FramePosition {
    pub page: usize,
    pub x_twips: i64,
    pub y_twips: i64,
    pub width_twips: i64,
    pub line_twips: i64,
}
