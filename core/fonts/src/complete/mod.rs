//! font completion: giving a document's subset font the letters it is
//! missing, so text added to a pdf looks as if it was always there.
//!
//! pdfs embed subset fonts (only the letters already on the page), often
//! of typefaces the person doesn't have. when new words need letters the
//! subset lacks, each one is made by the most faithful way that works:
//!
//! 1. **the same font** found on this computer (outlines compared glyph
//!    by glyph): its glyphs are copied unchanged, with its kerning;
//! 2. **the closest font, adjusted**: zones, width, weight, slant and
//!    spacing fitted to the glyphs both have, then applied;
//! 3. **built from the font's own glyphs**: accented letters from a base
//!    and a mark, ligatures, superscripts (preferred over a donor for
//!    accents, because the base is the real thing);
//! 4. **generated** by the optional AI tools provider, only with the
//!    person's yes, and only kept when it beats strategy 2;
//! 5. **the closest font as it is**, said plainly.
//!
//! every glyph says which way it was made, and the report says how alike
//! the result is. the completed font keeps every original glyph byte for
//! byte and number for number, with the new ones appended.
//!
//! | module | job |
//! |---|---|
//! | `subset_font` | the document's font: glyphs, numbers, pdf widths, name |
//! | `pipeline` | trying the strategies in order |
//! | `synthesis` | accented letters, ligatures, superscripts |
//! | `marks` | which letters are a base plus a mark |
//! | `generated` | the optional generated glyphs: request, disclosure, checks |
//! | `truetype` | writing the completed font |
//! | `report` | provenance, the match report and its sentences |
//! | `cache` | remembering fits and completed fonts |

pub mod cache;
pub mod generated;
pub mod marks;
mod pipeline;
pub mod report;
pub mod subset_font;
pub mod synthesis;
pub mod truetype;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::FontError;
use crate::library::FontLibrary;
use crate::matching::donors::{DonorFile, DonorPool};
use crate::matching::glyphs::GlyphData;
use crate::matching::identify::{identify_glyphs, Candidate, IdentifyOptions};
use crate::matching::style::StyleFit;
use crate::model::Source;

pub use generated::{
    plan_generation, FakeGenerator, Generation, GenerationError, GenerationPlan, GeneratorInfo,
    GlyphAcknowledgement, GlyphDisclosure, GlyphGenerator, GlyphPermit, GlyphRequest,
};
pub use report::{DonorNote, FitSummary, GlyphProvenance, Identified, MatchReport, Strategy};
pub use subset_font::SubsetFont;

/// choices for a completion.
#[derive(Clone, Debug)]
pub struct CompleteOptions {
    /// may fonts from the online library be fetched to compare? (only
    /// `font_for_new_text` fetches; the person must have agreed to the
    /// library's download notice)
    pub allow_download: bool,
    /// how many fonts are compared glyph by glyph
    pub candidate_limit: usize,
    /// how many near designs are fitted
    pub fit_limit: usize,
    /// build accented letters from the font's own glyphs before asking a
    /// near design for them
    pub prefer_composed_accents: bool,
    /// use a font as it is when nothing can be fitted (strategy 5)
    pub allow_look_alike: bool,
    /// where fitted donors and completed fonts are remembered
    pub cache_folder: Option<PathBuf>,
}

impl Default for CompleteOptions {
    fn default() -> Self {
        CompleteOptions {
            allow_download: false,
            candidate_limit: 10,
            fit_limit: 3,
            prefer_composed_accents: true,
            allow_look_alike: true,
            cache_folder: None,
        }
    }
}

/// a kerning pair for new text: the space between `left` and `right`
/// changes by `value` em (negative brings them closer).
pub type KernPair = (char, char, f64);

