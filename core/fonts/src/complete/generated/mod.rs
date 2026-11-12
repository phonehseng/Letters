//! glyphs drawn by the optional AI tools provider, used only when they
//! beat the fitted near design.
//!
//! **what is sent.** example glyphs of the document's font, as path data
//! on a grid of 1000 units per em, its measured metrics, and the set of
//! characters wanted (each once, sorted, so the words being typed can't
//! be read from it). never any text of the document.
//!
//! **told before it leaves.** this follows the same two steps as the
//! AI tools crate: `plan_generation` works out the request and returns a
//! `GlyphDisclosure` with a plain sentence; only an acknowledgement of
//! that very disclosure lets `run_generation` make the permit a
//! generator needs to send anything. requests and permits can only be
//! made inside this crate.
//!
//! **trusted for nothing.** a reply is parsed strictly (absolute M, L,
//! Q, C, Z only, a size limit), every contour must be closed and of
//! some size, inside sane bounds, without crossing itself or another
//! contour, with a believable advance. a glyph that passes is scored
//! against the measured style (heights, stem weight, advance) and is
//! kept only when it scores better than what the fitted donor made.
//!
//! the request format is plain json and names no provider; the
//! generator trait is all a provider must implement. tests use a fake
//! generator with canned replies; nothing here contacts any service.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::report::char_list;
use crate::matching::glyphs::{GlyphData, GlyphSet};
use crate::matching::shape::{Shape, ShapeBuilder};
use crate::matching::style::{expected_extent, horizontal_spans, StyleFit};

mod fake;
mod reply;
mod request;
#[cfg(test)]
mod tests;

pub use fake::*;
pub use reply::*;
pub use request::*;

/// the grid paths are given on: units per em.
pub const GRID: f64 = 1000.0;

/// the most example glyphs sent: enough to show the style.
pub const MAX_EXAMPLES: usize = 24;

/// the most characters asked for in one request.
pub const MAX_WANTED: usize = 64;

/// a reply larger than this is refused unread.
pub const MAX_REPLY_BYTES: usize = 512 * 1024;

/// the most points one generated glyph may have.
pub const MAX_POINTS: usize = 1500;

/// what a generator says about itself, for the disclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorInfo {
    /// a short name for the ui: "Model on this computer", "Online service"
    pub name: String,
    /// the host requests go to
    pub host: String,
    /// true when what is sent can leave this computer
    pub leaves_device: bool,
}

/// why generation didn't happen.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GenerationError {
    #[error("Letters needs your OK before sending the font’s outlines.")]
    ConsentRequired,
    #[error("That OK was for a different request. Please confirm again.")]
    ConsentMismatch,
    #[error("The AI tools settings changed after you confirmed. Please confirm again.")]
    ProviderChanged,
    #[error("The AI tools provider couldn’t draw the letters: {0}")]
    Provider(String),
}

/// something that can draw glyphs: the optional provider, or a fake one
/// in tests. implementations must call `permit.check(&self.describe())`
/// before sending anything, and return the reply text unchanged.
pub trait GlyphGenerator: Send + Sync {
    fn describe(&self) -> GeneratorInfo;
    fn generate(
        &self,
        request: &GlyphRequest,
        permit: &GlyphPermit,
    ) -> Result<String, GenerationError>;
}

/// a generation worked out but not run.
#[derive(Clone, Debug)]
pub struct GenerationPlan {
    disclosure: GlyphDisclosure,
    request: GlyphRequest,
}

impl GenerationPlan {
    pub fn disclosure(&self) -> &GlyphDisclosure {
        &self.disclosure
    }

    pub fn request(&self) -> &GlyphRequest {
        &self.request
    }
}

/// a planned generation with the generator and (when needed) the yes.
pub struct Generation<'a> {
    pub generator: &'a dyn GlyphGenerator,
    pub plan: GenerationPlan,
    pub acknowledgement: Option<GlyphAcknowledgement>,
}

/// how far a glyph is from what the style predicts, em: its bottom and
/// top against the expected heights, its stem against the measured
/// stem, and its advance against a known advance (the pdf's width) when
/// there is one. lower is better; the same score is used for the fitted
/// donor's glyph, so the two can be compared fairly. (an advance taken
/// from the fitted glyph itself would favour that glyph, so it is never
/// passed here.)
pub fn style_score(
    c: char,
    glyph: &GlyphData,
    style: &StyleFit,
    known_advance: Option<f64>,
) -> f64 {
    let Some(bounds) = glyph.shape.bounds() else {
        return 1.0;
    };
    let mut score = 0.0;
    if let Some((bottom, top)) = expected_extent(style, c) {
        score += (bounds.y_min - bottom).abs() + (bounds.y_max - top).abs();
    }
    let (stem, height) = if c.is_uppercase() {
        (style.cap_stem, style.cap_height)
    } else {
        (style.stem, style.x_height)
    };
    if let Some(span) = horizontal_spans(&glyph.shape, height * 0.45).first() {
        let width = span.1 - span.0;
        // bowls and diagonals are wider than stems; only stem-like spans count
        if width < stem * 2.0 {
            score += (width - stem).abs();
        }
    }
    let advance_error = known_advance.map_or(0.0, |advance| (glyph.advance - advance).abs());
    score + advance_error * 0.5
}
