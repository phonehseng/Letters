//! what an edit changed, in numbers the app can show ("3 lines moved,
//! 1 font extended") and the results the newer operations return.

use serde::{Deserialize, Serialize};

use crate::model::PageObjects;
use crate::text_fonts::FontNotice;

/// a count of everything an edit changed besides the thing edited.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChangesSummary {
    /// every page whose content changed, 0-based, in order
    pub pages_changed: Vec<usize>,
    pub text_lines_moved: usize,
    pub images_moved: usize,
    pub paths_moved: usize,
    pub annotations_moved: usize,
    /// paths whose own coordinates were rewritten (table rules stretched
    /// or moved, points edited)
    pub paths_reshaped: usize,
    pub fonts_extended: Vec<FontExtension>,
    pub tightening: Option<Tightening>,
    /// anything else worth telling the person, as sentences
    pub notes: Vec<String>,
}

impl ChangesSummary {
    pub(crate) fn page(&mut self, page: usize) {
        if !self.pages_changed.contains(&page) {
            self.pages_changed.push(page);
            self.pages_changed.sort_unstable();
        }
    }

    pub(crate) fn note(&mut self, sentence: impl Into<String>) {
        self.notes.push(sentence.into());
    }
}

/// glyphs added to a font already in the file, so new text is drawn in
/// the same font object as the text around it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontExtension {
    /// the font's name without its subset tag
    pub font: String,
    /// the characters added
    pub characters: String,
    /// where the new glyphs' outlines came from, e.g. "Arial on this computer"
    pub glyph_source: String,
    /// the least faithful way any new glyph was made, as letters-fonts
    /// labels it: "same font", "adjusted near font", "look-alike"...
    #[serde(default)]
    pub match_method: String,
    /// how alike the least alike new glyph is to the font, percent
    #[serde(default)]
    pub similarity: Option<f32>,
    /// one sentence for the person: which font drew the new letters and
    /// how alike they are
    #[serde(default)]
    pub sentence: String,
}

/// how much a paragraph was set tighter to fit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tightening {
    /// change of letter spacing in ems (negative is tighter), e.g. -0.01
    pub letter_spacing_em: f32,
    /// word spaces as a share of their normal width, e.g. 0.9
    pub word_spacing_scale: f32,
    /// line spacing as a share of the original, e.g. 0.98
    pub leading_scale: f32,
}

/// the result of the newer content edits: the edited page, any font
/// notices, and what else changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditResult {
    pub objects: PageObjects,
    pub notices: Vec<FontNotice>,
    pub summary: ChangesSummary,
    /// ids of the objects the edit was about, as they are now (for a
    /// delete, empty)
    pub object_ids: Vec<String>,
}

/// consent the person gave for edits with side effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditConsent {
    /// the person accepted that editing makes digital signatures invalid
    pub invalidate_signatures: bool,
}
