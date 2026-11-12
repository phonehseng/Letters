//! new text in the document's own look.
//!
//! a pdf embeds only the glyphs it draws, so a person who types a letter
//! the pdf never used would get word's fallback font. the easy to edit mode
//! therefore completes every rebuilt face before its grid fonts are made:
//! the original glyphs stay untouched (and keep their glyph numbers), and
//! every missing char of the wanted ranges (latin-1, latin extended-a,
//! basic punctuation, digits and currency signs) is added:
//!
//! - an accented letter whose base letter the face has is composed from
//!   the face's own base letter and the donor's accent;
//! - anything else is borrowed from the donor, scaled so its advances and
//!   heights match the face's (fitted on the letters both have).
//!
//! the donor is a `GlyphDonor`. `letters-fonts` has no completion api yet
//! (`core/fonts/src` has no `complete` module on 2026-11-12), so the one
//! donor here is round one's bundled stand-in for the face (arimo, tinos or
//! cousine, sil ofl 1.1). how close it looks is measured: the mean overlap
//! (intersection over union, rasterised at 64 px per em) of the letters
//! and digits both fonts have, after the fit.
//!
//! advances are snapped to the pixel grid with every other advance when the
//! grid fonts are built (recipe 4), so typed text lays out on the same grid.

use std::collections::BTreeMap;

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use super::overlap::{overlap, raster};
use crate::fonts::outline::{to_contours, ContourPoint, Contours, Recorder};
use crate::fonts::rebuild::RebuiltFace;
use crate::fonts::standins::{bundled, stand_in_for};
use crate::fonts::truetype::FontGlyph;

/// below this mean overlap the person is told the look-alike is weak.
pub const WEAK_SIMILARITY: f64 = 0.6;

/// the chars every completed face covers.
pub fn wanted_chars() -> Vec<char> {
    let ranges: [(u32, u32); 7] = [
        (0x20, 0x7E),     // basic latin: digits, letters, punctuation
        (0xA0, 0xFF),     // latin-1 supplement (with £ ¥ ¢ ¤)
        (0x100, 0x17F),   // latin extended-a
        (0x2010, 0x2027), // dashes, quotes, bullet, ellipsis
        (0x2030, 0x203A), // per mille, primes, single guillemets
        (0x20A0, 0x20BF), // currency signs (€ ₹ ₽ ...)
        (0x2122, 0x2122), // trade mark
    ];
    ranges
        .iter()
        .flat_map(|&(low, high)| (low..=high).filter_map(char::from_u32))
        .collect()
}

/// where glyphs for missing chars come from.
pub trait GlyphDonor {
    /// a short name for the report
    fn name(&self) -> String;
    fn units_per_em(&self) -> i64;
    /// the outline and advance of `char` in the donor's own units
    fn glyph(&self, char: char) -> Option<(Contours, i64)>;
}

/// one of the bundled stand-in fonts as a donor.
pub struct StandInDonor {
    face: ttf_parser::Face<'static>,
    file: &'static str,
}

impl StandInDonor {
    fn from_bundled(bytes: &'static [u8], file: &'static str) -> Self {
        Self {
            face: ttf_parser::Face::parse(bytes, 0).expect("the bundled stand-in fonts are valid"),
            file,
        }
    }

    /// the stand-in round one would give this face (recipe 3.8).
    pub fn for_face(pdf_font: &str, serif: bool) -> Self {
        let stand_in = stand_in_for(pdf_font, serif);
        Self::from_bundled(stand_in.bytes, stand_in.file)
    }

    /// the bundled stand-in whose glyphs overlap the face's best; round
    /// one's choice by name when they share too few letters to compare.
    pub fn most_alike(face: &RebuiltFace, serif: bool) -> Self {
        let mut best: Option<(f64, Self)> = None;
        for (bytes, file) in bundled() {
            let donor = Self::from_bundled(bytes, file);
            let shared = shared_chars(face, &donor);
            if shared.len() < MIN_SHARED_FOR_CHOICE {
                continue;
            }
            let score = similarity(face, &donor, &shared, &fit(face, &donor, &shared));
            if best.as_ref().is_none_or(|(top, _)| score > *top) {
                best = Some((score, donor));
            }
        }
        match best {
            Some((_, donor)) => donor,
            None => Self::for_face(&face.pdf_font, serif),
        }
    }
}

/// a donor is only chosen by its looks when at least this many letters and
/// digits can be compared; fewer say little about a design.
const MIN_SHARED_FOR_CHOICE: usize = 5;