/// the completed font and everything about how it was made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedFont {
    /// the font to embed: a TrueType file with the document font's glyphs
    /// unchanged and numbered as before, and the new glyphs after them
    #[serde(skip)]
    pub font: Vec<u8>,
    /// the neutral family name written into the font
    pub family_name: String,
    /// every character the font can draw and its glyph number
    pub char_to_glyph: BTreeMap<char, u16>,
    /// one entry per needed character, in the order first asked for
    pub glyphs: Vec<GlyphProvenance>,
    /// the new glyphs as outlines (em units), for callers that draw or
    /// measure them directly
    pub new_glyphs: BTreeMap<char, GlyphData>,
    /// kerning for pairs with a new glyph (only from the same font)
    pub kerning: Vec<KernPair>,
    /// the measured style of the document's font, to keep spacing
    /// consistent around the new text
    pub style: StyleFit,
    pub report: MatchReport,
    /// plain sentences for the person (the report's, plus problems)
    pub notices: Vec<String>,
    /// true when this came from the cache
    pub from_cache: bool,
}

impl CompletedFont {
    /// the strategy that drew `c`, if it was asked for.
    pub fn strategy_of(&self, c: char) -> Option<Strategy> {
        self.glyphs
            .iter()
            .find(|glyph| glyph.character == c)
            .map(|glyph| glyph.strategy)
    }
}

/// which fonts the document's font could be, best first; the first is
/// the same design when its `same_design` is set.
pub fn identify(
    subset: &SubsetFont,
    pool: &DonorPool,
    options: &CompleteOptions,
) -> Vec<Candidate> {
    identify_glyphs(
        &subset.glyphs,
        &subset.profile(),
        pool,
        &IdentifyOptions {
            candidate_limit: options.candidate_limit,
            solve_axes: true,
        },
    )
}

/// gives the document's font the characters of `needed` it lacks.
pub fn complete_font(
    subset: &SubsetFont,
    needed: &str,
    pool: &DonorPool,
    options: &CompleteOptions,
) -> Result<CompletedFont, FontError> {
    pipeline::complete(subset, needed, pool, options, None)
}

/// the same, allowing generated glyphs through a planned and (when it
/// leaves the computer) acknowledged generation.
pub fn complete_font_with(
    subset: &SubsetFont,
    needed: &str,
    pool: &DonorPool,
    options: &CompleteOptions,
    generation: Option<&Generation>,
) -> Result<CompletedFont, FontError> {
    pipeline::complete(subset, needed, pool, options, generation)
}

/// the font to embed for new text in the document's font: every font
/// the library has on this computer may lend glyphs, and with
/// `allow_download` the online library's closest families are fetched
/// for comparison first (their preview file only).
pub fn font_for_new_text(
    subset: &SubsetFont,
    text: &str,
    library: &FontLibrary,
    options: &CompleteOptions,
) -> Result<CompletedFont, FontError> {
    let mut pool = DonorPool::from_index(&library.index());
    if options.allow_download {
        for family in download_candidates(subset, library) {
            if library.fetch_preview(&family).is_err() {
                continue;
            }
            let Some(path) = library.preview_font_file(&family) else {
                continue;
            };
            let key = format!("downloadable:{}", path.display()).to_lowercase();
            if let Ok(bytes) = std::fs::read(&path) {
                if let Some(donor) = DonorFile::from_bytes(Source::Downloadable, &key, bytes, 0) {
                    pool.push(donor);
                }
            }
        }
    }
    complete_font(subset, text, &pool, options)
}

/// the online families worth fetching: the known twin of the font the
/// document names, then its look-alikes, at most three.
fn download_candidates(subset: &SubsetFont, library: &FontLibrary) -> Vec<String> {
    let Some(hint) = subset.name_hint.as_deref() else {
        return Vec::new();
    };
    let index = library.index();
    let mut names: Vec<String> = Vec::new();
    if let Some(twin) = crate::substitute::substitute_for(hint) {
        names.push(twin.with.to_string());
    }
    for similar in library.similar(hint, 6) {
        names.push(similar.family);
    }
    names.retain(|name| {
        index
            .family(name)
            .is_some_and(|family| !family.is_on_this_computer() && family.catalogue.is_some())
    });
    names.dedup();
    names.truncate(3);
    names
}
