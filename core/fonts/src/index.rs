//! one index of every font, whatever its source: the faces on this
//! computer (system, added, bundled, downloaded) grouped into families,
//! and the online library's families that aren't downloaded yet.
//!
//! a family is identified by its name, so "Inter" installed on the system
//! and "Inter" in the online library are one family with two sources.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::catalogue::{Catalogue, CatalogueFamily};
use crate::coverage::{ranges_contain, subsets_contain};
use crate::model::{Category, Embedding, FaceRecord, Source};
use crate::scan::license_name;

/// the key two names of one family share: lower case, single spaces.
pub fn family_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// one style of a family, available now or after a download.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub name: String,
    pub weight: u16,
    pub italic: bool,
    /// the face that draws it, when it is on this computer
    pub face: Option<String>,
}

/// one family in the index.
#[derive(Clone, Debug, PartialEq)]
pub struct Family {
    pub name: String,
    pub key: String,
    /// faces on this computer, regular first
    pub faces: Vec<FaceRecord>,
    /// the online library entry with the same name, if any
    pub catalogue: Option<&'static CatalogueFamily>,
}

impl Family {
    /// the sources of the family, most local first. an online family that
    /// has been downloaded counts as Library, not Downloadable.
    pub fn sources(&self) -> Vec<Source> {
        let mut sources: Vec<Source> = self.faces.iter().map(|face| face.source).collect();
        if self.catalogue.is_some() && !sources.contains(&Source::Library) && self.faces.is_empty()
        {
            sources.push(Source::Downloadable);
        }
        sources.sort();
        sources.dedup();
        sources
    }

    pub fn is_on_this_computer(&self) -> bool {
        !self.faces.is_empty()
    }

    /// the face nearest to regular upright, which stands for the family.
    pub fn regular(&self) -> Option<&FaceRecord> {
        self.faces.first()
    }

    pub fn category(&self) -> Category {
        match (self.catalogue, self.regular()) {
            (Some(entry), _) => entry.category(),
            (None, Some(face)) => face.facts.category,
            (None, None) => Category::SansSerif,
        }
    }

    pub fn monospace(&self) -> bool {
        self.category() == Category::Monospace
            || self.regular().is_some_and(|face| face.facts.monospace)
    }

    pub fn variable(&self) -> bool {
        self.faces.iter().any(|face| !face.facts.axes.is_empty())
            || self.catalogue.is_some_and(|entry| !entry.axes.is_empty())
    }

