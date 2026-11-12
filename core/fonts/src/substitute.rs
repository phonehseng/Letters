//! what to use when a document asks for a font this computer doesn't
//! have: first a font with the same widths (so lines break in the same
//! places), else a similar open font, else a sensible fallback for the
//! writing system. every choice comes with a plain sentence.

use serde::Serialize;

use crate::coverage::WRITING;
use crate::index::{family_key, FontIndex};
use crate::model::FaceRecord;

/// one row of the substitution table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Known {
    pub missing: &'static str,
    pub with: &'static str,
    /// true when every character has the same width in both
    pub metric_compatible: bool,
}

const fn known(missing: &'static str, with: &'static str, metric_compatible: bool) -> Known {
    Known {
        missing,
        with,
        metric_compatible,
    }
}

/// the fonts documents most often ask for that open fonts can stand in
/// for. the metric-compatible ones were made for this: Carlito for
/// Microsoft's Calibri, Arimo, Tinos and Cousine (the croscore fonts) for
/// Arial, Times New Roman and Courier New, and Gelasio for Georgia. the
/// others only look alike.
///
/// Caladea was made to match Cambria, but the version in the online
/// library no longer does: measured against Windows 11's Cambria its
/// letters differ by 2.5% of the em on average (the figures most), so it
/// is offered as similar, not as having the same widths.
pub const TABLE: &[Known] = &[
    known("Calibri", "Carlito", true),
    known("Cambria", "Caladea", false),
    known("Arial", "Arimo", true),
    known("Helvetica", "Arimo", true),
    known("Liberation Sans", "Arimo", true),
    known("Times New Roman", "Tinos", true),
    known("Times", "Tinos", true),
    known("Liberation Serif", "Tinos", true),
    known("Courier New", "Cousine", true),
    known("Courier", "Cousine", true),
    known("Liberation Mono", "Cousine", true),
    known("Georgia", "Gelasio", true),
    known("Calibri Light", "Carlito", false),
    known("Helvetica Neue", "Arimo", false),
    known("Aptos", "Inter", false),
    known("Segoe UI", "Noto Sans", false),
    known("Verdana", "Noto Sans", false),
    known("Tahoma", "Noto Sans", false),
    known("Trebuchet MS", "Fira Sans", false),
    known("Consolas", "Inconsolata", false),
    known("Lucida Console", "Cousine", false),
    known("Comic Sans MS", "Comic Neue", false),
    known("Garamond", "EB Garamond", false),
    known("Book Antiqua", "EB Garamond", false),
    known("Baskerville", "Libre Baskerville", false),
    known("Baskerville Old Face", "Libre Baskerville", false),
    known("Franklin Gothic", "Libre Franklin", false),
    known("Franklin Gothic Book", "Libre Franklin", false),
    known("Futura", "Jost", false),
    known("Century Gothic", "Didact Gothic", false),
    known("Bodoni MT", "Bodoni Moda", false),
    known("Didot", "GFS Didot", false),
    known("Arial Black", "Archivo Black", false),
    known("Arial Narrow", "Archivo Narrow", false),
    known("Segoe Print", "Patrick Hand", false),
    known("Segoe Script", "Caveat", false),
];

/// the table row for a missing font, ignoring case and spacing.
pub fn substitute_for(missing: &str) -> Option<&'static Known> {
    let key = family_key(missing);
    TABLE.iter().find(|row| family_key(row.missing) == key)
}

/// what Letters does about a font a document asks for.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Substitution {
    pub asked: String,
    pub used: String,
    pub metric_compatible: bool,
    /// true when the substitute is on this computer now
    pub available: bool,
    /// true when the substitute can be downloaded from the online library
    pub downloadable: bool,
    pub sentence: String,
}

fn substitution(
    index: &FontIndex,
    asked: &str,
    used: &str,
    metric_compatible: bool,
) -> Substitution {
    let family = index.family(used);
    let available = family.is_some_and(|family| family.is_on_this_computer());
    let downloadable = !available && family.is_some_and(|family| family.catalogue.is_some());
    let used = family
        .map(|family| family.name.clone())
        .unwrap_or_else(|| used.to_string());
    let sentence = match (metric_compatible, available) {
        (true, true) => format!(
            "“{asked}” isn’t on this computer, so Letters uses {used}, which has the same letter widths. Lines break in the same places."
        ),
        (true, false) => format!(
            "“{asked}” isn’t on this computer. Download {used}, which has the same letter widths, so lines break in the same places."
        ),
        (false, true) => format!(
            "“{asked}” isn’t on this computer, so Letters uses {used}, a similar font. Lines may break in slightly different places."
        ),
        (false, false) => format!(
            "“{asked}” isn’t on this computer. Download {used}, a similar font; lines may break in slightly different places."
        ),
    };
    Substitution {
        asked: asked.to_string(),
        used,
        metric_compatible,
        available,
        downloadable,
        sentence,
    }
}

