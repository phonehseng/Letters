//! letters-fonts' completion as the source of glyphs a document's font
//! lacks: the same font installed on this computer, else a near font fitted
//! to imitate it, else letters built from the font's own, else the closest
//! font as it is. every extension says which way was used and how alike
//! it is, in one sentence for the person.
//!
//! the fonts that may lend glyphs belong to the app's font library, which
//! core/edit doesn't own. the app lends them for the length of one edit
//! with `with_font_donors`; without them, only the same font installed
//! here (found by its name) is asked. generated glyphs are never asked for:
//! `complete_font` is the call without a generation.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use letters_fonts::complete::report::{char_list, family_and_style, percent, source_words};
use letters_fonts::complete::{
    complete_font, CompleteOptions, CompletedFont, Strategy, SubsetFont,
};
use letters_fonts::matching::donors::{DonorFile, DonorPool};
use letters_fonts::model::Source;
use ttf_parser::Face;

use crate::fonts::donor::{outline_of, Outline, Target};
use crate::fonts::system;

/// the fonts that may lend glyphs to a document's font, and where fitted
/// fonts are remembered between edits. cheap to clone.
#[derive(Clone, Debug)]
pub struct FontDonors {
    pool: Arc<DonorPool>,
    cache_folder: Option<PathBuf>,
}

impl FontDonors {
    pub fn new(pool: DonorPool, cache_folder: Option<PathBuf>) -> FontDonors {
        FontDonors {
            pool: Arc::new(pool),
            cache_folder,
        }
    }

    fn options(&self) -> CompleteOptions {
        CompleteOptions {
            cache_folder: self.cache_folder.clone(),
            ..CompleteOptions::default()
        }
    }
}

thread_local! {
    /// the donors lent for the edit running on this thread
    static LENT: RefCell<Option<FontDonors>> = const { RefCell::new(None) };
}

/// puts the lent donors back as they were, even if the edit panics.
struct Restore(Option<FontDonors>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        LENT.with(|lent| *lent.borrow_mut() = previous);
    }
}

/// runs `work` (one or more edits) with `donors` lending glyphs to any font
/// the edits extend. a thread-local, so edits on other threads and other
/// tests are never affected.
pub fn with_font_donors<T>(donors: &FontDonors, work: impl FnOnce() -> T) -> T {
    let previous = LENT.with(|lent| lent.replace(Some(donors.clone())));
    let _restore = Restore(previous);
    work()
}

/// the donors for an edit: the lent ones, else the same font installed here.
pub(crate) fn donors_for(font_name: &str) -> Option<FontDonors> {
    if let Some(lent) = LENT.with(|lent| lent.borrow().clone()) {
        return Some(lent);
    }
    let found = system::find(font_name)?;
    let bytes = std::fs::read(&found.path).ok()?;
    let key = found.path.display().to_string().to_lowercase();
    let donor = DonorFile::from_bytes(Source::System, &key, bytes, found.index)?;
    Some(FontDonors::new(
        DonorPool {
            donors: vec![donor],
        },
        None,
    ))
}

/// donors made of one font file the app chose (Extend Font's look-alike).
pub(crate) fn donors_from_file(bytes: Vec<u8>) -> Option<FontDonors> {
    let donor = DonorFile::from_bytes(Source::Custom, "chosen look-alike font", bytes, 0)?;
    Some(FontDonors::new(
        DonorPool {
            donors: vec![donor],
        },
        None,
    ))
}

/// the document's font as the completion needs it.
pub(crate) struct DocumentFont<'a> {
    /// its TrueType program
    pub program: &'a [u8],
    /// which glyph draws each character it can draw
    pub glyphs: BTreeMap<char, u16>,
    /// its pdf widths, in thousandths of the em
    pub widths: BTreeMap<char, f64>,
    /// the pdf's name for it, without the subset tag
    pub name: &'a str,
}

/// what the person is told about glyphs added to a font.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MatchNote {
    /// the least faithful way any of them was made, e.g. "adjusted near font"
    pub method: String,
    /// how alike the least alike of them is, percent
    pub similarity: Option<f32>,
    pub sentence: String,
    /// where the outlines came from, e.g. "Arial on this computer"
    pub source: String,
}

/// the glyphs completion made for the characters asked for.
pub(crate) struct CompletedGlyphs {
    completed: CompletedFont,
}

/// asks for every character of `wanted`. none when completion fails or any
/// of them can't be drawn: then nothing is added and round one's fallback
/// fonts take over, with their own notices.
pub(crate) fn complete(
    font: &DocumentFont,
    wanted: &BTreeSet<char>,
    donors: &FontDonors,
) -> Option<CompletedGlyphs> {
    let subset = SubsetFont::from_font_bytes(font.program, 0, Some(&font.glyphs))
        .ok()?
        .with_pdf_widths(&font.widths)
        .with_name(font.name);
    let needed: String = wanted.iter().collect();
    let completed = complete_font(&subset, &needed, &donors.pool, &donors.options()).ok()?;
    let all_drawn = wanted.iter().all(|c| {
        completed
            .strategy_of(*c)
            .is_some_and(|s| s != Strategy::Missing)
            && completed.char_to_glyph.contains_key(c)
    });
    all_drawn.then_some(CompletedGlyphs { completed })
}

impl CompletedGlyphs {
    /// the outline of `character` in the target font's units.
    pub fn outline(&self, character: char, target: Target) -> Option<Outline> {
        let face = Face::parse(&self.completed.font, 0).ok()?;
        let glyph = *self.completed.char_to_glyph.get(&character)?;
        let scale = f32::from(target.units_per_em) / f32::from(face.units_per_em().max(1));
        Some(outline_of(&face, glyph, scale))
    }