    /// every style, from the faces on disk or else from the catalogue.
    pub fn styles(&self) -> Vec<Style> {
        if !self.faces.is_empty() {
            return self
                .faces
                .iter()
                .map(|face| Style {
                    name: face.facts.style_name.clone(),
                    weight: face.facts.weight,
                    italic: face.facts.italic || face.facts.oblique,
                    face: Some(face.key.clone()),
                })
                .collect();
        }
        self.catalogue
            .map(|entry| {
                entry
                    .files
                    .iter()
                    .map(|file| Style {
                        name: style_label(
                            file.weight,
                            file.style == "italic",
                            !entry.axes.is_empty(),
                        ),
                        weight: file.weight,
                        italic: file.style == "italic",
                        face: None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// a variable font draws every weight in its range from one file.
    fn weight_range(&self) -> (u16, u16) {
        let mut low = u16::MAX;
        let mut high = 0;
        for face in &self.faces {
            low = low.min(face.facts.weight);
            high = high.max(face.facts.weight);
            for axis in face.facts.axes.iter().filter(|axis| axis.tag == "wght") {
                low = low.min(axis.min as u16);
                high = high.max(axis.max as u16);
            }
        }
        if let Some(entry) = self.catalogue.filter(|_| self.faces.is_empty()) {
            for file in &entry.files {
                low = low.min(file.weight);
                high = high.max(file.weight);
            }
            for axis in entry.axes.iter().filter(|axis| axis.tag == "wght") {
                low = low.min(axis.min as u16);
                high = high.max(axis.max as u16);
            }
        }
        (low, high)
    }

    pub fn has_bold(&self) -> bool {
        self.weight_range().1 >= 600
    }

    pub fn has_italic(&self) -> bool {
        self.styles().iter().any(|style| style.italic)
    }

    /// the number of styles a person can pick; a variable weight axis
    /// counts each hundred it spans, as menus show them.
    pub fn style_count(&self) -> usize {
        let named = self.styles().len();
        let (low, high) = self.weight_range();
        if self.variable() && high > low {
            let weights = usize::from((high - low) / 100 + 1);
            let slants = if self.has_italic() { 2 } else { 1 };
            named.max(weights * slants)
        } else {
            named
        }
    }

    /// writing systems drawn by the regular face, or the catalogue's.
    pub fn writing(&self) -> Vec<String> {
        match (self.regular(), self.catalogue) {
            (Some(face), _) => face.facts.writing.clone(),
            (None, Some(entry)) => entry.writing(),
            (None, None) => Vec::new(),
        }
    }

    /// what embedding the regular face allows; open library fonts allow all.
    pub fn embedding(&self) -> Embedding {
        self.regular()
            .map(|face| face.facts.embedding.clone())
            .unwrap_or(Embedding::OPEN)
    }

    /// the licence, for library, bundled and added open fonts.
    pub fn license(&self) -> Option<String> {
        if let Some(license) = self.faces.iter().find_map(|face| face.license.clone()) {
            return Some(license);
        }
        let from_library = self.faces.is_empty()
            || self
                .faces
                .iter()
                .any(|face| matches!(face.source, Source::Library | Source::Bundled));
        self.catalogue
            .filter(|_| from_library)
            .map(|entry| license_name(&entry.license).to_string())
    }

    /// whether the family can draw `c`: exactly for faces on disk, and
    /// probably (from its subsets) for one not downloaded.
    pub fn can_draw(&self, c: char) -> bool {
        match (self.regular(), self.catalogue) {
            (Some(face), _) => ranges_contain(&face.facts.coverage, c),
            (None, Some(entry)) => subsets_contain(&entry.subsets, c),
            (None, None) => false,
        }
    }

    pub fn download_bytes(&self) -> Option<u64> {
        self.catalogue
            .filter(|_| self.faces.is_empty())
            .map(|entry| entry.download_bytes())
    }
}

/// "Bold Italic" from a weight and a slant, as the library names them.
pub fn style_label(weight: u16, italic: bool, variable: bool) -> String {
    let weight_name = if variable {
        "Variable"
    } else {
        match weight {
            0..=149 => "Thin",
            150..=249 => "Extra Light",
            250..=349 => "Light",
            350..=449 => "Regular",
            450..=549 => "Medium",
            550..=649 => "Semibold",
            650..=749 => "Bold",
            750..=849 => "Extra Bold",
            _ => "Black",
        }
    };
    match (weight_name, italic) {
        ("Regular", true) => "Italic".to_string(),
        (name, true) => format!("{name} Italic"),
        (name, false) => name.to_string(),
    }
}

/// all families, sorted by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontIndex {
    pub families: Vec<Family>,
}

/// how close a face is to regular upright: smaller is closer.
fn regularity(face: &FaceRecord) -> (bool, u16, u16, String) {
    let facts = &face.facts;
    (
        facts.italic || facts.oblique,
        (i32::from(facts.width) - 5).unsigned_abs() as u16,
        (i32::from(facts.weight) - 400).unsigned_abs() as u16,
        facts.style_name.clone(),
    )
}

impl FontIndex {
    /// groups faces into families and adds the online library. when one
    /// face is found twice (the same font installed and bundled), the more
    /// local copy wins and the other is dropped.
    pub fn build(records: Vec<FaceRecord>, catalogue: &'static Catalogue) -> FontIndex {
        let mut groups: BTreeMap<String, Family> = BTreeMap::new();
        for record in records {
            let key = family_key(&record.facts.family);
            let family = groups.entry(key.clone()).or_insert_with(|| Family {
                name: record.facts.family.trim().to_string(),
                key,
                faces: Vec::new(),
                catalogue: None,
            });
            let duplicate = family.faces.iter().position(|face| {
                face.facts.style_name == record.facts.style_name
                    && face.facts.postscript_name == record.facts.postscript_name
            });
            match duplicate {
                Some(at) if family.faces[at].source <= record.source => {}
                Some(at) => family.faces[at] = record,
                None => family.faces.push(record),
            }
        }
        for entry in &catalogue.families {
            let key = family_key(&entry.name);
            groups
                .entry(key.clone())
                .or_insert_with(|| Family {
                    name: entry.name.clone(),
                    key,
                    faces: Vec::new(),
                    catalogue: None,
                })
                .catalogue = Some(entry);
        }
        let mut families: Vec<Family> = groups.into_values().collect();
        for family in &mut families {
            family.faces.sort_by_key(regularity);
        }
        FontIndex { families }
    }

    pub fn family(&self, name: &str) -> Option<&Family> {
        let key = family_key(name);
        self.families
            .binary_search_by(|family| family.key.as_str().cmp(&key))
            .ok()
            .map(|at| &self.families[at])
    }

    /// the face with this key, wherever it is.
    pub fn face(&self, key: &str) -> Option<&FaceRecord> {
        self.families
            .iter()
            .flat_map(|family| &family.faces)
            .find(|face| face.key == key)
    }

    /// the face of a family closest to the asked weight and slant.
    pub fn closest_face(&self, family: &str, weight: u16, italic: bool) -> Option<&FaceRecord> {
        self.family(family)?.faces.iter().min_by_key(|face| {
            let slant_differs = (face.facts.italic || face.facts.oblique) != italic;
            let covers = face.facts.axes.iter().any(|axis| {
                axis.tag == "wght" && axis.min <= f32::from(weight) && f32::from(weight) <= axis.max
            });
            let distance = if covers {
                0
            } else {
                (i32::from(face.facts.weight) - i32::from(weight)).unsigned_abs()
            };
            (
                slant_differs,
                distance,
                (i32::from(face.facts.width) - 5).unsigned_abs(),
            )
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{FaceFacts, Measurements, Outlines};
    use std::path::PathBuf;

    /// a face record for tests, with made-up facts.
    pub fn record(
        family: &str,
        style: &str,
        weight: u16,
        italic: bool,
        source: Source,
    ) -> FaceRecord {
        let path = PathBuf::from(format!("/fonts/{family}-{style}.ttf"));
        FaceRecord {
            key: FaceRecord::make_key(source, &path, 0),
            source,
            path,
            index: 0,
            license: None,
            facts: FaceFacts {
                family: family.to_string(),
                style_name: style.to_string(),
                full_name: format!("{family} {style}"),
                postscript_name: format!("{family}-{style}").replace(' ', ""),
                weight,
                width: 5,
                italic,
                oblique: false,
                monospace: false,
                axes: Vec::new(),
                writing: vec!["latin".into()],
                coverage: vec![(0x20, 0x7e)],
                category: Category::SansSerif,
                embedding: Embedding::OPEN,
                version: "1.0".into(),
                glyph_count: 100,
                outlines: Outlines::TrueType,
                measurements: Measurements {
                    units_per_em: 1000,
                    ascender: 800,
                    descender: -200,
                    x_height: 500,
                    cap_height: 700,
                    panose: [0; 10],
                    ascii_widths: vec![500; 95],
                    contrast: 1.2,
                    stem: 90.0,
                },
            },
        }
    }

    #[test]
    fn faces_group_into_families_with_regular_first() {
        let catalogue = Catalogue::shipped();
        let index = FontIndex::build(
            vec![
                record("Arial", "Bold", 700, false, Source::System),
                record("Arial", "Regular", 400, false, Source::System),
                record("Arial", "Italic", 400, true, Source::System),
                record("My  Hand", "Regular", 400, false, Source::Custom),
            ],
            catalogue,
        );
        let arial = index.family("arial").unwrap();
        assert_eq!(arial.faces[0].facts.style_name, "Regular");
        assert!(arial.has_bold() && arial.has_italic());
        assert_eq!(arial.style_count(), 3);
        assert_eq!(arial.sources(), vec![Source::System]);
        assert_eq!(index.family("my hand").unwrap().name, "My  Hand");
        // the online library is in the same index
        let lobster = index.family("Lobster").unwrap();
        assert_eq!(lobster.sources(), vec![Source::Downloadable]);
        assert!(lobster.download_bytes().unwrap() > 0);
        assert_eq!(
            lobster.license().as_deref(),
            Some("SIL Open Font License 1.1")
        );
        assert_eq!(
            index
                .closest_face("Arial", 700, false)
                .unwrap()
                .facts
                .style_name,
            "Bold"
        );
        assert_eq!(
            index
                .closest_face("Arial", 400, true)
                .unwrap()
                .facts
                .style_name,
            "Italic"
        );
    }

    #[test]
    fn the_more_local_copy_of_a_face_wins() {
        let index = FontIndex::build(
            vec![
                record("Carlito", "Regular", 400, false, Source::Bundled),
                record("Carlito", "Regular", 400, false, Source::System),
            ],
            Catalogue::shipped(),
        );
        let carlito = index.family("Carlito").unwrap();
        assert_eq!(carlito.faces.len(), 1);
        assert_eq!(carlito.faces[0].source, Source::System);
        assert!(!carlito.sources().contains(&Source::Downloadable));
    }

    #[test]
    fn style_labels() {
        assert_eq!(style_label(400, false, false), "Regular");
        assert_eq!(style_label(400, true, false), "Italic");
        assert_eq!(style_label(700, true, false), "Bold Italic");
        assert_eq!(style_label(400, false, true), "Variable");
    }
}