/// a font the document needs, and what Letters will do about it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    /// the face that will draw the text, when one is on this computer
    pub face: Option<FaceRecord>,
    /// set when the font asked for isn't the one used
    pub substitution: Option<Substitution>,
}

/// the face for a font a document asks for, with a substitute when the
/// font isn't here.
pub fn resolve(index: &FontIndex, family: &str, weight: u16, italic: bool) -> Resolution {
    if let Some(face) = index.closest_face(family, weight, italic) {
        return Resolution {
            face: Some(face.clone()),
            substitution: None,
        };
    }
    let substitution = match substitute_for(family) {
        Some(row) => substitution(index, family, row.with, row.metric_compatible),
        None => {
            let generic = GENERIC
                .iter()
                .find(|name| {
                    index
                        .family(name)
                        .is_some_and(|found| found.is_on_this_computer())
                })
                .copied()
                .unwrap_or(GENERIC[0]);
            substitution(index, family, generic, false)
        }
    };
    let face = index
        .closest_face(&substitution.used, weight, italic)
        .cloned();
    Resolution {
        face,
        substitution: Some(substitution),
    }
}

/// what stands in for a font nothing is known about: common sans fonts.
const GENERIC: [&str; 5] = ["Noto Sans", "Arimo", "Arial", "Segoe UI", "Helvetica"];

/// the fonts to try, in order, for text in a writing system the chosen
/// font can't draw. the open Noto fonts come first because they can be
/// embedded and downloaded; the system fonts follow.
pub fn fallback_chain(writing: &str) -> &'static [&'static str] {
    match writing {
        "arabic" => &[
            "Noto Sans Arabic",
            "Noto Naskh Arabic",
            "Segoe UI",
            "Arial",
            "Geeza Pro",
        ],
        "hebrew" => &["Noto Sans Hebrew", "Segoe UI", "Arial", "Arial Hebrew"],
        "devanagari" => &[
            "Noto Sans Devanagari",
            "Nirmala UI",
            "Mangal",
            "Kohinoor Devanagari",
        ],
        "bengali" => &["Noto Sans Bengali", "Nirmala UI", "Vrinda"],
        "tamil" => &["Noto Sans Tamil", "Nirmala UI", "Latha"],
        "thai" => &["Noto Sans Thai", "Leelawadee UI", "Tahoma", "Thonburi"],
        "armenian" => &["Noto Sans Armenian", "Segoe UI", "Arial"],
        "georgian" => &["Noto Sans Georgian", "Segoe UI", "Sylfaen"],
        "ethiopic" => &["Noto Sans Ethiopic", "Ebrima", "Nyala"],
        "japanese" => &[
            "Noto Sans JP",
            "Yu Gothic",
            "Meiryo",
            "Hiragino Sans",
            "MS Gothic",
        ],
        "korean" => &["Noto Sans KR", "Malgun Gothic", "Apple SD Gothic Neo"],
        "chinese-simplified" => &["Noto Sans SC", "Microsoft YaHei", "PingFang SC", "SimSun"],
        "chinese-traditional" => &[
            "Noto Sans TC",
            "Microsoft JhengHei",
            "PingFang TC",
            "MingLiU",
        ],
        "math" => &["Noto Sans Math", "Cambria Math", "STIX Two Math"],
        _ => &["Noto Sans", "Arimo", "Segoe UI", "Arial", "Helvetica"],
    }
}

