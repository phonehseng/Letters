//! the font library (letters-fonts) behind the engine's `FontSource`.
//!
//! letters-fonts knows every font Letters can use: the computer's, the
//! person's added fonts and the open fonts that come with the app. it
//! also knows which open fonts have the same letter widths as the common
//! office fonts (Carlito for Calibri, Arimo for Arial, Tinos for Times
//! New Roman, Cousine for Courier New) and which only look alike (Caladea
//! for Cambria: measured by the font library at 2.5 % of the em off on
//! average, so lines break differently). this module turns that into
//! face lookups for layout:
//!
//! - a family is found by its typographic name and weight, so "Calibri
//!   Light" (a legacy family name word stores) is Calibri at weight 300;
//! - a missing family gets its metric-compatible stand-in first, then a
//!   look-alike, and the substitution is recorded with its reason;
//! - a character the face cannot draw follows the font library's
//!   fallback chain for its writing system.
//!
//! variable fonts (Arimo comes as one) are laid out at the asked weight:
//! the face is loaded with that instance's advances.

use std::path::{Path, PathBuf};

use letters_fonts::{FaceRecord, FontIndex};

use super::{FaceLocation, FontSource};

/// fonts from the font library's index.
pub struct LibraryFonts {
    index: FontIndex,
}

/// weight words at the end of legacy family names ("Segoe UI Semibold").
const WEIGHT_WORDS: &[(&str, u16)] = &[
    ("thin", 100),
    ("hairline", 100),
    ("extralight", 200),
    ("ultralight", 200),
    ("extra light", 200),
    ("light", 300),
    ("semilight", 350),
    ("semi light", 350),
    ("book", 400),
    ("medium", 500),
    ("semibold", 600),
    ("semi bold", 600),
    ("demibold", 600),
    ("demi bold", 600),
    ("extrabold", 800),
    ("extra bold", 800),
    ("ultrabold", 800),
    ("black", 900),
    ("heavy", 900),
];

/// the most a font file may be when it is read for its facts
const FACT_LIMIT: u64 = 128 << 20;

impl LibraryFonts {
    /// an index over every font file in `folders` (each read once).
    pub fn from_folders(folders: &[PathBuf]) -> LibraryFonts {
        let mut records = Vec::new();
        for file in letters_fonts::scan::find_font_files(folders) {
            let source = letters_fonts::Source::Bundled;
            for (index, facts) in letters_fonts::scan::read_file_faces(&file.path, FACT_LIMIT) {
                records.push(FaceRecord {
                    key: FaceRecord::make_key(source, &file.path, index),
                    source,
                    path: file.path.clone(),
                    index,
                    facts,
                    license: None,
                });
            }
        }
        LibraryFonts::from_records(records)
    }

    /// an index over face records the caller already has (the app's
    /// font library hands its own index over this way).
    pub fn from_records(records: Vec<FaceRecord>) -> LibraryFonts {
        LibraryFonts {
            index: FontIndex::build(records, letters_fonts::Catalogue::shipped()),
        }
    }

    /// the open fonts that come with Letters only (vendor/fonts while
    /// developing): what a computer without Microsoft's fonts can use.
    pub fn bundled_only(folder: &Path) -> LibraryFonts {
        LibraryFonts::from_folders(&[folder.to_path_buf()])
    }

    /// the computer's fonts and the bundled ones.
    pub fn system_and_bundled(bundled: Option<&Path>) -> LibraryFonts {
        let mut folders = letters_fonts::scan::system_font_folders();
        folders.extend(bundled.map(Path::to_path_buf));
        LibraryFonts::from_folders(&folders)
    }

    pub fn index(&self) -> &FontIndex {
        &self.index
    }

    /// a face of a family the index has on this computer, at a weight.
    fn face(&self, family: &str, weight: u16, italic: bool) -> Option<&FaceRecord> {
        let found = self.index.family(family)?;
        if !found.is_on_this_computer() {
            return None;
        }
        self.index.closest_face(family, weight, italic)
    }
}

/// splits "Calibri Light" into ("Calibri", 300); a name with no weight
/// word stays whole at 400.
pub fn split_weight(family: &str) -> (String, u16) {
    let lower = family.trim().to_lowercase();
    for (word, weight) in WEIGHT_WORDS {
        if let Some(stem) = lower.strip_suffix(word) {
            if stem.ends_with(' ') && stem.trim().len() > 1 {
                let length = stem.trim_end().len();
                return (family.trim()[..length].to_string(), *weight);
            }
        }
    }
    (family.trim().to_string(), 400)
}