impl GlyphDonor for StandInDonor {
    fn name(&self) -> String {
        self.file.to_string()
    }

    fn units_per_em(&self) -> i64 {
        i64::from(self.face.units_per_em())
    }

    fn glyph(&self, char: char) -> Option<(Contours, i64)> {
        let id = self.face.glyph_index(char)?;
        let mut recorder = Recorder::default();
        self.face.outline_glyph(id, &mut recorder);
        let advance = i64::from(self.face.glyph_hor_advance(id)?);
        Some((to_contours(&recorder.outline, 1.0, 1.0, false), advance))
    }
}

/// how one added glyph was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// the face's own base letter with the donor's accent
    Composed,
    /// the donor's glyph, scaled to the face
    Borrowed,
}

/// what completion did to one face.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionReport {
    pub pdf_font: String,
    pub donor: String,
    /// chars the face had already
    pub original: usize,
    pub composed: usize,
    pub borrowed: usize,
    /// wanted chars neither the face nor the donor has
    pub unavailable: Vec<char>,
    /// letters and digits both fonts have, used for the fit and the similarity
    pub shared: usize,
    /// mean overlap of the shared glyphs after the fit, 0 to 1 (none:
    /// no letters or digits in common to compare)
    pub similarity: Option<f64>,
    pub scale_x: f64,
    pub scale_y: f64,
    /// how each added char was obtained (for the report)
    pub added: BTreeMap<String, Origin>,
    /// the space's advance was set to the pdf's own space width
    pub space_from_pdf: bool,
    /// a plain sentence when the look-alike is weak
    pub warning: Option<String>,
}

/// a scaling from donor units to face units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale_x: f64,
    pub scale_y: f64,
}

impl Fit {
    fn apply(&self, contours: &Contours, shift: (f64, f64)) -> Contours {
        contours
            .iter()
            .map(|contour| {
                contour
                    .iter()
                    .map(|p| ContourPoint {
                        x: (f64::from(p.x) * self.scale_x + shift.0).round() as i32,
                        y: (f64::from(p.y) * self.scale_y + shift.1).round() as i32,
                        on_curve: p.on_curve,
                    })
                    .collect()
            })
            .collect()
    }
}

/// the box of some contours: (x min, y min, x max, y max).
pub fn bounds(contours: &Contours) -> Option<(i32, i32, i32, i32)> {
    let mut points = contours.iter().flatten();
    let first = points.next()?;
    Some(points.fold((first.x, first.y, first.x, first.y), |b, p| {
        (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y))
    }))
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

/// the letters and digits both fonts have.
fn shared_chars(face: &RebuiltFace, donor: &dyn GlyphDonor) -> Vec<char> {
    ('0'..='9')
        .chain('A'..='Z')
        .chain('a'..='z')
        .filter(|c| face.cmap.contains_key(&(*c as u32)) && donor.glyph(*c).is_some())
        .collect()
}

fn face_glyph(face: &RebuiltFace, char: char) -> Option<&FontGlyph> {
    face.cmap
        .get(&(char as u32))
        .and_then(|&g| face.glyphs.get(g as usize))
}

/// fits the donor to the face: horizontally by the median ratio of the
/// shared glyphs' advances, vertically by the median ratio of their
/// heights; the plain ratio of the ems when nothing is shared.
pub fn fit(face: &RebuiltFace, donor: &dyn GlyphDonor, shared: &[char]) -> Fit {
    let em = face.units_per_em as f64 / donor.units_per_em() as f64;
    let mut widths = Vec::new();
    let mut heights = Vec::new();
    for &char in shared {
        let (Some(own), Some((theirs, advance))) = (face_glyph(face, char), donor.glyph(char))
        else {
            continue;
        };
        if advance > 0 {
            widths.push(f64::from(own.advance) / advance as f64);
        }
        if let (Some(a), Some(b)) = (bounds(&own.contours), bounds(&theirs)) {
            if b.3 > 0 && a.3 > 0 {
                heights.push(f64::from(a.3) / f64::from(b.3));
            }
        }
    }
    Fit {
        scale_x: median(&mut widths).unwrap_or(em),
        scale_y: median(&mut heights).unwrap_or(em),
    }
}

