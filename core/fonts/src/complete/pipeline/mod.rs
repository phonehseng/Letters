//! trying the strategies in order for each missing character, then
//! writing the completed font and its report. complete/mod.rs explains
//! the order and why.

use std::collections::{BTreeMap, BTreeSet};

use ttf_parser::{Face, Tag};

use super::cache::{cache_key, CompletionCache};
use super::generated::{read_reply, run_generation, style_score, Generation};
use super::marks::{decompose, letters_with, ligature_parts, superscript_base};
use super::report::{
    char_list, family_and_style, percent, source_words, DonorNote, FitSummary, GlyphProvenance,
    Identified, MatchReport, Strategy,
};
use super::synthesis::{compose_accented, compose_ligature, compose_superscript, Composed};
use super::truetype::{write_completed_font, FontToWrite};
use super::{CompleteOptions, CompletedFont, KernPair, SubsetFont};
use crate::error::FontError;
use crate::matching::donors::{quick_distance, DonorFile, DonorPool};
use crate::matching::fit::{fit_donor, FittedDonor};
use crate::matching::glyphs::{pair_kerning, read_glyphs, GlyphData, GlyphSet};
use crate::matching::identify::{identify_glyphs, Candidate, IdentifyOptions};
use crate::matching::style::{measure_style, StyleFit};

mod generation;
mod lenders;
mod sentences;
mod strategies;

// the parts share this module's imports through `use super::*`, and are
// used here and by each other without qualification
use generation::*;
use lenders::*;
use sentences::*;
use strategies::*;

/// one character's new glyph and how it was made.
#[derive(Clone, Debug)]
pub(crate) struct Made {
    glyph: Option<GlyphData>,
    strategy: Strategy,
    donor: Option<DonorNote>,
    built_from: Vec<char>,
    similarity: Option<f64>,
    sentence: String,
}

impl Made {
    fn missing(c: char) -> Made {
        Made {
            glyph: None,
            strategy: Strategy::Missing,
            donor: None,
            built_from: Vec::new(),
            similarity: None,
            sentence: format!("Nothing on this computer can draw “{c}”."),
        }
    }
}

pub(super) fn complete(
    subset: &SubsetFont,
    needed_text: &str,
    pool: &DonorPool,
    options: &CompleteOptions,
    generation: Option<&Generation>,
) -> Result<CompletedFont, FontError> {
    let needed = needed_chars(subset, needed_text);
    let style = measure_style(&subset.glyphs);
    let cache = options.cache_folder.as_deref().map(CompletionCache::new);
    let needed_text_key: String = needed.iter().collect();
    let key = cache_key(&[
        &subset.outline_hash(),
        &needed_text_key,
        &options_fingerprint(options),
        &pool_fingerprint(pool),
    ]);
    if generation.is_none() {
        if let Some(hit) = cache.as_ref().and_then(|cache| cache.load_completed(&key)) {
            return Ok(hit);
        }
    }
    let completed = complete_uncached(
        subset,
        &needed,
        &style,
        pool,
        options,
        generation,
        cache.as_ref(),
    )?;
    if generation.is_none() {
        if let Some(cache) = &cache {
            cache.store_completed(&key, &completed);
        }
    }
    Ok(completed)
}

/// the characters of the text the font can't draw, each once, in the
/// order they first appear.
fn needed_chars(subset: &SubsetFont, text: &str) -> Vec<char> {
    let mut seen = BTreeSet::new();
    text.chars()
        .filter(|c| !c.is_control() && !subset.has(*c))
        .filter(|c| seen.insert(*c))
        .collect()
}

fn options_fingerprint(options: &CompleteOptions) -> String {
    format!(
        "{}-{}-{}-{}",
        options.candidate_limit,
        options.fit_limit,
        options.prefer_composed_accents,
        options.allow_look_alike
    )
}

/// which fonts the pool holds: a new font installed means a new answer.
fn pool_fingerprint(pool: &DonorPool) -> String {
    let mut keys: Vec<&str> = pool.donors.iter().map(|donor| donor.key.as_str()).collect();
    keys.sort_unstable();
    cache_key(&keys)
}

/// the fonts compared and those that lend glyphs.
struct Lenders<'a> {
    candidates: Vec<Candidate>,
    exact: Option<ExactLender<'a>>,
    near: Vec<NearLender<'a>>,
}

/// licence bits for a completed font whose document font had no OS/2
/// table: editable embedding, which is what editing the document needs.
const UNKNOWN_FS_TYPE: u16 = 0x0008;