    pub fn note(&self) -> MatchNote {
        let made = self.new_glyphs();
        let worst = made.iter().map(|glyph| glyph.strategy).max();
        let similarity = made
            .iter()
            .filter_map(|glyph| glyph.similarity)
            .fold(None, |lowest: Option<f64>, value| {
                Some(lowest.map_or(value, |lowest| lowest.min(value)))
            });
        MatchNote {
            method: worst.map(Strategy::label).unwrap_or_default().to_owned(),
            similarity: similarity.map(|value| value as f32),
            sentence: match_sentence(&made),
            source: donor_words(&made),
        }
    }

    /// the glyphs the completion really made (not the ones the font had).
    fn new_glyphs(&self) -> Vec<&letters_fonts::complete::GlyphProvenance> {
        self.completed
            .glyphs
            .iter()
            .filter(|glyph| glyph.strategy != Strategy::Original)
            .collect()
    }
}

type Provenance = letters_fonts::complete::GlyphProvenance;

/// "Arial on this computer", or "Tahoma and Verdana on this computer".
fn donor_words(made: &[&Provenance]) -> String {
    let mut names: Vec<String> = Vec::new();
    let mut place = "";
    for donor in made.iter().filter_map(|glyph| glyph.donor.as_ref()) {
        let name = family_and_style(&donor.family, &donor.style);
        if !names.contains(&name) {
            names.push(name);
        }
        place = source_words(donor.source);
    }
    if names.is_empty() {
        return "the font’s own letters".to_owned();
    }
    format!("{} {place}", names.join(" and "))
}

/// one sentence saying how the new letters were made and how alike they
/// are, grouped by way, most faithful first.
pub(crate) fn match_sentence(made: &[&Provenance]) -> String {
    let mut by_way: BTreeMap<Strategy, Vec<&Provenance>> = BTreeMap::new();
    for glyph in made {
        by_way.entry(glyph.strategy).or_default().push(glyph);
    }
    let clauses: Vec<String> = by_way
        .iter()
        .map(|(way, glyphs)| clause(*way, glyphs))
        .collect();
    if clauses.is_empty() {
        return String::new();
    }
    format!("New letters: {}.", clauses.join("; "))
}

fn clause(way: Strategy, glyphs: &[&Provenance]) -> String {
    let characters: Vec<char> = glyphs.iter().map(|glyph| glyph.character).collect();
    let letters = char_list(&characters);
    let donor = glyphs
        .iter()
        .find_map(|glyph| glyph.donor.as_ref())
        .map(|donor| family_and_style(&donor.family, &donor.style))
        .unwrap_or_default();
    let place = glyphs
        .iter()
        .find_map(|glyph| glyph.donor.as_ref())
        .map(|donor| source_words(donor.source))
        .unwrap_or_default();
    let alike = glyphs
        .iter()
        .filter_map(|glyph| glyph.similarity)
        .fold(f64::INFINITY, f64::min);
    let alike = if alike.is_finite() {
        format!(", {} alike", percent(alike))
    } else {
        String::new()
    };
    match way {
        Strategy::ExactDonor => format!("{letters} from {donor} {place}, the same font"),
        Strategy::FittedDonor => {
            format!("{letters} from {donor} {place}, adjusted to match{alike}")
        }
        Strategy::Composed => format!("{letters} built from this font’s own letters"),
        Strategy::LookAlike => {
            format!("{letters} from {donor} as it is, the closest font found{alike}")
        }
        Strategy::Generated => format!("{letters} drawn to match this font{alike}"),
        Strategy::Original | Strategy::Missing => letters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_fonts::complete::DonorNote;
    use letters_fonts::model::EmbeddingPermission;

    fn made(
        character: char,
        strategy: Strategy,
        family: &str,
        similarity: Option<f64>,
    ) -> Provenance {
        Provenance {
            character,
            glyph_id: Some(1),
            strategy,
            donor: (!family.is_empty()).then(|| DonorNote {
                family: family.into(),
                style: "Regular".into(),
                source: Source::System,
                permission: EmbeddingPermission::Installable,
                permission_sentence: String::new(),
            }),
            built_from: Vec::new(),
            similarity,
            sentence: String::new(),
        }
    }

    #[test]
    fn the_same_font_is_named_with_where_it_was_found() {
        let glyphs = [made('é', Strategy::ExactDonor, "Arial", Some(100.0))];
        let refs: Vec<&Provenance> = glyphs.iter().collect();
        assert_eq!(
            match_sentence(&refs),
            "New letters: “é” from Arial on this computer, the same font."
        );
    }

    #[test]
    fn mixed_ways_are_listed_most_faithful_first_with_how_alike() {
        let glyphs = [
            made('Q', Strategy::LookAlike, "Verdana", Some(71.04)),
            made('é', Strategy::Composed, "", None),
            made('€', Strategy::FittedDonor, "Tahoma", Some(93.12)),
        ];
        let refs: Vec<&Provenance> = glyphs.iter().collect();
        assert_eq!(
            match_sentence(&refs),
            "New letters: “€” from Tahoma on this computer, adjusted to match, 93.1% alike; \
             “é” built from this font’s own letters; \
             “Q” from Verdana as it is, the closest font found, 71.0% alike."
        );
    }

    #[test]
    fn lent_donors_last_only_for_the_work_they_were_lent_to() {
        let donors = FontDonors::new(DonorPool::default(), None);
        with_font_donors(&donors, || {
            let lent = LENT.with(|lent| lent.borrow().is_some());
            assert!(lent);
        });
        assert!(LENT.with(|lent| lent.borrow().is_none()));
    }
}