/// the mean overlap of the shared glyphs, the donor's scaled by the fit and
/// both aligned at their left edges.
pub fn similarity(face: &RebuiltFace, donor: &dyn GlyphDonor, shared: &[char], fit: &Fit) -> f64 {
    let em = face.units_per_em as f64;
    let mut total = 0.0;
    let mut count = 0;
    for &char in shared {
        let (Some(own), Some((theirs, _))) = (face_glyph(face, char), donor.glyph(char)) else {
            continue;
        };
        let scaled = fit.apply(&theirs, (0.0, 0.0));
        let (Some(a), Some(b)) = (bounds(&own.contours), bounds(&scaled)) else {
            continue;
        };
        let aligned = Fit {
            scale_x: 1.0,
            scale_y: 1.0,
        }
        .apply(&scaled, (f64::from(a.0 - b.0), 0.0));
        total += overlap(&raster(&own.contours, em), &raster(&aligned, em));
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

/// combining marks drawn below their letter (the rest go above).
fn mark_below(mark: char) -> bool {
    matches!(
        mark,
        '\u{0323}' | '\u{0326}' | '\u{0327}' | '\u{0328}' | '\u{0331}'
    )
}

/// an accented letter from the face's base letter and the donor's marks:
/// each mark is centred on the base and moved by how much taller (or
/// lower) the face's base is than the donor's.
fn composed(
    face: &RebuiltFace,
    donor: &dyn GlyphDonor,
    fit: &Fit,
    char: char,
) -> Option<FontGlyph> {
    let mut parts = char.to_string().nfd().collect::<Vec<char>>().into_iter();
    let base = parts.next()?;
    let marks: Vec<char> = parts.collect();
    if marks.is_empty() {
        return None;
    }
    let own = face_glyph(face, base)?;
    let own_box = bounds(&own.contours)?;
    let (donor_base, _) = donor.glyph(base)?;
    let donor_box = bounds(&fit.apply(&donor_base, (0.0, 0.0)))?;
    let mut contours = own.contours.clone();
    for mark in marks {
        let (outline, _) = donor.glyph(mark)?;
        let scaled = fit.apply(&outline, (0.0, 0.0));
        let mark_box = bounds(&scaled)?;
        let centre_shift = f64::from(own_box.0 + own_box.2 - mark_box.0 - mark_box.2) / 2.0;
        let lift = if mark_below(mark) {
            f64::from(own_box.1 - donor_box.1)
        } else {
            f64::from(own_box.3 - donor_box.3)
        };
        contours.extend(
            Fit {
                scale_x: 1.0,
                scale_y: 1.0,
            }
            .apply(&scaled, (centre_shift, lift)),
        );
    }
    Some(FontGlyph {
        contours,
        advance: own.advance,
    })
}

/// the donor's glyph scaled to the face.
fn borrowed(donor: &dyn GlyphDonor, fit: &Fit, char: char) -> Option<FontGlyph> {
    let (outline, advance) = donor.glyph(char)?;
    Some(FontGlyph {
        contours: fit.apply(&outline, (0.0, 0.0)),
        advance: (advance as f64 * fit.scale_x).round().clamp(0.0, 65535.0) as u16,
    })
}

/// completes one face in place. `space_width` is the pdf's width of the
/// face's space in 1/1000 em, when the pdf drew one.
pub fn complete_face(
    face: &mut RebuiltFace,
    donor: &dyn GlyphDonor,
    space_width: Option<f64>,
) -> CompletionReport {
    let shared = shared_chars(face, donor);
    let fit = fit(face, donor, &shared);
    let mut report = CompletionReport {
        pdf_font: face.pdf_font.clone(),
        donor: donor.name(),
        shared: shared.len(),
        similarity: (!shared.is_empty()).then(|| similarity(face, donor, &shared, &fit)),
        scale_x: fit.scale_x,
        scale_y: fit.scale_y,
        ..CompletionReport::default()
    };
    // typed spaces should be as wide as the pdf's (the rebuilt space is a quarter em)
    if let (Some(width), Some(space)) = (space_width, face.glyphs.get_mut(1)) {
        space.advance = (width * face.units_per_em as f64 / 1000.0)
            .round()
            .clamp(0.0, 65535.0) as u16;
        report.space_from_pdf = true;
    }
    for char in wanted_chars() {
        if face.cmap.contains_key(&(char as u32)) {
            report.original += 1;
            continue;
        }
        let made = composed(face, donor, &fit, char)
            .map(|g| (g, Origin::Composed))
            .or_else(|| borrowed(donor, &fit, char).map(|g| (g, Origin::Borrowed)));
        let Some((glyph, origin)) = made else {
            report.unavailable.push(char);
            continue;
        };
        let id = face.glyphs.len() as u16;
        face.glyphs.push(glyph);
        face.cmap.insert(char as u32, id);
        match origin {
            Origin::Composed => report.composed += 1,
            Origin::Borrowed => report.borrowed += 1,
        }
        report.added.insert(char.to_string(), origin);
    }
    let weak = report.similarity.is_some_and(|s| s < WEAK_SIMILARITY);
    if report.borrowed > 0 && weak {
        report.warning = Some(format!(
            "New text typed in “{}” may look only roughly like the original where the PDF’s font lacks a character (the closest available look-alike, {}, matches its letters by {:.0}%).",
            face.pdf_font,
            donor.name().trim_end_matches(".ttf"),
            100.0 * report.similarity.unwrap_or(0.0)
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap as Map, HashMap};

    /// a face made of the donor's own glyphs for a few letters.
    fn face_from(donor: &StandInDonor, chars: &[char]) -> RebuiltFace {
        let mut glyphs = vec![
            FontGlyph {
                contours: Vec::new(),
                advance: 1024,
            },
            FontGlyph {
                contours: Vec::new(),
                advance: 512,
            },
        ];
        let mut cmap = Map::from([(0x20, 1u16)]);
        for &char in chars {
            let (contours, advance) = donor.glyph(char).unwrap();
            cmap.insert(char as u32, glyphs.len() as u16);
            glyphs.push(FontGlyph {
                contours,
                advance: advance as u16,
            });
        }
        RebuiltFace {
            face_key: "Test|truetype".into(),
            family: "Ltr 1".into(),
            pdf_font: "Test".into(),
            units_per_em: donor.units_per_em(),
            glyphs,
            cmap,
            char_for: HashMap::new(),
            ascent: 1800,
            descent: -400,
            fs_type: None,
            permission: "",
            cubic_source: false,
            outline_error_em: 0.0,
            private_use: 0,
            substitute_for: None,
            problems: Vec::new(),
        }
    }

    #[test]
    fn the_wanted_ranges_cover_latin_and_currency() {
        let chars = wanted_chars();
        for char in ['a', 'é', 'Ł', 'ž', '€', '£', '—', '…', '™'] {
            assert!(chars.contains(&char), "{char}");
        }
    }

    #[test]
    fn a_face_matches_itself_and_keeps_its_glyphs() {
        let donor = StandInDonor::for_face("Helvetica", false);
        let mut face = face_from(&donor, &['a', 'b', 'c', 'e', 'n', 'o', 'H', 'E']);
        let before: Vec<FontGlyph> = face.glyphs.clone();
        let report = complete_face(&mut face, &donor, Some(278.0));
        let alike = report.similarity.unwrap();
        assert!((alike - 1.0).abs() < 1e-9, "{alike}");
        assert!((report.scale_x - 1.0).abs() < 1e-9);
        assert_eq!(&face.glyphs[2..before.len()], &before[2..]);
        assert!(report.warning.is_none());
        // é from the face's own e; € borrowed
        assert_eq!(report.added.get("é"), Some(&Origin::Composed));
        assert_eq!(report.added.get("€"), Some(&Origin::Borrowed));
        assert!(face.cmap.contains_key(&('Ž' as u32)));
        // 278/1000 em of a 2048 em
        assert_eq!(face.glyphs[1].advance, 569);
    }

    #[test]
    fn a_different_design_measures_lower() {
        let serif = StandInDonor::for_face("Times-Roman", true);
        let sans = StandInDonor::for_face("Helvetica", false);
        let face = face_from(&serif, &['a', 'e', 'g', 'n', 'o', 'H', 'R']);
        let shared = shared_chars(&face, &sans);
        let fit = fit(&face, &sans, &shared);
        let score = similarity(&face, &sans, &shared, &fit);
        assert!(score < 0.95, "{score}");
        assert!(score > 0.3, "{score}");
    }

    #[test]
    fn the_most_alike_donor_is_chosen_by_its_looks() {
        let serif = StandInDonor::for_face("Times-Roman", true);
        let face = face_from(&serif, &['a', 'e', 'g', 'n', 'o', 'H', 'R']);
        // the pdf claims sans, but the letters are tinos'
        assert_eq!(
            StandInDonor::most_alike(&face, false).name(),
            "Tinos-Regular.ttf"
        );
    }
}