/// the family on this computer that should draw `c` when `preferred`
/// can't: the fallback chain of its writing system, then any family that
/// has it. None when nothing here can.
pub fn fallback_for(index: &FontIndex, preferred: &str, c: char) -> Option<String> {
    let draws = |name: &str| {
        index
            .family(name)
            .filter(|family| family.is_on_this_computer() && family.can_draw(c))
            .map(|family| family.name.clone())
    };
    if let Some(name) = draws(preferred) {
        return Some(name);
    }
    let writing = WRITING
        .iter()
        .find(|writing| {
            writing.letters.contains(c)
                || crate::coverage::subsets_contain(&owned(writing.subsets), c)
        })
        .map(|writing| writing.id)
        .unwrap_or("latin");
    fallback_chain(writing)
        .iter()
        .find_map(|name| draws(name))
        .or_else(|| {
            index
                .families
                .iter()
                .find(|family| family.is_on_this_computer() && family.can_draw(c))
                .map(|family| family.name.clone())
        })
}

fn owned(subsets: &[&str]) -> Vec<String> {
    subsets.iter().map(|subset| subset.to_string()).collect()
}

/// one font a document uses that isn't on this computer.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFont {
    pub name: String,
    /// true when the font itself is in the online library
    pub downloadable: bool,
    pub substitution: Substitution,
}

/// which of a document's fonts are missing, and what to do about each.
pub fn missing_fonts(index: &FontIndex, names: &[String]) -> Vec<MissingFont> {
    let mut seen = Vec::new();
    names
        .iter()
        .filter(|name| !name.trim().is_empty())
        .filter(|name| {
            let key = family_key(name);
            let fresh = !seen.contains(&key);
            seen.push(key);
            fresh
        })
        .filter(|name| {
            !index
                .family(name)
                .is_some_and(|family| family.is_on_this_computer())
        })
        .map(|name| {
            let downloadable = index
                .family(name)
                .is_some_and(|family| family.catalogue.is_some());
            let substitution = resolve(index, name, 400, false)
                .substitution
                .unwrap_or_else(|| substitution(index, name, name, true));
            MissingFont {
                name: name.clone(),
                downloadable,
                substitution,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Catalogue;
    use crate::index::tests::record;
    use crate::model::Source;

    #[test]
    fn every_substitute_is_in_the_online_library() {
        let catalogue = Catalogue::shipped();
        for row in TABLE {
            assert!(
                catalogue.family(row.with).is_some(),
                "{} is not in the library",
                row.with
            );
        }
    }

    #[test]
    fn a_metric_twin_is_proposed_in_a_plain_sentence() {
        let index = FontIndex::build(
            vec![record("Carlito", "Regular", 400, false, Source::Bundled)],
            Catalogue::shipped(),
        );
        let resolution = resolve(&index, "Calibri", 400, false);
        let substitution = resolution.substitution.unwrap();
        assert_eq!(substitution.used, "Carlito");
        assert!(substitution.available);
        assert_eq!(
            substitution.sentence,
            "“Calibri” isn’t on this computer, so Letters uses Carlito, which has the same letter widths. Lines break in the same places."
        );
        assert_eq!(resolution.face.unwrap().facts.family, "Carlito");

        let cambria = resolve(&index, "cambria", 700, false).substitution.unwrap();
        assert_eq!(cambria.used, "Caladea");
        assert!(!cambria.available && cambria.downloadable);
        assert!(cambria
            .sentence
            .starts_with("“cambria” isn’t on this computer. Download Caladea"));
    }

    #[test]
    fn present_fonts_need_no_substitute() {
        let index = FontIndex::build(
            vec![record("Arial", "Regular", 400, false, Source::System)],
            Catalogue::shipped(),
        );
        assert!(resolve(&index, "Arial", 400, false).substitution.is_none());
        let missing = missing_fonts(
            &index,
            &[
                "Arial".into(),
                "Calibri".into(),
                "calibri".into(),
                "Unknown Face".into(),
            ],
        );
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].substitution.used, "Carlito");
        // nothing known: a common sans that is here
        assert_eq!(missing[1].substitution.used, "Arial");
    }

    #[test]
    fn fallbacks_follow_the_writing_system() {
        let mut arabic = record("Noto Sans Arabic", "Regular", 400, false, Source::Library);
        arabic.facts.coverage = vec![(0x0600, 0x06ff)];
        let index = FontIndex::build(
            vec![
                record("Arial", "Regular", 400, false, Source::System),
                arabic,
            ],
            Catalogue::shipped(),
        );
        assert_eq!(fallback_for(&index, "Arial", 'a').as_deref(), Some("Arial"));
        assert_eq!(
            fallback_for(&index, "Arial", 'ب').as_deref(),
            Some("Noto Sans Arabic")
        );
        assert_eq!(fallback_for(&index, "Arial", '中'), None);
    }
}