fn complete_uncached(
    subset: &SubsetFont,
    needed: &[char],
    style: &StyleFit,
    pool: &DonorPool,
    options: &CompleteOptions,
    generation: Option<&Generation>,
    cache: Option<&CompletionCache>,
) -> Result<CompletedFont, FontError> {
    let mut report = MatchReport::default();
    let mut notices = Vec::new();
    let lenders = find_lenders(subset, needed, style, pool, options, cache, &mut report);
    let mut made = make_all(subset, needed, style, &lenders, pool, options);
    if let Some(generation) = generation {
        try_generation(
            generation,
            subset,
            style,
            &lenders.near,
            &mut made,
            &mut notices,
        );
    }
    let kerning = lenders
        .exact
        .as_ref()
        .map(|exact| exact_kerning(exact, subset, &made))
        .unwrap_or_default();
    let new_glyphs: Vec<(char, GlyphData)> = needed
        .iter()
        .filter_map(|c| made.get(c)?.glyph.clone().map(|glyph| (*c, glyph)))
        .collect();
    let family_name = neutral_family(subset, &new_glyphs);
    let (font, char_to_glyph) = write_font(subset, &new_glyphs, &kerning, &family_name)?;
    if subset.truetype.is_none() && !subset.glyph_ids.is_empty() {
        notices.push("The document’s font isn’t TrueType, so its letters were redrawn as TrueType outlines (within a thousandth of the em) with new glyph numbers.".to_string());
    }
    let glyphs = provenance(needed, &made, &char_to_glyph);
    fill_report(&mut report, &glyphs, &lenders);
    notices.splice(0..0, report.sentences.iter().cloned());
    Ok(CompletedFont {
        font,
        family_name,
        char_to_glyph,
        new_glyphs: new_glyphs.into_iter().collect(),
        glyphs,
        kerning,
        style: style.clone(),
        report,
        notices,
        from_cache: false,
    })
}

/// identifies the font, then picks the same design (when allowed) and
/// fits the closest ones.
fn find_lenders<'a>(
    subset: &SubsetFont,
    needed: &[char],
    style: &StyleFit,
    pool: &'a DonorPool,
    options: &CompleteOptions,
    cache: Option<&CompletionCache>,
    report: &mut MatchReport,
) -> Lenders<'a> {
    if needed.is_empty() {
        return Lenders {
            candidates: Vec::new(),
            exact: None,
            near: Vec::new(),
        };
    }
    let identify_options = IdentifyOptions {
        candidate_limit: options.candidate_limit,
        solve_axes: true,
    };
    let candidates = identify_glyphs(&subset.glyphs, &subset.profile(), pool, &identify_options);
    // the ui shows a handful of alternatives, not the whole shortlist
    report.candidates = candidates.iter().take(5).cloned().collect();
    let wanted = lender_chars(needed);
    let exact = choose_exact(&candidates, pool, &wanted, report);
    let near = fit_near(
        subset,
        style,
        &candidates,
        pool,
        &wanted,
        exact.as_ref(),
        options,
        cache,
        report,
    );
    Lenders {
        candidates,
        exact,
        near,
    }
}

/// every needed character, letters that can't be built first so that
/// built ones (accented letters, ligatures) can use them.
fn make_all(
    subset: &SubsetFont,
    needed: &[char],
    style: &StyleFit,
    lenders: &Lenders,
    pool: &DonorPool,
    options: &CompleteOptions,
) -> BTreeMap<char, Made> {
    let mut made: BTreeMap<char, Made> = BTreeMap::new();
    let (plain, buildable): (Vec<char>, Vec<char>) =
        needed.iter().partition(|c| !is_buildable(**c));
    for c in plain.iter().chain(&buildable) {
        let letters = letters_so_far(subset, &made);
        let result = make_glyph(
            *c,
            subset,
            style,
            &letters,
            lenders.exact.as_ref(),
            &lenders.near,
            pool,
            options,
        );
        made.insert(*c, result);
    }
    made
}

/// the completed font and its character map; with nothing new and a
/// TrueType document font, that font as it was.
fn write_font(
    subset: &SubsetFont,
    new_glyphs: &[(char, GlyphData)],
    kerning: &[KernPair],
    family_name: &str,
) -> Result<(Vec<u8>, BTreeMap<char, u16>), FontError> {
    if new_glyphs.is_empty() {
        if let Some(original) = &subset.truetype {
            return Ok((original.clone(), subset.glyph_ids.clone()));
        }
    }
    let written = write_completed_font(&FontToWrite {
        base: subset.truetype.as_deref(),
        metrics: &subset.glyphs.metrics,
        original_ids: &subset.glyph_ids,
        original_glyphs: &subset.glyphs.glyphs,
        new_glyphs,
        kerning,
        family: family_name,
        fs_type: subset.fs_type.unwrap_or(UNKNOWN_FS_TYPE),
    })?;
    Ok((written.bytes, written.char_to_glyph))
}

/// how each needed character was drawn, in the order asked.
fn provenance(
    needed: &[char],
    made: &BTreeMap<char, Made>,
    char_to_glyph: &BTreeMap<char, u16>,
) -> Vec<GlyphProvenance> {
    needed
        .iter()
        .map(|c| {
            let m = &made[c];
            GlyphProvenance {
                character: *c,
                glyph_id: m.glyph.as_ref().and_then(|_| char_to_glyph.get(c).copied()),
                strategy: m.strategy,
                donor: m.donor.clone(),
                built_from: m.built_from.clone(),
                similarity: m.similarity,
                sentence: m.sentence.clone(),
            }
        })
        .collect()
}

fn fill_report(report: &mut MatchReport, glyphs: &[GlyphProvenance], lenders: &Lenders) {
    report.strategies = glyphs.iter().map(|g| (g.character, g.strategy)).collect();
    report.identified = identified(&lenders.candidates, lenders.exact.as_ref());
    report.sentences = sentences(
        glyphs,
        lenders.exact.as_ref(),
        &lenders.near,
        &lenders.candidates,
    );
    note_licences(glyphs, report);
}
