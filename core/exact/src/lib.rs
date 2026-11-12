//! converts a pdf to a docx that word renders the same, glyph for glyph.
//!
//! the algorithm is the measured recipe of the fidelity experiments
//! (`RECIPE.md` on the fidelity branch, round 4): every glyph is planned on
//! the positions microsoft word's pdf writer can print, so word's own pdf
//! export of the docx puts each glyph within 0.125 pt of the source. the
//! sections of that recipe are cited throughout as "recipe 6.3" and so on.
//!
//! modules, in pipeline order:
//! - `pdf`: reads glyphs, paths and pictures from the content streams
//! - `fonts`: rebuilds every font as clean truetype, then one grid font per
//!   face and size, and embeds them the way word expects
//! - `word`: the measured rules of word's layout and pdf writer
//! - `plan`: lines, frames and per-glyph spacing on word's print lattice
//! - `docx`: writes the package
//! - `bridge`: drives a private, invisible copy of word
//! - `calibration`: the measured facts per word build, cached
//! - `verify`: exports the docx through word and measures every glyph
//! - `convert`: the pipeline, and the report it returns

pub mod bridge;
pub mod calibration;
mod convert;
pub mod docx;
mod error;
pub mod fonts;
pub mod pdf;
pub mod plan;
pub mod report;
pub mod units;
pub mod verify;
pub mod word;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use convert::convert;
pub use error::ExactError;
pub use plan::raster::{RenderFn, RenderedPage};
pub use report::{ConversionReport, PageReport, WordReport};

/// a flag another thread sets to stop a conversion at the next step.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// a step of the conversion, for a progress bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Reading,
    Fonts,
    Measuring,
    Planning,
    Writing,
    Checking,
    Verifying,
    Done,
}

/// where a conversion is.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Progress {
    pub stage: Stage,
    /// 0 to 1 for the whole conversion
    pub fraction: f32,
}

pub type ProgressFn = dyn Fn(Progress) + Send + Sync;

/// how to convert.
#[derive(Clone)]
pub struct ConvertOptions {
    /// where measured facts about this computer's word are kept between
    /// conversions; without one, only the built-in tables and this
    /// conversion's own measurements are used
    pub cache_dir: Option<PathBuf>,
    /// export the result through word and measure every glyph
    pub verify: bool,
    /// false: never start word, as if it weren't installed
    pub use_word: bool,
    /// the checking export that re-plans the few lines word moved (recipe 7.6)
    pub check_passes: bool,
    /// the longest one document may take in word, in seconds
    pub word_seconds_per_document: u64,
    /// draws a pdf page, for vector-only content that must become a picture
    /// and for the pixel comparison of the verification
    pub render: Option<Arc<RenderFn>>,
    pub progress: Option<Arc<ProgressFn>>,
    pub cancel: CancelFlag,
    /// a folder for intermediate files; a fresh temporary one when `None`
    pub work_dir: Option<PathBuf>,
    /// keep the intermediate files (probe and checking exports)
    pub keep_work: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            cache_dir: None,
            verify: false,
            use_word: true,
            check_passes: true,
            word_seconds_per_document: 120,
            render: None,
            progress: None,
            cancel: CancelFlag::new(),
            work_dir: None,
            keep_work: false,
        }
    }
}
