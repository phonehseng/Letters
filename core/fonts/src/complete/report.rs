//! what the completion did, for the person and for the ui: which font
//! the document's font turned out to be, how each new letter was made,
//! how alike the result is, licence notes, and plain sentences.

use serde::{Deserialize, Serialize};

use crate::matching::fit::{FittedDonor, Transform};
use crate::matching::identify::Candidate;
use crate::model::{EmbeddingPermission, Source};

/// how one glyph was made, most faithful first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Strategy {
    /// the document's font already had it
    Original,
    /// copied unchanged from the same design found on this computer
    ExactDonor,
    /// from the closest design, changed to match
    FittedDonor,
    /// built from glyphs the font has (accents, ligatures, superscripts)
    Composed,
    /// drawn by the optional AI tools provider, checked against the style
    Generated,
    /// from the closest font as it is, nothing closer being found
    LookAlike,
    /// nothing could draw it
    Missing,
}

impl Strategy {
    pub fn label(self) -> &'static str {
        match self {
            Strategy::Original => "original",
            Strategy::ExactDonor => "same font",
            Strategy::FittedDonor => "adjusted near font",
            Strategy::Composed => "built from the font’s own glyphs",
            Strategy::Generated => "generated",
            Strategy::LookAlike => "look-alike",
            Strategy::Missing => "missing",
        }
    }
}

/// the font a glyph came from, with its licence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DonorNote {
    pub family: String,
    pub style: String,
    pub source: Source,
    pub permission: EmbeddingPermission,
    /// the licence sentence, as the font library words it
    pub permission_sentence: String,
}

/// how one needed character was drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphProvenance {
    pub character: char,
    /// its glyph number in the completed font (None when missing)
    pub glyph_id: Option<u16>,
    pub strategy: Strategy,
    pub donor: Option<DonorNote>,
    /// for composed glyphs: the characters used, base first
    pub built_from: Vec<char>,
    /// how alike it is expected to be, percent (see `similarity_percent`)
    pub similarity: Option<f64>,
    pub sentence: String,
}

/// the font identified as the document's typeface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identified {
    pub family: String,
    pub style: String,
    pub source: Source,
    /// true when it is the same design, not just the closest one
    pub same_design: bool,
    pub similarity: f64,
}

/// the fitted near design, and how well it predicts the document's font.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitSummary {
    pub family: String,
    pub style: String,
    pub source: Source,
    /// percent alike on glyphs held out of the fit
    pub similarity: f64,
    /// percent alike before it was changed
    pub unchanged_similarity: f64,
    /// held-out mean and largest outline distance, em
    pub mean_distance: f64,
    pub max_distance: f64,
    /// held-out mean advance difference, em
    pub advance_error: f64,
    pub fitted_glyphs: usize,
    pub held_out_glyphs: usize,
    pub transform: Transform,
}

impl FitSummary {
    pub fn of(fitted: &FittedDonor) -> FitSummary {
        FitSummary {
            family: fitted.family.clone(),
            style: fitted.style.clone(),
            source: fitted.source,
            similarity: fitted.quality.similarity,
            unchanged_similarity: crate::matching::identify::similarity_percent(
                fitted.quality.unchanged_mean,
            ),
            mean_distance: fitted.quality.mean,
            max_distance: fitted.quality.max,
            advance_error: fitted.quality.advance_error,
            fitted_glyphs: fitted.quality.fitted,
            held_out_glyphs: fitted.quality.held_out,
            transform: fitted.transform.clone(),
        }
    }
}

/// everything the ui shows about a completion.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReport {
    pub identified: Option<Identified>,
    /// each needed character and how it was made
    pub strategies: Vec<(char, Strategy)>,
    pub fit: Option<FitSummary>,
    /// the fonts compared, best first (at most a few)
    pub candidates: Vec<Candidate>,
    /// fonts not used because of their licence, and the licences of
    /// every font that did lend glyphs
    pub permission_notes: Vec<String>,
    /// plain sentences for the person, most important first
    pub sentences: Vec<String>,
}

/// where a font is, in words: "on this computer", "that comes with
/// Letters"...
pub fn source_words(source: Source) -> &'static str {
    match source {
        Source::System => "on this computer",
        Source::Custom => "that you added",
        Source::Bundled => "that comes with Letters",
        Source::Library => "downloaded from the Letters library",
        Source::Downloadable => "from the Letters library",
    }
}

/// "Arial" or "Arial Bold".
pub fn family_and_style(family: &str, style: &str) -> String {
    if style.is_empty() || style.eq_ignore_ascii_case("regular") {
        family.to_string()
    } else {
        format!("{family} {style}")
    }
}

/// "99.2%": one decimal, without a trailing ".0" at 100.
pub fn percent(value: f64) -> String {
    if value >= 99.95 {
        "100%".to_string()
    } else {
        format!("{value:.1}%")
    }
}

/// a list of characters for a sentence: "“j”, “k” and “q”".
pub fn char_list(chars: &[char]) -> String {
    let quoted: Vec<String> = chars.iter().map(|c| format!("“{c}”")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_for_people() {
        assert_eq!(percent(99.24), "99.2%");
        assert_eq!(percent(99.99), "100%");
        assert_eq!(char_list(&['j', 'k', 'q']), "“j”, “k” and “q”");
        assert_eq!(char_list(&['é']), "“é”");
        assert_eq!(family_and_style("Arial", "Regular"), "Arial");
        assert_eq!(family_and_style("Arial", "Bold"), "Arial Bold");
        assert_eq!(source_words(Source::Bundled), "that comes with Letters");
    }
}