/// the font library's substitution row for a family: its own row (the
/// library says Calibri Light has no metric twin), else its stem's.
fn substitute_row(family: &str) -> Option<&'static letters_fonts::substitute::Known> {
    letters_fonts::substitute::substitute_for(family)
        .or_else(|| letters_fonts::substitute::substitute_for(&split_weight(family).0))
}

/// the weight a request asks for: the family's own weight word, or bold.
fn wanted_weight(base: u16, bold: bool) -> u16 {
    if bold {
        base.max(700)
    } else {
        base
    }
}

fn location_of(record: &FaceRecord, weight: u16) -> FaceLocation {
    let variable = record
        .facts
        .axes
        .iter()
        .find(|axis| axis.tag == "wght")
        .filter(|axis| (axis.default - f32::from(weight)).abs() > 0.5)
        .map(|axis| f32::from(weight).clamp(axis.min, axis.max));
    FaceLocation::File {
        path: record.path.clone(),
        index: record.index,
        weight: variable,
    }
}

impl FontSource for LibraryFonts {
    fn find(&self, family: &str, bold: bool, italic: bool) -> Option<FaceLocation> {
        // the whole name first: some families really are called "... Light"
        if let Some(record) = self.face(family, wanted_weight(400, bold), italic) {
            return Some(location_of(record, wanted_weight(400, bold)));
        }
        let (stem, weight) = split_weight(family);
        if weight == 400 {
            return None;
        }
        // "Calibri Light" bold is drawn from Calibri Light, emboldened
        let record = self.face(&stem, weight, italic)?;
        Some(location_of(record, weight))
    }

    fn metric_compatible(&self, family: &str) -> Vec<String> {
        substitute_row(family)
            .filter(|row| row.metric_compatible)
            .map(|row| row.with.to_string())
            .into_iter()
            .chain(
                super::metric_compatible_substitutes(family)
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect()
    }

    fn similar(&self, family: &str) -> Vec<String> {
        substitute_row(family)
            .filter(|row| !row.metric_compatible)
            .map(|row| row.with.to_string())
            .into_iter()
            .collect()
    }

    fn fallback_families(&self, ch: char) -> Vec<String> {
        let mut families: Vec<String> = letters_fonts::fallback_for(&self.index, "", ch)
            .into_iter()
            .collect();
        families.extend(super::default_fallbacks(ch).iter().map(|s| s.to_string()));
        families
    }

    fn last_resort(&self) -> Vec<String> {
        // the generic open fonts the library would offer, then the usual ones
        ["Tinos", "Noto Serif", "Arimo", "Noto Sans"]
            .iter()
            .map(|s| s.to_string())
            .chain(super::LAST_RESORT.iter().map(|s| s.to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_family_names_carry_their_weight() {
        assert_eq!(split_weight("Calibri Light"), ("Calibri".to_string(), 300));
        assert_eq!(
            split_weight("Segoe UI Semibold"),
            ("Segoe UI".to_string(), 600)
        );
        assert_eq!(split_weight("Arial Black"), ("Arial".to_string(), 900));
        assert_eq!(
            split_weight("Times New Roman"),
            ("Times New Roman".to_string(), 400)
        );
        // a name that is only a weight word is not split
        assert_eq!(split_weight("Light"), ("Light".to_string(), 400));
    }

    #[test]
    fn bold_asks_for_at_least_seven_hundred() {
        assert_eq!(wanted_weight(400, true), 700);
        assert_eq!(wanted_weight(300, false), 300);
        assert_eq!(wanted_weight(900, true), 900);
    }

    #[test]
    fn calibri_has_a_metric_twin_and_cambria_only_a_look_alike() {
        let fonts = LibraryFonts::from_records(Vec::new());
        assert!(fonts
            .metric_compatible("Calibri")
            .contains(&"Carlito".to_string()));
        // carlito has no light weight: only a look-alike for calibri light
        assert!(!fonts
            .metric_compatible("Calibri Light")
            .contains(&"Carlito".to_string()));
        assert!(fonts
            .similar("Calibri Light")
            .contains(&"Carlito".to_string()));
        // caladea is offered as similar only (font library measurement)
        assert!(
            !letters_fonts::substitute::substitute_for("Cambria")
                .unwrap()
                .metric_compatible
        );
        assert!(fonts.similar("Cambria").contains(&"Caladea".to_string()));
    }
}
