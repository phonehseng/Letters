//! the settings of a comparison, and the progress it reports.

use serde::{Deserialize, Serialize};

/// what to compare and how strictly. the defaults suit most documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompareOptions {
    /// differences only in spacing or in where a word was hyphenated are
    /// left out of the report (they are still counted in the summary)
    pub ignore_spacing: bool,
    /// "Apple" and "apple" count as the same word
    pub ignore_case: bool,
    pub compare_formatting: bool,
    pub compare_layout: bool,
    /// draw both pages and compare pictures and drawings. needs both
    /// documents to be able to render
    pub compare_pictures: bool,
    /// how far text may move, in points, before it counts as moved
    pub position_tolerance: f32,
    /// pixels per point for the picture comparison. 1 is plenty to find
    /// a changed picture, and keeps a page under 2 MB
    pub picture_scale: f32,
    /// the fewest words a passage needs to be reported as moved rather
    /// than as removed in one place and added in another
    pub min_move_words: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            ignore_spacing: true,
            ignore_case: false,
            compare_formatting: true,
            compare_layout: true,
            compare_pictures: true,
            position_tolerance: 2.0,
            picture_scale: 1.0,
            min_move_words: 4,
        }
    }
}

/// which part of the work a progress report is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// reading the old document's text, to match pages
    ReadingOld,
    /// reading the new document's text, to match pages
    ReadingNew,
    /// comparing matched pages one pair at a time
    ComparingPages,
}

/// how far the comparison has got. `done` of `total` steps of `stage`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub done: u32,
    pub total: u32,
}
